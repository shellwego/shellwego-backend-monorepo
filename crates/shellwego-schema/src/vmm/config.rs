//! MicroVM configuration types
//!
//! Maps to Firecracker's API types but simplified for our use case.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Complete microVM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct MicrovmConfig {
    /// Application ID
    pub app_id: Uuid,
    /// VM instance ID
    pub vm_id: Uuid,
    /// Memory allocation in MB
    pub memory_mb: u64,
    /// CPU shares (converted to vCPU count: 1024 = 1 vCPU)
    pub cpu_shares: u64,
    /// Path to the kernel image (vmlinux)
    pub kernel_path: PathBuf,
    /// Kernel boot arguments
    pub kernel_boot_args: String,
    /// Block device configurations
    pub drives: Vec<DriveConfig>,
    /// Network interface configurations
    pub network_interfaces: Vec<NetworkInterface>,
    /// VSock socket path for guest-host communication
    pub vsock_path: String,
}

impl Default for MicrovmConfig {
    fn default() -> Self {
        Self {
            app_id: Uuid::nil(),
            vm_id: Uuid::nil(),
            memory_mb: 128,
            cpu_shares: 1024,
            kernel_path: PathBuf::from("/var/lib/shellwego/vmlinux"),
            kernel_boot_args: "console=ttyS0 reboot=k panic=1 pci=off".to_string(),
            drives: vec![],
            network_interfaces: vec![],
            vsock_path: String::new(),
        }
    }
}

impl MicrovmConfig {
    /// Create a new MicrovmConfig with the given app and VM IDs
    pub fn new(app_id: Uuid, vm_id: Uuid) -> Self {
        Self {
            app_id,
            vm_id,
            ..Default::default()
        }
    }

    /// Set memory allocation in MB
    pub fn with_memory(mut self, mb: u64) -> Self {
        self.memory_mb = mb;
        self
    }

    /// Set CPU shares (1024 = 1 vCPU)
    pub fn with_cpu_shares(mut self, shares: u64) -> Self {
        self.cpu_shares = shares;
        self
    }

    /// Set kernel path
    pub fn with_kernel(mut self, path: PathBuf) -> Self {
        self.kernel_path = path;
        self
    }

    /// Set kernel boot arguments
    pub fn with_boot_args(mut self, args: &str) -> Self {
        self.kernel_boot_args = args.to_string();
        self
    }

    /// Add a drive
    pub fn with_drive(mut self, drive: DriveConfig) -> Self {
        self.drives.push(drive);
        self
    }

    /// Add a network interface
    pub fn with_network_interface(mut self, iface: NetworkInterface) -> Self {
        self.network_interfaces.push(iface);
        self
    }

    /// Get the number of vCPUs based on CPU shares
    pub fn vcpu_count(&self) -> u64 {
        (self.cpu_shares / 1024).max(1)
    }

    /// Convert to WASM-compatible config (for WASM mode fallback)
    pub fn to_wasm_config(&self) -> WasmConfig {
        WasmConfig {
            app_id: self.app_id,
            memory_mb: self.memory_mb,
            // WASM uses a fraction of the CPU allocation
            max_compute_units: (self.cpu_shares as f64 / 1024.0).max(0.1),
        }
    }
}

/// Block device configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct DriveConfig {
    /// Unique drive identifier
    pub drive_id: String,
    /// Path to the drive image on the host
    pub path_on_host: PathBuf,
    /// Whether this is the root device
    pub is_root_device: bool,
    /// Whether the drive is read-only
    pub is_read_only: bool,
    /// Optional rate limiter configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limiter: Option<RateLimiterConfig>,
}

impl DriveConfig {
    /// Create a new rootfs drive configuration
    pub fn rootfs(path: PathBuf) -> Self {
        Self {
            drive_id: "rootfs".to_string(),
            path_on_host: path,
            is_root_device: true,
            is_read_only: true,
            rate_limiter: None,
        }
    }

