//! # ShellWeGo Firecracker SDK
//!
//! A comprehensive Rust SDK for the Firecracker microVM API.
//!
//! Based on Firecracker API specification v1.16.0-dev
//! Latest stable release: v1.14.1
//!
//! ## Features
//!
//! - Full API coverage for all Firecracker endpoints
//! - Async/await support with Tokio
//! - Type-safe models with serde serialization
//! - Unix Domain Socket communication
//!
//! ## Example
//!
//! ```rust,no_run
//! use shellwego_firecracker::{FirecrackerClient, models::*};
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = FirecrackerClient::new(Path::new("/tmp/firecracker.sock"));
//!
//!     // Configure boot source
//!     client.put_boot_source(BootSource {
//!         kernel_image_path: "/path/to/vmlinux".to_string(),
//!         boot_args: Some("console=ttyS0 reboot=k panic=1 pci=off".to_string()),
//!         initrd_path: None,
//!     }).await?;
//!
//!     // Configure machine
//!     client.put_machine_config(MachineConfiguration {
//!         vcpu_count: 2,
//!         mem_size_mib: 1024,
//!         ..Default::default()
//!     }).await?;
//!
//!     // Start the instance
//!     client.start_instance().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## API Version Support
//!
//! This SDK supports Firecracker API v1.x, including:
//! - v1.7.0 (tested)
//! - v1.8.x
//! - v1.9.x
//! - v1.10.x
//! - v1.11.x
//! - v1.12.x
//! - v1.13.x
//! - v1.14.x (latest stable)
//!
//! ## New Features in Recent Versions
//!
//! ### v1.13+
//! - PCIe support
//! - GPU/hardware acceleration support (experimental)
//!
//! ### v1.10+
//! - Memory hotplug (virtio-mem)
//! - Free page hinting
//!
//! ### v1.8+
//! - Balloon device
//! - Vsock device
//! - Entropy device
//! - MMDS v2

pub mod vmm;

// Re-export models from schema crate
pub mod models {
    //! Firecracker API models re-exported from shellwego-schema
    pub use shellwego_schema::firecracker::*;
}

// Re-export main client
pub use vmm::client::FirecrackerClient;

// Re-export commonly used models from schema
pub use shellwego_schema::firecracker::{
    ActionType,

    // Balloon
    Balloon,
    BalloonHintingStatus,

    BalloonStartCmd,
    BalloonStats,
    BalloonStatsUpdate,
    BalloonUpdate,
    // Boot
    BootSource,

    CacheType,
    // CPU
    CpuConfig,

    CpuTemplate,
    // Drives
    Drive,
    // Entropy
    EntropyDevice,

    // Error
    Error,
    FirecrackerMetrics,

    FirecrackerVersion,

    // Full Config
    FullVmConfiguration,

    HugePages,

    // Actions
    InstanceActionInfo,
    // Instance
    InstanceInfo,
    InstanceState,
    IoEngine,

    LogLevel,
    // Logger & Metrics
    Logger,
    // Machine
    MachineConfiguration,
    MemoryBackend,
    MemoryBackendType,
    // Memory Hotplug
    MemoryHotplugConfig,
    MemoryHotplugSizeUpdate,
    MemoryHotplugStatus,

    Metrics,
    // MMDS
    MmdsConfig,
    MmdsContentsObject,

    MmdsVersion,
    // Network
    NetworkInterface,
    NetworkOverride,

    PartialDrive,
    PartialNetworkInterface,

    // PMEM
    Pmem,

    // Rate Limiting
    RateLimiter,
    // Serial
    SerialDevice,

    // Snapshot
    SnapshotCreateParams,
    SnapshotLoadParams,
    SnapshotType,
    TokenBucket,

    // VM State
    Vm,
    VmState,

    // Vsock
    Vsock,
};

/// SDK version
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Supported Firecracker API version
pub const FIRECRACKER_API_VERSION: &str = "1.16.0-dev";

/// Minimum supported Firecracker version
pub const MIN_FIRECRACKER_VERSION: &str = "1.7.0";

/// Maximum tested Firecracker version
pub const MAX_TESTED_FIRECRACKER_VERSION: &str = "1.14.1";
