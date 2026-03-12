//! Firecracker VMM Driver
//!
//! This module provides a driver for Firecracker microVMs using the
//! `shellwego-firecracker` crate which mirrors the Firecracker API.
//! Supports both KVM (hardware) and PVM (software) virtualization modes.

use anyhow::Context;
use shellwego_firecracker::models::{
    BootSource, CacheType, CpuTemplate, Drive, FirecrackerMetrics, InstanceState, IoEngine,
    MachineConfiguration, Metrics, NetworkInterface, SnapshotCreateParams, SnapshotLoadParams,
    SnapshotType,
};
use shellwego_firecracker::vmm::client::FirecrackerClient;
use std::path::{Path, PathBuf};

/// Firecracker API driver for a specific VM socket
#[derive(Clone)]
pub struct FirecrackerDriver {
    /// Path to the Firecracker binary
    binary: PathBuf,
    /// Path to the VM's Unix socket
    socket_path: Option<PathBuf>,
    /// Path to the metrics FIFO/file
    metrics_path: Option<PathBuf>,
    /// HTTP client over UDS
    client: Option<FirecrackerClient>,
    /// Virtualization mode (KVM or PVM)
    mode: Option<crate::VirtualizationMode>,
}

impl std::fmt::Debug for FirecrackerDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FirecrackerDriver")
            .field("binary", &self.binary)
            .field("socket_path", &self.socket_path)
            .field("metrics_path", &self.metrics_path)
            .field(
                "client",
                &if self.client.is_some() {
                    "Some(FirecrackerClient)"
                } else {
                    "None"
                },
            )
            .field("mode", &self.mode)
            .finish()
    }
}

impl FirecrackerDriver {
    /// Create a new Firecracker driver instance
    pub async fn new(binary: &PathBuf) -> anyhow::Result<Self> {
        if !binary.exists() {
            anyhow::bail!("Firecracker binary not found at {:?}", binary);
        }

        Ok(Self {
            binary: binary.clone(),
            socket_path: None,
            metrics_path: None,
            client: None,
            mode: None,
        })
    }

    /// Create a new Firecracker driver with explicit virtualization mode
    pub async fn with_mode(
        binary: &PathBuf,
        mode: crate::VirtualizationMode,
    ) -> anyhow::Result<Self> {
        let mut driver = Self::new(binary).await?;
        driver.mode = Some(mode);
        Ok(driver)
    }

    /// Create a driver instance bound to a specific VM socket
    pub fn for_socket(&self, socket: &Path) -> Self {
        let client = FirecrackerClient::new(socket);
        Self {
            binary: self.binary.clone(),
            socket_path: Some(socket.to_path_buf()),
            metrics_path: None,
            client: Some(client),
            mode: self.mode,
        }
    }

    /// Attach a metrics path to this driver instance
    pub fn with_metrics_path(mut self, path: PathBuf) -> Self {
        self.metrics_path = Some(path);
        self
    }

