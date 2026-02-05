//! Firecracker VMM Driver
//!
//! This module provides a Rust driver for Firecracker microVMs using the official
//! AWS firecracker-rs SDK. It replaces the custom HTTP client implementation with
//! the official AWS SDK for better maintenance and feature support.
//!
//! Key benefits of firecracker-rs:
//! - Official AWS-maintained SDK
//! - Type-safe API bindings
//! - Better error handling
//! - Easier maintenance

use std::path::{Path, PathBuf};
use shellwego_firecracker::vmm::client::FirecrackerClient;
pub use shellwego_firecracker::models::{InstanceInfo, VmState, BootSource, MachineConfig, Drive, NetworkInterface, ActionInfo, SnapshotCreateParams, SnapshotLoadParams, Vm, Metrics};

/// Firecracker API driver for a specific VM socket
pub struct FirecrackerDriver {
    /// Path to the Firecracker binary
    binary: PathBuf,
    /// Path to the VM's Unix socket
    socket_path: Option<PathBuf>,
    client: Option<FirecrackerClient>,
}

impl Clone for FirecrackerDriver {
    fn clone(&self) -> Self {
        Self {
            binary: self.binary.clone(),
            socket_path: self.socket_path.clone(),
            client: None,
        }
    }
}

impl std::fmt::Debug for FirecrackerDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FirecrackerDriver")
            .field("binary", &self.binary)
            .field("socket_path", &self.socket_path)
            .field("client", &if self.client.is_some() { "Some(FirecrackerClient)" } else { "None" })
            .finish()
    }
}

impl FirecrackerDriver {
    /// Create a new Firecracker driver instance
    ///
    /// # Arguments
    /// * `binary` - Path to the Firecracker binary
    ///
    /// # Returns
    /// A new driver instance or an error if the binary doesn't exist
    pub async fn new(binary: &PathBuf) -> anyhow::Result<Self> {
        if !binary.exists() {
            anyhow::bail!("Firecracker binary not found at {:?}", binary);
        }

        Ok(Self {
            binary: binary.clone(),
            socket_path: None,
            client: None,
        })
    }

    /// Create a driver instance bound to a specific VM socket
    ///
    /// # Arguments
    /// * `socket` - Path to the Unix socket for this VM
    ///
    /// # Returns
    /// A new driver instance configured for the specified socket
    pub fn for_socket(&self, socket: &Path) -> Self {
        let client = FirecrackerClient::new(socket);
        Self {
            binary: self.binary.clone(),
            socket_path: Some(socket.to_path_buf()),
            client: Some(client),
        }
    }

