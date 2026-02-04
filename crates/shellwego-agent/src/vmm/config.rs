//! MicroVM configuration structures
//! 
//! Maps to Firecracker's API types but simplified for our use case.

use std::path::PathBuf;

/// Complete microVM configuration
#[derive(Debug, Clone)]
pub struct MicrovmConfig {
    pub app_id: uuid::Uuid,
    pub vm_id: uuid::Uuid,
    pub memory_mb: u64,
    pub cpu_shares: u64, // Converted to vCPU count
    pub kernel_path: PathBuf,
    pub kernel_boot_args: String,
    pub drives: Vec<DriveConfig>,
    pub network_interfaces: Vec<NetworkInterface>,
    pub vsock_path: String,
}

/// Block device configuration
#[derive(Debug, Clone)]
pub struct DriveConfig {
    pub drive_id: String,
    pub path_on_host: PathBuf,
    pub is_root_device: bool,
    pub is_read_only: bool,
    // TODO: Add rate limiting (iops, bandwidth)
}

/// Network interface configuration
#[derive(Debug, Clone)]
pub struct NetworkInterface {
    pub iface_id: String,
    pub host_dev_name: String,
    pub guest_mac: String,
    pub guest_ip: String,
    pub host_ip: String,
    // TODO: Add rate limiting, firewall rules
}

/// Runtime state of a microVM
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicrovmState {
    Uninitialized,
    Configured,
    Running,
    Paused,
    Halted,
}

/// Metrics from a running microVM
#[derive(Debug, Clone, Default)]
pub struct MicrovmMetrics {
    pub cpu_usage_usec: u64,
    pub memory_rss_bytes: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub block_read_bytes: u64,
    pub block_write_bytes: u64,
}