//! Firecracker API Models
//!
//! Types for the Firecracker microVM API, generated from API specification v1.16.0-dev.
//! Latest stable release: v1.14.1
//!
//! ## Module Organization
//!
//! | Module | Types |
//! |--------|-------|
//! | `instance` | InstanceInfo, InstanceState, FirecrackerVersion |
//! | `boot` | BootSource |
//! | `machine` | MachineConfiguration, CpuTemplate, HugePages, CpuConfig |
//! | `drives` | Drive, PartialDrive, CacheType, IoEngine, Pmem |
//! | `network` | NetworkInterface, PartialNetworkInterface, RateLimiter, TokenBucket |
//! | `balloon` | Balloon, BalloonUpdate, BalloonStats, BalloonStatsUpdate |
//! | `devices` | Vsock, EntropyDevice, SerialDevice |
//! | `logging` | Logger, LogLevel, Metrics |
//! | `metrics` | FirecrackerMetrics, VmmMetrics, NetMetrics, BlockMetrics |
//! | `actions` | InstanceActionInfo, ActionType, Vm, VmState |
//! | `snapshot` | SnapshotCreateParams, SnapshotLoadParams, SnapshotType, MemoryBackend, MemoryBackendType, NetworkOverride |
//! | `memory` | MemoryHotplugConfig, MemoryHotplugSizeUpdate, MemoryHotplugStatus |
//! | `mmds` | MmdsConfig, MmdsVersion |
//! | `full_config` | FullVmConfiguration |
//! | `error` | Error |

pub mod instance;
pub mod boot;
pub mod machine;
pub mod drives;
pub mod network;
pub mod balloon;
pub mod devices;
pub mod logging;
pub mod metrics;
pub mod actions;
pub mod snapshot;
pub mod memory;
pub mod mmds;
pub mod full_config;
pub mod error;

// Re-export commonly used types at module level
pub use instance::{InstanceInfo, InstanceState, FirecrackerVersion};
pub use boot::BootSource;
pub use machine::{MachineConfiguration, CpuTemplate, HugePages, CpuConfig, CpuidLeafModifier, CpuidRegisterModifier, MsrModifier, ArmRegisterModifier, VcpuFeatures, CpuidRegister};
pub use drives::{Drive, PartialDrive, CacheType, IoEngine, Pmem};
pub use network::{NetworkInterface, PartialNetworkInterface, RateLimiter, TokenBucket};
pub use balloon::{Balloon, BalloonUpdate, BalloonStats, BalloonStatsUpdate, BalloonStartCmd, BalloonHintingStatus};
pub use devices::{Vsock, EntropyDevice, SerialDevice};
pub use logging::{Logger, LogLevel, Metrics};
pub use metrics::{FirecrackerMetrics, VmmMetrics, NetMetrics, BlockMetrics};
pub use actions::{InstanceActionInfo, ActionType, Vm, VmState};
pub use snapshot::{SnapshotCreateParams, SnapshotLoadParams, SnapshotType, MemoryBackend, MemoryBackendType, NetworkOverride};
pub use memory::{MemoryHotplugConfig, MemoryHotplugSizeUpdate, MemoryHotplugStatus};
pub use mmds::{MmdsConfig, MmdsVersion, MmdsContentsObject};
pub use full_config::FullVmConfiguration;
pub use error::Error;
