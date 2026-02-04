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

use std::path::PathBuf;

// Re-export types from firecracker-rs for external use
// TODO: Add firecracker = "0.4" or latest version to Cargo.toml
// use firecracker::{Firecracker, BootSource, MachineConfig, Drive, NetworkInterface, VmState};

/// Firecracker API driver for a specific VM socket
///
/// This struct wraps the firecracker-rs SDK and provides a high-level interface
/// for interacting with Firecracker microVMs.
#[derive(Debug, Clone)]
pub struct FirecrackerDriver {
    /// Path to the Firecracker binary
    binary: PathBuf,
    /// Path to the VM's Unix socket
    socket_path: Option<PathBuf>,
    // TODO: Add firecracker-rs client field
    // client: Option<Firecracker>,
}

/// Instance information returned by describe_instance
#[derive(Debug, Clone)]
pub struct InstanceInfo {
    /// Current state of the microVM
    pub state: String,
}

/// VM state enumeration (from firecracker-rs)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmState {
    /// VM has not started
    NotStarted,
    /// VM is booting
    Booting,
    /// VM is running
    Running,
    /// VM is paused
    Paused,
    /// VM is halted
    Halted,
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
        // TODO: Verify binary exists and is executable
        // TODO: Check binary version compatibility
        // TODO: Initialize firecracker-rs client

        Ok(Self {
            binary: binary.clone(),
            socket_path: None,
            // client: None,
        })
    }

    /// Get the path to the Firecracker binary
    pub fn binary_path(&self) -> &PathBuf {
        &self.binary
    }

    /// Create a driver instance bound to a specific VM socket
    ///
    /// # Arguments
    /// * `socket` - Path to the Unix socket for this VM
    ///
    /// # Returns
    /// A new driver instance configured for the specified socket
    pub fn for_socket(&self, socket: &PathBuf) -> Self {
        // TODO: Create new firecracker-rs client with socket path
        // TODO: Validate socket path format

        Self {
            binary: self.binary.clone(),
            socket_path: Some(socket.clone()),
            // client: None,
        }
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
        // TODO: Get or create firecracker-rs client
        // TODO: Configure boot source with kernel path and boot args
        // TODO: Configure machine with vCPU count and memory size
        // TODO: Add all drives from config
        // TODO: Add all network interfaces from config
        // TODO: Configure vsock for agent communication
        // TODO: Handle any configuration errors with detailed messages

        Ok(())
    }

    /// Start the microVM
    ///
    /// Sends the InstanceStart action to Firecracker to begin execution.
    ///
    /// # Returns
    /// Ok(()) if the VM starts successfully, or an error
    pub async fn start_vm(&self) -> anyhow::Result<()> {
        // TODO: Get firecracker-rs client
        // TODO: Send InstanceStart action
        // TODO: Wait for VM to transition to running state
        // TODO: Handle start failures with appropriate error messages

        Ok(())
    }

    /// Graceful shutdown via ACPI
    ///
    /// Sends a Ctrl+Alt+Del signal to the guest, allowing it to shut down cleanly.
    ///
    /// # Returns
    /// Ok(()) if the shutdown signal is sent successfully, or an error
    pub async fn stop_vm(&self) -> anyhow::Result<()> {
        // TODO: Get firecracker-rs client
        // TODO: Send SendCtrlAltDel action
        // TODO: Optionally wait for VM to halt
        // TODO: Handle shutdown signal failures

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
        // TODO: Implement API-based force stop if available
        // TODO: Otherwise, signal that process should be killed externally
        // TODO: Log force shutdown event

        Ok(())
    }

    /// Get instance information
    ///
    /// Retrieves the current state and metadata of the microVM.
    ///
    /// # Returns
    /// InstanceInfo containing the VM state, or an error
    pub async fn describe_instance(&self) -> anyhow::Result<InstanceInfo> {
        // TODO: Get firecracker-rs client
        // TODO: Call get_vm_info or equivalent
        // TODO: Map response to InstanceInfo
        // TODO: Handle API errors

        Ok(InstanceInfo {
            state: "Unknown".to_string(),
        })
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
        // TODO: Get firecracker-rs client
        // TODO: Create snapshot configuration
        // TODO: Set snapshot type to Full
        // TODO: Set snapshot path and memory file path
        // TODO: Send snapshot create request
        // TODO: Wait for snapshot to complete
        // TODO: Handle snapshot errors

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
        // TODO: Get firecracker-rs client
        // TODO: Create snapshot load configuration
        // TODO: Set memory and snapshot paths
        // TODO: Configure diff snapshots if enabled
        // TODO: Send snapshot load request
        // TODO: Wait for VM to resume
        // TODO: Handle load errors

        Ok(())
    }

    /// Pause the microVM
    ///
    /// Pauses the microVM for live migration preparation.
    ///
    /// # Returns
    /// Ok(()) if pause succeeds, or an error
    pub async fn pause_vm(&self) -> anyhow::Result<()> {
        // TODO: Get firecracker-rs client
        // TODO: Send pause action
        // TODO: Wait for VM to reach paused state
        // TODO: Handle pause errors

        Ok(())
    }

    /// Resume the microVM
    ///
    /// Resumes a previously paused microVM.
    ///
    /// # Returns
    /// Ok(()) if resume succeeds, or an error
    pub async fn resume_vm(&self) -> anyhow::Result<()> {
        // TODO: Get firecracker-rs client
        // TODO: Send resume action
        // TODO: Wait for VM to reach running state
        // TODO: Handle resume errors

        Ok(())
    }

    /// Get metrics from the microVM
    ///
    /// Retrieves performance metrics including CPU, memory, network, and block I/O.
    ///
    /// # Returns
    /// MicrovmMetrics containing performance data, or an error
    pub async fn get_metrics(&self) -> anyhow::Result<super::MicrovmMetrics> {
        // TODO: Get firecracker-rs client
        // TODO: Request metrics from Firecracker
        // TODO: Parse and map metrics to MicrovmMetrics
        // TODO: Handle metrics API errors

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
        vcpu_count: Option<i64>,
        mem_size_mib: Option<i64>,
    ) -> anyhow::Result<()> {
        // TODO: Get firecracker-rs client
        // TODO: Check if updates are supported for running VM
        // TODO: Update vCPU count if provided
        // TODO: Update memory size if provided
        // TODO: Handle update errors

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
        iface: &super::NetworkInterface,
    ) -> anyhow::Result<()> {
        // TODO: Get firecracker-rs client
        // TODO: Create network interface configuration
        // TODO: Send add network interface request
        // TODO: Handle errors

        Ok(())
    }

    /// Remove a network interface from a running microVM
    ///
    /// # Arguments
    /// * `iface_id` - ID of the interface to remove
    ///
    /// # Returns
    /// Ok(()) if interface is removed successfully, or an error
    pub async fn remove_network_interface(&self, iface_id: &str) -> anyhow::Result<()> {
        // TODO: Get firecracker-rs client
        // TODO: Send remove network interface request
        // TODO: Handle errors

        Ok(())
    }

    /// Add a drive to a running microVM
    ///
    /// # Arguments
    /// * `drive` - Drive configuration
    ///
    /// # Returns
    /// Ok(()) if drive is added successfully, or an error
    pub async fn add_drive(&self, drive: &super::DriveConfig) -> anyhow::Result<()> {
        // TODO: Get firecracker-rs client
        // TODO: Create drive configuration
        // TODO: Send add drive request
        // TODO: Handle errors

        Ok(())
    }

    /// Remove a drive from a running microVM
    ///
    /// # Arguments
    /// * `drive_id` - ID of the drive to remove
    ///
    /// # Returns
    /// Ok(()) if drive is removed successfully, or an error
    pub async fn remove_drive(&self, drive_id: &str) -> anyhow::Result<()> {
        // TODO: Get firecracker-rs client
        // TODO: Send remove drive request
        // TODO: Handle errors

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
        kernel_path: &PathBuf,
        boot_args: &str,
    ) -> anyhow::Result<()> {
        // TODO: Get firecracker-rs client
        // TODO: Create boot source configuration
        // TODO: Send update boot source request
        // TODO: Handle errors

        Ok(())
    }

    /// Send a Ctrl+Alt+Del signal to the guest
    ///
    /// # Returns
    /// Ok(()) if signal is sent successfully, or an error
    pub async fn send_ctrl_alt_del(&self) -> anyhow::Result<()> {
        // TODO: Get firecracker-rs client
        // TODO: Send SendCtrlAltDel action
        // TODO: Handle errors

        Ok(())
    }

    /// Get the current VM state
    ///
    /// # Returns
    /// The current VmState, or an error
    pub async fn get_vm_state(&self) -> anyhow::Result<VmState> {
        // TODO: Get firecracker-rs client
        // TODO: Request current VM state
        // TODO: Map response to VmState enum
        // TODO: Handle errors

        Ok(VmState::NotStarted)
    }
}