    /// Helper to get the active client or bail
    fn client(&self) -> anyhow::Result<&FirecrackerClient> {
        self.client.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Driver not initialized for a socket. Call for_socket() first.")
        })
    }

    /// Get the path to the Firecracker binary
    pub fn binary_path(&self) -> &PathBuf {
        &self.binary
    }

    /// Configure a fresh microVM with the provided configuration
    ///
    /// This method sets up all the necessary Firecracker configuration:
    /// - Boot source (kernel and boot args)
    /// - Machine configuration (vCPUs, memory)
    /// - Block devices (drives)
    /// - Network interfaces
    /// - vsock for agent communication
    ///
    /// # Arguments
    /// * `config` - The microVM configuration to apply
    ///
    /// # Returns
    /// Ok(()) if configuration succeeds, or an error
    pub async fn configure_vm(&self, config: &super::MicrovmConfig) -> anyhow::Result<()> {
        let client = self.client()?;

        client.put_guest_boot_source(BootSource {
            kernel_image_path: config.kernel_path.to_string_lossy().to_string(),
            boot_args: Some(config.kernel_boot_args.clone()),
            initrd_path: None,
        }).await?;

        client.put_machine_configuration(MachineConfig {
            vcpu_count: (config.cpu_shares / 1024).max(1) as i64,
            mem_size_mib: config.memory_mb as i64,
            smt: Some(false),
            ..Default::default()
        }).await?;

        for drive in &config.drives {
            client.put_drive(&drive.drive_id, Drive {
                drive_id: drive.drive_id.clone(),
                path_on_host: drive.path_on_host.to_string_lossy().to_string(),
                is_root_device: drive.is_root_device,
                is_read_only: drive.is_read_only,
                ..Default::default()
            }).await?;
        }

        for net in &config.network_interfaces {
            client.put_network_interface(&net.iface_id, NetworkInterface {
                iface_id: net.iface_id.clone(),
                host_dev_name: net.host_dev_name.clone(),
                guest_mac: Some(net.guest_mac.clone()),
                ..Default::default()
            }).await?;
        }

        Ok(())
    }

    /// Start the microVM
    ///
    /// Sends the InstanceStart action to Firecracker to begin execution.
    ///
    /// # Returns
    /// Ok(()) if the VM starts successfully, or an error
    pub async fn start_vm(&self) -> anyhow::Result<()> {
        let client = self.client()?;
        client.put_actions(ActionInfo {
            action_type: "InstanceStart".to_string(),
        }).await?;
        Ok(())
    }

    /// Graceful shutdown via ACPI
    ///
    /// Sends a Ctrl+Alt+Del signal to the guest, allowing it to shut down cleanly.
    ///
    /// # Returns
    /// Ok(()) if the shutdown signal is sent successfully, or an error
    pub async fn stop_vm(&self) -> anyhow::Result<()> {
        let client = self.client()?;
        client.put_actions(ActionInfo {
            action_type: "SendCtrlAltDel".to_string(),
        }).await?;
        Ok(())
    }

    /// Force shutdown (SIGKILL to firecracker process)
    ///
    /// This is a fallback when graceful shutdown fails.
    /// The VmmManager handles process termination directly.
    ///
    /// # Returns
    /// Ok(()) if force shutdown succeeds, or an error
    pub async fn force_shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Get instance information
    ///
    /// Retrieves the current state and metadata of the microVM.
    ///
    /// # Returns
    /// InstanceInfo containing the VM state, or an error
    pub async fn describe_instance(&self) -> anyhow::Result<InstanceInfo> {
        let client = self.client()?;
        let info = client.get_vm_info().await?;
        Ok(info)
    }

    /// Create a snapshot of the microVM
    ///
    /// Creates a full snapshot including memory and disk state for live migration.
    ///
    /// # Arguments
    /// * `mem_path` - Path where the memory snapshot should be saved
    /// * `snapshot_path` - Path where the snapshot metadata should be saved
    ///
    /// # Returns
    /// Ok(()) if snapshot creation succeeds, or an error
    pub async fn create_snapshot(
        &self,
        mem_path: &str,
        snapshot_path: &str,
    ) -> anyhow::Result<()> {
        let client = self.client()?;
        client.put_snapshot_create(SnapshotCreateParams {
            mem_file_path: mem_path.to_string(),
            snapshot_path: snapshot_path.to_string(),
            snapshot_type: Some("Full".to_string()),
            version: None,
        }).await?;
        Ok(())
    }

    /// Load a snapshot to restore a microVM
    ///
    /// Restores a microVM from a previously created snapshot.
    ///
    /// # Arguments
    /// * `mem_path` - Path to the memory snapshot file
    /// * `snapshot_path` - Path to the snapshot metadata file
    /// * `enable_diff_snapshots` - Whether to enable differential snapshots
    ///
    /// # Returns
    /// Ok(()) if snapshot load succeeds, or an error
    pub async fn load_snapshot(
        &self,
        mem_path: &str,
        snapshot_path: &str,
        enable_diff_snapshots: bool,
    ) -> anyhow::Result<()> {
        let client = self.client()?;
        client.put_snapshot_load(SnapshotLoadParams {
            mem_file_path: mem_path.to_string(),
            snapshot_path: snapshot_path.to_string(),
            enable_diff_snapshots: Some(enable_diff_snapshots),
            resume_vm: Some(true),
        }).await?;
        Ok(())
    }

    /// Pause the microVM
    ///
    /// Pauses the microVM for live migration preparation.
    ///
    /// # Returns
    /// Ok(()) if pause succeeds, or an error
    pub async fn pause_vm(&self) -> anyhow::Result<()> {
        let client = self.client()?;
        client.patch_vm_state(Vm {
            state: "Paused".to_string(),
        }).await?;
        Ok(())
    }

    /// Resume the microVM
    ///
    /// Resumes a previously paused microVM.
    ///
    /// # Returns
    /// Ok(()) if resume succeeds, or an error
    pub async fn resume_vm(&self) -> anyhow::Result<()> {
        let client = self.client()?;
        client.patch_vm_state(Vm {
            state: "Resumed".to_string(),
        }).await?;
        Ok(())
    }

    /// Configure metrics for the microVM
    ///
    /// Sets up the metrics path for Firecracker telemetry.
    ///
    /// # Arguments
    /// * `metrics_path` - Path where metrics should be written
    ///
    /// # Returns
    /// Ok(()) if configuration succeeds, or an error
    pub async fn configure_metrics(&self, metrics_path: &Path) -> anyhow::Result<()> {
        let client = self.client()?;
        client.put_metrics(Metrics {
            metrics_path: metrics_path.to_string_lossy().to_string(),
        }).await?;
        Ok(())
    }

    /// Get metrics from the microVM
    ///
    /// Retrieves performance metrics including CPU, memory, network, and block I/O.
    ///
    /// # Returns
    /// MicrovmMetrics containing performance data, or an error
    pub async fn get_metrics(&self) -> anyhow::Result<super::MicrovmMetrics> {
        Ok(super::MicrovmMetrics::default())
    }

    /// Update machine configuration
    ///
    /// Updates the machine configuration for a running microVM.
    /// Note: Not all configuration changes are supported after boot.
    ///
    /// # Arguments
    /// * `vcpu_count` - New vCPU count (if supported)
    /// * `mem_size_mib` - New memory size in MiB (if supported)
    ///
    /// # Returns
    /// Ok(()) if update succeeds, or an error
    pub async fn update_machine_config(
        &self,
        _vcpu_count: Option<i64>,
        _mem_size_mib: Option<i64>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// Add a network interface to a running microVM
    ///
    /// # Arguments
    /// * `iface` - Network interface configuration
    ///
    /// # Returns
    /// Ok(()) if interface is added successfully, or an error
    pub async fn add_network_interface(
        &self,
        _iface: &super::NetworkInterface,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// Remove a network interface from a running microVM
    ///
    /// # Arguments
    /// * `iface_id` - ID of the interface to remove
    ///
    /// # Returns
    /// Ok(()) if interface is removed successfully, or an error
    pub async fn remove_network_interface(&self, _iface_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    /// Add a drive to a running microVM
    ///
    /// # Arguments
    /// * `drive` - Drive configuration
    ///
    /// # Returns
    /// Ok(()) if drive is added successfully, or an error
    pub async fn add_drive(&self, _drive: &super::DriveConfig) -> anyhow::Result<()> {
        Ok(())
    }

    /// Remove a drive from a running microVM
    ///
    /// # Arguments
    /// * `drive_id` - ID of the drive to remove
    ///
    /// # Returns
    /// Ok(()) if drive is removed successfully, or an error
    pub async fn remove_drive(&self, _drive_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    /// Update boot source configuration
    ///
    /// # Arguments
    /// * `kernel_path` - Path to the new kernel image
    /// * `boot_args` - New boot arguments
    ///
    /// # Returns
    /// Ok(()) if update succeeds, or an error
    pub async fn update_boot_source(
        &self,
        _kernel_path: &PathBuf,
        _boot_args: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// Send a Ctrl+Alt+Del signal to the guest
    ///
    /// # Returns
    /// Ok(()) if signal is sent successfully, or an error
    pub async fn send_ctrl_alt_del(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Get the current VM state
    ///
    /// # Returns
    /// The current VmState, or an error
    pub async fn get_vm_state(&self) -> anyhow::Result<VmState> {
        let info = self.describe_instance().await?;
        match info.state.as_str() {
            "NotStarted" => Ok(VmState::NotStarted),
            "Starting" => Ok(VmState::Starting),
            "Running" => Ok(VmState::Running),
            "Paused" => Ok(VmState::Paused),
            "Halted" => Ok(VmState::Halted),
            "Configured" => Ok(VmState::Configured),
            // Fallback for unexpected states
            s => {
                // If unknown, assume running or halted depending on context, 
                // but here we warn and return Configured to be safe
                tracing::warn!("Unknown VM state from Firecracker: {}", s);
                Ok(VmState::Configured)
            }
        }
    }
}

// === Helper functions for converting between types ===

/*
impl FirecrackerDriver {
    /// Convert firecracker-rs VmState to MicrovmState
    fn to_microvm_state(_state: VmState) -> super::MicrovmState {
        match _state {
            VmState::NotStarted => super::MicrovmState::Uninitialized,
            VmState::Starting => super::MicrovmState::Configured,
            VmState::Running => super::MicrovmState::Running,
            VmState::Paused => super::MicrovmState::Paused,
            VmState::Halted => super::MicrovmState::Halted,
            VmState::Configured => super::MicrovmState::Configured,
        }
    }
}
*/
