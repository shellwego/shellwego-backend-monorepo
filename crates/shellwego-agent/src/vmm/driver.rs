//! Firecracker VMM Driver
//!
//! This module provides a driver for Firecracker microVMs using the
//! `shellwego-firecracker` crate which mirrors the Firecracker API.
//! Supports both KVM (hardware) and PVM (software) virtualization modes.

use anyhow::Context;
use shellwego_firecracker::models::{
    Balloon, BootSource, CacheType, CpuConfig, CpuTemplate, Drive, EntropyDevice,
    FirecrackerMetrics, InstanceState, IoEngine, LogLevel, Logger, MachineConfiguration,
    Metrics, MmdsConfig, MmdsContentsObject, MmdsVersion, NetworkInterface,
    SnapshotCreateParams, SnapshotLoadParams, SnapshotType, Vsock,
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

        // Entropy device for cryptographic randomness in the guest
        client
            .put_entropy(EntropyDevice { rate_limiter: None })
            .await
            .with_context(|| "Failed to configure entropy device")?;

        // Vsock for host-guest communication (if configured)
        if !config.vsock_path.is_empty() {
            let vsock_socket = format!(
                "{}/vsock.sock",
                std::path::Path::new(&config.vsock_path)
                    .parent()
                    .unwrap_or(std::path::Path::new("/var/run/shellwego"))
                    .display()
            );
            client
                .put_vsock(Vsock {
                    vsock_id: Some("vsock0".to_string()),
                    guest_cid: 3,
                    uds_path: vsock_socket,
                })
                .await
                .with_context(|| "Failed to configure vsock")?;
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

    /// Configure VM logging to a file for debug output
    pub async fn configure_logger(&self, log_path: &Path) -> anyhow::Result<()> {
        let client = self.client()?;
        client
            .put_logger(Logger {
                log_path: Some(log_path.to_string_lossy().to_string()),
                level: Some(LogLevel::Warning),
                show_level: Some(true),
                show_log_origin: Some(false),
                module: None,
            })
            .await
            .with_context(|| "Failed to configure logger")?;
        Ok(())
    }

    /// Configure MMDS (Microvm Metadata Service) with data and settings
    pub async fn configure_mmds(&self, data: MmdsContentsObject) -> anyhow::Result<()> {
        let client = self.client()?;
        client
            .put_mmds(data)
            .await
            .with_context(|| "Failed to configure MMDS data")?;
        // Also set MMDS config to allow all IPs and use v2
        client
            .put_mmds_config(MmdsConfig {
                version: Some(MmdsVersion::V2),
                network_interfaces: vec!["eth0".to_string()],
                ipv4_address: None,
                imds_compat: None,
            })
            .await
            .with_context(|| "Failed to configure MMDS config")?;
        Ok(())
    }

    /// Configure balloon device for memory overcommit
    pub async fn configure_balloon(
        &self,
        amount_mib: i64,
        deflate_on_oom: bool,
        stats_polling_interval_s: Option<i64>,
    ) -> anyhow::Result<()> {
        let client = self.client()?;
        client
            .put_balloon(Balloon {
                amount_mib,
                deflate_on_oom,
                stats_polling_interval_s,
                free_page_hinting: None,
                free_page_reporting: None,
            })
            .await
            .with_context(|| "Failed to configure balloon device")?;
        Ok(())
    }

    /// Configure CPU modifiers (e.g., CPUID, MSR, register modifiers)
    pub async fn configure_cpu(&self, cpu_config: CpuConfig) -> anyhow::Result<()> {
        let client = self.client()?;
        client
            .put_cpu_config(cpu_config)
            .await
            .with_context(|| "Failed to configure CPU")?;
        Ok(())
    }

    /// Configure a VM with initrd support
    pub async fn configure_vm_with_initrd(
        &self,
        config: &super::MicrovmConfig,
        initrd_path: Option<&str>,
    ) -> anyhow::Result<()> {
        let client = self.client()?;

        // Kernel, Boot Args, and optional initrd
        client
            .put_boot_source(BootSource {
                kernel_image_path: config.kernel_path.to_string_lossy().to_string(),
                boot_args: Some(config.kernel_boot_args.clone()),
                initrd_path: initrd_path.map(|s| s.to_string()),
            })
            .await
            .with_context(|| "Failed to configure boot source with initrd")?;

        // Machine Config (vCPU, Mem)
        let cpu_template = if self.mode == Some(crate::VirtualizationMode::Pvm) {
            None
        } else {
            Some(CpuTemplate::T2)
        };

        client
            .put_machine_config(MachineConfiguration {
                vcpu_count: config.vcpu_count() as i64,
                mem_size_mib: config.memory_mb as i64,
                smt: Some(false),
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
                        cache_type: Some(CacheType::Unsafe),
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

        // Entropy device for cryptographic randomness in the guest
        client
            .put_entropy(EntropyDevice { rate_limiter: None })
            .await
            .with_context(|| "Failed to configure entropy device")?;

        // Vsock for host-guest communication (if configured)
        if !config.vsock_path.is_empty() {
            let vsock_socket = format!(
                "{}/vsock.sock",
                std::path::Path::new(&config.vsock_path)
                    .parent()
                    .unwrap_or(std::path::Path::new("/var/run/shellwego"))
                    .display()
            );
            client
                .put_vsock(Vsock {
                    vsock_id: Some("vsock0".to_string()),
                    guest_cid: 3,
                    uds_path: vsock_socket,
                })
                .await
                .with_context(|| "Failed to configure vsock")?;
        }

        Ok(())
    }
}

/// Jailer configuration for isolating Firecracker processes
///
/// The jailer is a Firecracker companion process that provides:
/// - Process isolation via chroot and namespace restrictions
/// - Resource limits via cgroups
/// - Reduced privilege execution via UID/GID dropping
///
/// When jailer is used, the Firecracker API socket path changes to
/// be inside the jailer's chroot directory.
#[derive(Debug, Clone)]
pub struct JailerConfig {
    /// Jail directory (base for all jailed VMs)
    pub jail_dir: PathBuf,
    /// UID to run the jailed process as
    pub uid: u32,
    /// GID to run the jailed process as
    pub gid: u32,
    /// Chroot base directory
    pub chroot_base: String,
    /// Node name (for cgroup naming)
    pub node_name: String,
    /// Exec file path (Firecracker binary to jail)
    pub exec_file: PathBuf,
}

impl Default for JailerConfig {
    fn default() -> Self {
        Self {
            jail_dir: PathBuf::from("/var/run/jailer"),
            uid: 1000,
            gid: 1000,
            chroot_base: "/srv/jailer".to_string(),
            node_name: String::new(),
            exec_file: PathBuf::from("/usr/local/bin/firecracker"),
        }
    }
}

impl JailerConfig {
    /// Build jailer command-line arguments for a given app ID
    pub fn build_args(&self, app_id: &uuid::Uuid) -> Vec<String> {
        let id = format!("shellwego-{}", app_id);
        vec![
            format!("--id={}", id),
            format!("--exec-file={}", self.exec_file.display()),
            format!("--uid={}", self.uid),
            format!("--gid={}", self.gid),
            format!("--chroot-base-dir={}", self.chroot_base),
            "--daemonize".to_string(),
            format!(
                "--node={}",
                if self.node_name.is_empty() {
                    "default"
                } else {
                    &self.node_name
                }
            ),
        ]
    }

    /// Get the API socket path when jailer is used for a given app ID
    ///
    /// The socket is located inside the jailer's chroot at:
    /// `{chroot_base}/{id}/root/run/firecracker.sock`
    pub fn socket_path(&self, app_id: &uuid::Uuid) -> PathBuf {
        let id = format!("shellwego-{}", app_id);
        PathBuf::from(&self.chroot_base)
            .join(&id)
            .join("root")
            .join("run")
            .join("firecracker.sock")
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

    // --- JailerConfig tests ---

    #[test]
    fn test_jailer_config_default_values() {
        let config = JailerConfig::default();
        assert_eq!(config.jail_dir, PathBuf::from("/var/run/jailer"));
        assert_eq!(config.uid, 1000);
        assert_eq!(config.gid, 1000);
        assert_eq!(config.chroot_base, "/srv/jailer");
        assert!(config.node_name.is_empty());
        assert_eq!(
            config.exec_file,
            PathBuf::from("/usr/local/bin/firecracker")
        );
    }

    #[test]
    fn test_jailer_config_build_args() {
        let config = JailerConfig::default();
        let app_id = uuid::Uuid::new_v4();
        let args = config.build_args(&app_id);

        assert!(args.iter().any(|a| a.starts_with("--id=shellwego-")));
        assert!(args
            .iter()
            .any(|a| a == "--exec-file=/usr/local/bin/firecracker"));
        assert!(args.iter().any(|a| a == "--uid=1000"));
        assert!(args.iter().any(|a| a == "--gid=1000"));
        assert!(args.iter().any(|a| a == "--chroot-base-dir=/srv/jailer"));
        assert!(args.iter().any(|a| a == "--daemonize"));
        assert!(args.iter().any(|a| a == "--node=default"));
    }

    #[test]
    fn test_jailer_config_build_args_with_custom_node() {
        let mut config = JailerConfig::default();
        config.node_name = "worker-01".to_string();
        let app_id = uuid::Uuid::new_v4();
        let args = config.build_args(&app_id);

        assert!(args.iter().any(|a| a == "--node=worker-01"));
        assert!(!args.iter().any(|a| a == "--node=default"));
    }

    #[test]
    fn test_jailer_config_socket_path() {
        let config = JailerConfig::default();
        let app_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let path = config.socket_path(&app_id);

        assert_eq!(
            path,
            PathBuf::from(format!(
                "/srv/jailer/shellwego-{}/root/run/firecracker.sock",
                app_id
            ))
        );
    }

    #[test]
    fn test_jailer_config_socket_path_with_custom_chroot() {
        let mut config = JailerConfig::default();
        config.chroot_base = "/opt/jailer".to_string();
        let app_id = uuid::Uuid::new_v4();
        let path = config.socket_path(&app_id);

        let expected_prefix = "/opt/jailer/shellwego-";
        assert!(path.to_string_lossy().starts_with(expected_prefix));
        assert!(path.to_string_lossy().ends_with("root/run/firecracker.sock"));
    }

    // --- EntropyDevice struct test ---

    #[test]
    fn test_entropy_device_default() {
        let entropy = EntropyDevice::default();
        assert!(entropy.rate_limiter.is_none());
    }

    // --- Balloon struct test ---

    #[test]
    fn test_balloon_struct_creation() {
        let balloon = Balloon {
            amount_mib: 256,
            deflate_on_oom: true,
            stats_polling_interval_s: Some(5),
            free_page_hinting: None,
            free_page_reporting: None,
        };
        assert_eq!(balloon.amount_mib, 256);
        assert!(balloon.deflate_on_oom);
        assert_eq!(balloon.stats_polling_interval_s, Some(5));
    }

    // --- Logger struct test ---

    #[test]
    fn test_logger_struct_creation() {
        let logger = Logger {
            log_path: Some("/var/log/fc.log".to_string()),
            level: Some(LogLevel::Warning),
            show_level: Some(true),
            show_log_origin: Some(false),
            module: None,
        };
        assert_eq!(logger.log_path, Some("/var/log/fc.log".to_string()));
        assert_eq!(logger.level, Some(LogLevel::Warning));
    }

    // --- Vsock struct test ---

    #[test]
    fn test_vsock_struct_creation() {
        let vsock = Vsock {
            vsock_id: Some("vsock0".to_string()),
            guest_cid: 3,
            uds_path: "/tmp/vsock.sock".to_string(),
        };
        assert_eq!(vsock.guest_cid, 3);
        assert_eq!(vsock.vsock_id, Some("vsock0".to_string()));
        assert_eq!(vsock.uds_path, "/tmp/vsock.sock");
    }

    // --- MmdsConfig struct test ---

    #[test]
    fn test_mmds_config_struct_creation() {
        let mmds_config = MmdsConfig {
            version: Some(MmdsVersion::V2),
            network_interfaces: vec!["eth0".to_string()],
            ipv4_address: None,
            imds_compat: None,
        };
        assert_eq!(mmds_config.version, Some(MmdsVersion::V2));
        assert_eq!(mmds_config.network_interfaces.len(), 1);
    }
}