// === Helper functions for converting between types ===

impl FirecrackerDriver {
    /// Convert MicrovmConfig to firecracker-rs BootSource
    fn to_boot_source(config: &super::MicrovmConfig) {
        // TODO: Convert kernel_path to string
        // TODO: Set boot_args from config
        // TODO: Return BootSource struct
    }

    /// Convert MicrovmConfig to firecracker-rs MachineConfig
    fn to_machine_config(config: &super::MicrovmConfig) {
        // TODO: Convert cpu_shares to vcpu_count
        // TODO: Convert memory_mb to mem_size_mib
        // TODO: Set optional fields (smt, cpu_template, track_dirty_pages)
        // TODO: Return MachineConfig struct
    }

    /// Convert DriveConfig to firecracker-rs Drive
    fn to_drive(drive: &super::DriveConfig) {
        // TODO: Map drive_id
        // TODO: Map path_on_host
        // TODO: Map is_root_device
        // TODO: Map is_read_only
        // TODO: Return Drive struct
    }

    /// Convert NetworkInterface to firecracker-rs NetworkInterface
    fn to_network_interface(net: &super::NetworkInterface) {
        // TODO: Map iface_id
        // TODO: Map host_dev_name
        // TODO: Map guest_mac
        // TODO: Return NetworkInterface struct
    }

    /// Convert firecracker-rs VmState to MicrovmState
    fn to_microvm_state(state: VmState) -> super::MicrovmState {
        // TODO: Map VmState::NotStarted to MicrovmState::Uninitialized
        // TODO: Map VmState::Paused to MicrovmState::Paused
        // TODO: Map VmState::Running to MicrovmState::Running
        // TODO: Map other states appropriately

        super::MicrovmState::Uninitialized
    }
}
