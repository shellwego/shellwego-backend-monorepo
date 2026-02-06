//! Firecracker VMM Driver
//!
//! This module provides a driver for Firecracker microVMs using the
//! `shellwego-firecracker` crate which mirrors the Firecracker API.

use std::path::{Path, PathBuf};
use shellwego_firecracker::vmm::client::FirecrackerClient;
// Re-export models for convenience
pub use shellwego_firecracker::models::{
    InstanceInfo, VmState, BootSource, MachineConfig, Drive, NetworkInterface, 
    ActionInfo, SnapshotCreateParams, SnapshotLoadParams, Vm, Metrics, FirecrackerMetrics
};

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
}

impl std::fmt::Debug for FirecrackerDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FirecrackerDriver")
            .field("binary", &self.binary)
            .field("socket_path", &self.socket_path)
            .field("metrics_path", &self.metrics_path)
            .field("client", &if self.client.is_some() { "Some(FirecrackerClient)" } else { "None" })
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
        })
    }

    /// Create a driver instance bound to a specific VM socket
    pub fn for_socket(&self, socket: &Path) -> Self {
        let client = FirecrackerClient::new(socket);
        Self {
            binary: self.binary.clone(),
            socket_path: Some(socket.to_path_buf()),
            metrics_path: None, // Can be set via with_metrics_path
            client: Some(client),
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

    pub fn binary_path(&self) -> &PathBuf {
        &self.binary
    }

    /// Configure a fresh microVM
    pub async fn configure_vm(&self, config: &super::MicrovmConfig) -> anyhow::Result<()> {
        let client = self.client()?;

        // Kernel & Boot Args
        client.put_guest_boot_source(BootSource {
            kernel_image_path: config.kernel_path.to_string_lossy().to_string(),
            boot_args: Some(config.kernel_boot_args.clone()),
            initrd_path: None,
        }).await?;

        // Machine Config (vCPU, Mem)
        client.put_machine_configuration(MachineConfig {
            vcpu_count: (config.cpu_shares / 1024).max(1) as i64,
            mem_size_mib: config.memory_mb as i64,
            smt: Some(false), // Disable SMT for better isolation
            track_dirty_pages: Some(false),
            cpu_template: Some("T2".to_string()), // Use T2 template by default for x86
        }).await?;

        // Drives
        for drive in &config.drives {
            client.put_drive(&drive.drive_id, Drive {
                drive_id: drive.drive_id.clone(),
                path_on_host: drive.path_on_host.to_string_lossy().to_string(),
                is_root_device: drive.is_root_device,
                is_read_only: drive.is_read_only,
                cache_type: Some("Unsafe".to_string()), // Better performance for ephemeral
                io_engine: Some("Sync".to_string()),
                rate_limiter: None, 
                partuuid: None,
            }).await?;
        }

        // Network
        for net in &config.network_interfaces {
            client.put_network_interface(&net.iface_id, NetworkInterface {
                iface_id: net.iface_id.clone(),
                host_dev_name: net.host_dev_name.clone(),
                guest_mac: Some(net.guest_mac.clone()),
                allow_mmds_requests: Some(true),
                ..Default::default()
            }).await?;
        }

        Ok(())
    }

    pub async fn start_vm(&self) -> anyhow::Result<()> {
        let client = self.client()?;
        client.put_actions(ActionInfo {
            action_type: "InstanceStart".to_string(),
        }).await?;
        Ok(())
    }

    pub async fn stop_vm(&self) -> anyhow::Result<()> {
        let client = self.client()?;
        client.put_actions(ActionInfo {
            action_type: "SendCtrlAltDel".to_string(),
        }).await?;
        Ok(())
    }

    pub async fn force_shutdown(&self) -> anyhow::Result<()> {
        // Handled by process killing in VmmManager
        Ok(())
    }

    pub async fn describe_instance(&self) -> anyhow::Result<InstanceInfo> {
        let client = self.client()?;
        client.get_vm_info().await
    }

    pub async fn create_snapshot(&self, mem_path: &str, snapshot_path: &str) -> anyhow::Result<()> {
        let client = self.client()?;
        client.put_snapshot_create(SnapshotCreateParams {
            mem_file_path: mem_path.to_string(),
            snapshot_path: snapshot_path.to_string(),
            snapshot_type: Some("Full".to_string()),
            version: None,
        }).await
    }

    pub async fn load_snapshot(&self, mem_path: &str, snapshot_path: &str, enable_diff_snapshots: bool) -> anyhow::Result<()> {
        let client = self.client()?;
        client.put_snapshot_load(SnapshotLoadParams {
            mem_file_path: mem_path.to_string(),
            snapshot_path: snapshot_path.to_string(),
            enable_diff_snapshots: Some(enable_diff_snapshots),
            resume_vm: Some(true),
        }).await
    }

    pub async fn pause_vm(&self) -> anyhow::Result<()> {
        self.client()?.patch_vm_state(Vm { state: "Paused".to_string() }).await
    }

    pub async fn resume_vm(&self) -> anyhow::Result<()> {
        self.client()?.patch_vm_state(Vm { state: "Resumed".to_string() }).await
    }

    pub async fn configure_metrics(&self, metrics_path: &Path) -> anyhow::Result<()> {
        let client = self.client()?;
        client.put_metrics(Metrics {
            metrics_path: metrics_path.to_string_lossy().to_string(),
        }).await
    }

    /// Read and parse metrics from the configured metrics FIFO/file
    pub async fn get_metrics(&self) -> anyhow::Result<super::MicrovmMetrics> {
        let path = self.metrics_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Metrics path not configured for this driver instance")
        })?;

        // Try reading metrics file
        let content = tokio::fs::read_to_string(path).await?;
        if content.trim().is_empty() {
            return Ok(super::MicrovmMetrics::default());
        }

        let fc_metrics: FirecrackerMetrics = serde_json::from_str(&content)?;
        
        // Aggregate block metrics
        let (block_read, block_write) = if let Some(block) = fc_metrics.block {
            block.values().fold((0, 0), |acc, m| (acc.0 + m.read_bytes, acc.1 + m.write_bytes))
        } else {
            (0, 0)
        };

        // Aggregate network metrics
        let (net_rx, net_tx) = if let Some(net) = fc_metrics.net {
            net.values().fold((0, 0), |acc, m| (acc.0 + m.rx_bytes_count, acc.1 + m.tx_bytes_count))
        } else {
            (0, 0)
        };
        
        Ok(super::MicrovmMetrics {
            cpu_usage_usec: 0, 
            memory_rss_bytes: 0, 
            network_rx_bytes: net_rx,
            network_tx_bytes: net_tx,
            block_read_bytes: block_read,
            block_write_bytes: block_write,
        })
    }

    pub async fn update_machine_config(&self, vcpu_count: Option<i64>, mem_size_mib: Option<i64>) -> anyhow::Result<()> {
        let client = self.client()?;
        client.put_machine_configuration(MachineConfig {
            vcpu_count: vcpu_count.unwrap_or(1),
            mem_size_mib: mem_size_mib.unwrap_or(128),
            smt: Some(false),
            ..Default::default()
        }).await?;
        Ok(())
    }

    pub async fn add_drive(&self, drive: &super::DriveConfig) -> anyhow::Result<()> {
        let client = self.client()?;
        client.put_drive(&drive.drive_id, Drive {
            drive_id: drive.drive_id.clone(),
            path_on_host: drive.path_on_host.to_string_lossy().to_string(),
            is_root_device: false,
            is_read_only: drive.is_read_only,
            cache_type: Some("Unsafe".to_string()),
            ..Default::default()
        }).await?;
        Ok(())
    }

    pub async fn remove_drive(&self, _drive_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("Drive removal not fully supported by Firecracker hotplug yet");
    }
    
    pub async fn update_boot_source(&self, _kernel_path: &PathBuf, _boot_args: &str) -> anyhow::Result<()> {
         Ok(()) 
    }

    pub async fn send_ctrl_alt_del(&self) -> anyhow::Result<()> {
        self.stop_vm().await
    }

    pub async fn get_vm_state(&self) -> anyhow::Result<VmState> {
        let info = self.describe_instance().await?;
        match info.state.as_str() {
            "NotStarted" => Ok(VmState::NotStarted),
            "Starting" => Ok(VmState::Starting),
            "Running" => Ok(VmState::Running),
            "Paused" => Ok(VmState::Paused),
            "Halted" => Ok(VmState::Halted),
            "Configured" => Ok(VmState::Configured),
            _ => Ok(VmState::Configured),
        }
    }
    
    pub async fn add_network_interface(&self, _iface: &super::NetworkInterface) -> anyhow::Result<()> {
        anyhow::bail!("Network hotplug not implemented")
    }
    
    pub async fn remove_network_interface(&self, _iface_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("Network hotplug not implemented")
    }
}