    /// Internal helper to get the active client or bail
    fn client(&self) -> anyhow::Result<&FirecrackerClient> {
        self.client.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Driver not initialized for a socket. Call for_socket() first.")
        })
    }

    /// Get the path to the Firecracker binary
    pub fn binary_path(&self) -> &PathBuf {
        &self.binary
    }

    /// Configure a fresh microVM
    pub async fn configure_vm(&self, config: &super::MicrovmConfig) -> anyhow::Result<()> {
        let client = self.client()?;

        // Kernel & Boot Args
        client
            .put_boot_source(BootSource {
                kernel_image_path: config.kernel_path.to_string_lossy().to_string(),
                boot_args: Some(config.kernel_boot_args.clone()),
                initrd_path: None,
            })
            .await
            .with_context(|| "Failed to configure boot source")?;

        // Machine Config (vCPU, Mem)
        // For PVM mode, we skip CPU templates
        let cpu_template = if self.mode == Some(crate::VirtualizationMode::Pvm) {
            None // PVM doesn't support hardware CPU templates
        } else {
            Some(CpuTemplate::T2)
        };

        client
            .put_machine_config(MachineConfiguration {
                vcpu_count: config.vcpu_count() as i64,
                mem_size_mib: config.memory_mb as i64,
                smt: Some(false), // Disable SMT for better isolation
                track_dirty_pages: Some(false),
                cpu_template,
                huge_pages: None,
            })
            .await
            .with_context(|| "Failed to configure machine")?;

        // Drives
        for drive in &config.drives {
            client
                .put_drive(
                    &drive.drive_id,
                    Drive {
                        drive_id: drive.drive_id.clone(),
                        path_on_host: Some(drive.path_on_host.to_string_lossy().to_string()),
                        is_root_device: drive.is_root_device,
                        is_read_only: Some(drive.is_read_only),
                        cache_type: Some(CacheType::Unsafe), // Better performance for ephemeral
                        io_engine: Some(IoEngine::Sync),
                        rate_limiter: None,
                        partuuid: None,
                        socket: None,
                    },
                )
                .await
                .with_context(|| format!("Failed to configure drive {}", drive.drive_id))?;
        }

        // Network
        for net in &config.network_interfaces {
            client
                .put_network_interface(
                    &net.iface_id,
                    NetworkInterface {
                        iface_id: net.iface_id.clone(),
                        host_dev_name: net.host_dev_name.clone(),
                        guest_mac: Some(net.guest_mac.clone()),
                        rx_rate_limiter: None,
                        tx_rate_limiter: None,
                    },
                )
                .await
                .with_context(|| {
                    format!("Failed to configure network interface {}", net.iface_id)
                })?;
        }

        Ok(())
    }

    /// Start the microVM
    pub async fn start_vm(&self) -> anyhow::Result<()> {
        let client = self.client()?;
        client
            .start_instance()
            .await
            .with_context(|| "Failed to start VM")?;
        Ok(())
    }

    /// Stop the microVM (graceful shutdown via Ctrl+Alt+Del)
    pub async fn stop_vm(&self) -> anyhow::Result<()> {
        let client = self.client()?;
        client
            .send_ctrl_alt_del()
            .await
            .with_context(|| "Failed to stop VM")?;
        Ok(())
    }

    /// Force shutdown (handled by process killing in VmmManager)
    pub async fn force_shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Get instance information
    pub async fn describe_instance(
        &self,
    ) -> anyhow::Result<shellwego_firecracker::models::InstanceInfo> {
        let client = self.client()?;
        client
            .describe_instance()
            .await
            .with_context(|| "Failed to get VM info")
    }

    /// Create a snapshot
    pub async fn create_snapshot(&self, mem_path: &str, snapshot_path: &str) -> anyhow::Result<()> {
        let client = self.client()?;
        client
            .create_snapshot(SnapshotCreateParams {
                mem_file_path: mem_path.to_string(),
                snapshot_path: snapshot_path.to_string(),
                snapshot_type: Some(SnapshotType::Full),
            })
            .await
            .with_context(|| "Failed to create snapshot")
    }

    /// Load a snapshot
    pub async fn load_snapshot(
        &self,
        mem_path: &str,
        snapshot_path: &str,
        enable_diff_snapshots: bool,
    ) -> anyhow::Result<()> {
        let client = self.client()?;
        client
            .load_snapshot(SnapshotLoadParams {
                mem_file_path: Some(mem_path.to_string()),
                snapshot_path: snapshot_path.to_string(),
                track_dirty_pages: Some(enable_diff_snapshots),
                resume_vm: Some(true),
                mem_backend: None,
                enable_diff_snapshots: None,
                network_overrides: None,
            })
            .await
            .with_context(|| "Failed to load snapshot")
    }

    /// Pause the microVM
    pub async fn pause_vm(&self) -> anyhow::Result<()> {
        self.client()?
            .pause_vm()
            .await
            .with_context(|| "Failed to pause VM")
    }

    /// Resume the microVM
    pub async fn resume_vm(&self) -> anyhow::Result<()> {
        self.client()?
            .resume_vm()
            .await
            .with_context(|| "Failed to resume VM")
    }

    /// Configure metrics output path
    pub async fn configure_metrics(&self, metrics_path: &Path) -> anyhow::Result<()> {
        let client = self.client()?;
        client
            .put_metrics(Metrics {
                metrics_path: metrics_path.to_string_lossy().to_string(),
            })
            .await
            .with_context(|| "Failed to configure metrics")
    }

    /// Read and parse metrics from the configured metrics file
    pub async fn get_metrics(&self) -> anyhow::Result<super::MicrovmMetrics> {
        let path = self.metrics_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Metrics path not configured for this driver instance")
        })?;

        // Try reading metrics file
        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read metrics from {:?}", path))?;

        if content.trim().is_empty() {
            return Ok(super::MicrovmMetrics::default());
        }

        let fc_metrics: FirecrackerMetrics =
            serde_json::from_str(&content).with_context(|| "Failed to parse metrics JSON")?;

        // Aggregate block metrics
        let (block_read, block_write) = fc_metrics
            .block
            .as_ref()
            .map(|b| {
                b.values().fold((0, 0), |acc, m| {
                    (acc.0 + m.read_bytes, acc.1 + m.write_bytes)
                })
            })
            .unwrap_or((0, 0));

        // Aggregate network metrics
        let (net_rx, net_tx) = fc_metrics
            .net
            .as_ref()
            .map(|n| {
                n.values().fold((0, 0), |acc, m| {
                    (acc.0 + m.rx_bytes_count, acc.1 + m.tx_bytes_count)
                })
            })
            .unwrap_or((0, 0));

        Ok(super::MicrovmMetrics {
            cpu_usage_usec: 0,
            memory_rss_bytes: 0,
            network_rx_bytes: net_rx,
            network_tx_bytes: net_tx,
            block_read_bytes: block_read,
            block_write_bytes: block_write,
        })
    }

    /// Update machine configuration
    pub async fn update_machine_config(
        &self,
        vcpu_count: Option<i64>,
        mem_size_mib: Option<i64>,
    ) -> anyhow::Result<()> {
        let client = self.client()?;
        client
            .put_machine_config(MachineConfiguration {
                vcpu_count: vcpu_count.unwrap_or(1),
                mem_size_mib: mem_size_mib.unwrap_or(128),
                smt: Some(false),
                ..Default::default()
            })
            .await
            .with_context(|| "Failed to update machine config")?;
        Ok(())
    }

    /// Add a drive (hotplug)
    pub async fn add_drive(&self, drive: &super::DriveConfig) -> anyhow::Result<()> {
        let client = self.client()?;
        client
            .put_drive(
                &drive.drive_id,
                Drive {
                    drive_id: drive.drive_id.clone(),
                    path_on_host: Some(drive.path_on_host.to_string_lossy().to_string()),
                    is_root_device: false,
                    is_read_only: Some(drive.is_read_only),
                    cache_type: Some(CacheType::Unsafe),
                    partuuid: None,
                    io_engine: None,
                    rate_limiter: None,
                    socket: None,
                },
            )
            .await
            .with_context(|| "Failed to add drive")?;
        Ok(())
    }

    /// Remove a drive (not fully supported by Firecracker)
    pub async fn remove_drive(&self, _drive_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("Drive removal not fully supported by Firecracker hotplug yet");
    }

    /// Update boot source (not supported after VM start)
    pub async fn update_boot_source(
        &self,
        _kernel_path: &PathBuf,
        _boot_args: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// Send Ctrl+Alt+Del to the VM
    pub async fn send_ctrl_alt_del(&self) -> anyhow::Result<()> {
        self.stop_vm().await
    }

    /// Get the current VM state
    pub async fn get_vm_state(&self) -> anyhow::Result<InstanceState> {
        let info = self.describe_instance().await?;
        Ok(info.state)
    }

    /// Add a network interface (hotplug - not implemented)
    pub async fn add_network_interface(
        &self,
        _iface: &super::NetworkInterface,
    ) -> anyhow::Result<()> {
        anyhow::bail!("Network hotplug not implemented");
    }

    /// Remove a network interface (hotplug - not implemented)
    pub async fn remove_network_interface(&self, _iface_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("Network hotplug not implemented");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_driver_creation_fails_for_missing_binary() {
        let result = FirecrackerDriver::new(&PathBuf::from("/nonexistent/firecracker")).await;
        assert!(result.is_err());
    }
}
