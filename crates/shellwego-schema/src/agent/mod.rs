//! Agent types for ShellWeGo
//!
//! This module contains types for agent configuration and capabilities
//! used by worker nodes in the ShellWeGo cluster.

pub mod capabilities;
pub mod config;
pub mod wasm;
pub mod snapshot;
pub mod desired_state;

// Re-export commonly used types at module level
pub use capabilities::{Capabilities, NodeCapacity};
pub use config::{AgentConfig, AgentConfigJson};
pub use wasm::{WasmRuntimeConfig, WasmRuntimeStats, WasmExitStatus};
pub use snapshot::{AgentSnapshotType, AgentSnapshotInfo};
pub use desired_state::{DesiredState, DesiredApp, DesiredVolume, VolumeMount};