    /// Create a new data drive configuration
    pub fn data(drive_id: &str, path: PathBuf, read_only: bool) -> Self {
        Self {
            drive_id: drive_id.to_string(),
            path_on_host: path,
            is_root_device: false,
            is_read_only: read_only,
            rate_limiter: None,
        }
    }
}

/// Rate limiter configuration for drives and network
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct RateLimiterConfig {
    /// Bandwidth rate limit in bytes per second
    pub bandwidth: Option<u64>,
    /// Operations per second limit
    pub ops: Option<u64>,
}

/// Network interface configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct NetworkInterface {
    /// Interface identifier
    pub iface_id: String,
    /// Host TAP device name
    pub host_dev_name: String,
    /// Guest MAC address
    pub guest_mac: String,
    /// Guest IP address
    pub guest_ip: String,
    /// Host IP address (gateway)
    pub host_ip: String,
    /// Optional TX rate limiter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_rate_limiter: Option<RateLimiterConfig>,
    /// Optional RX rate limiter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx_rate_limiter: Option<RateLimiterConfig>,
}

impl NetworkInterface {
    /// Create a new network interface configuration
    pub fn new(iface_id: &str, tap_name: &str, mac: &str, guest_ip: &str, host_ip: &str) -> Self {
        Self {
            iface_id: iface_id.to_string(),
            host_dev_name: tap_name.to_string(),
            guest_mac: mac.to_string(),
            guest_ip: guest_ip.to_string(),
            host_ip: host_ip.to_string(),
            tx_rate_limiter: None,
            rx_rate_limiter: None,
        }
    }
}

/// WASM-specific configuration (for WASM mode)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct WasmConfig {
    /// Application ID
    pub app_id: Uuid,
    /// Memory allocation in MB
    pub memory_mb: u64,
    /// Maximum compute units (fractional vCPU equivalent)
    pub max_compute_units: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_microvm_config_default() {
        let config = MicrovmConfig::default();
        assert_eq!(config.memory_mb, 128);
        assert_eq!(config.vcpu_count(), 1);
    }

    #[test]
    fn test_microvm_config_builder() {
        let app_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let config = MicrovmConfig::new(app_id, vm_id)
            .with_memory(256)
            .with_cpu_shares(2048);

        assert_eq!(config.app_id, app_id);
        assert_eq!(config.vm_id, vm_id);
        assert_eq!(config.memory_mb, 256);
        assert_eq!(config.vcpu_count(), 2);
    }

    #[test]
    fn test_drive_config_rootfs() {
        let drive = DriveConfig::rootfs(PathBuf::from("/var/lib/app/rootfs.ext4"));
        assert_eq!(drive.drive_id, "rootfs");
        assert!(drive.is_root_device);
        assert!(drive.is_read_only);
    }

    #[test]
    fn test_network_interface_new() {
        let iface = NetworkInterface::new(
            "eth0",
            "tap-abc123",
            "02:00:00:00:00:01",
            "10.0.0.2",
            "10.0.0.1",
        );

        assert_eq!(iface.iface_id, "eth0");
        assert_eq!(iface.host_dev_name, "tap-abc123");
    }

    #[test]
    fn test_microvm_config_serialization() {
        let config = MicrovmConfig {
            app_id: Uuid::nil(),
            vm_id: Uuid::nil(),
            memory_mb: 256,
            cpu_shares: 2048,
            kernel_path: PathBuf::from("/vmlinux"),
            kernel_boot_args: "console=ttyS0".to_string(),
            drives: vec![DriveConfig::rootfs(PathBuf::from("/rootfs"))],
            network_interfaces: vec![],
            vsock_path: "/run/vsock.sock".to_string(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let decoded: MicrovmConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.memory_mb, decoded.memory_mb);
        assert_eq!(config.cpu_shares, decoded.cpu_shares);
    }
}
