//! Agent types for ShellWeGo
//!
//! This module contains types for agent configuration and capabilities
//! used by worker nodes in the ShellWeGo cluster.

pub mod capabilities;
pub mod config;

// Re-export commonly used types at module level
pub use capabilities::{Capabilities, NodeCapacity};
pub use config::{AgentConfig, AgentConfigJson};
