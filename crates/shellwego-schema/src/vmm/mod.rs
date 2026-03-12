//! Virtual Machine Manager types
//!
//! This module contains all types related to microVM configuration, state,
//! and metrics. These types are used by the agent for managing Firecracker
//! microVMs and by the control plane for scheduling decisions.

pub mod config;
pub mod metrics;
pub mod state;
pub mod virtualization;

// Re-export commonly used types at module level
pub use config::{
    DriveConfig, MicrovmConfig, NetworkInterface, RateLimiterConfig, WasmConfig,
};
pub use metrics::MicrovmMetrics;
pub use state::{MicrovmState, MicrovmSummary};
pub use virtualization::VirtualizationMode;
