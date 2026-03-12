//! MicroVM state types
//!
//! Defines the runtime state of microVMs and summary information.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Runtime state of a microVM
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum MicrovmState {
    /// VM is not initialized
    Uninitialized,
    /// VM is configured but not started
    Configured,
    /// VM is running
    Running,
    /// VM is paused
    Paused,
    /// VM is halted/stopped
    Halted,
}

impl Default for MicrovmState {
    fn default() -> Self {
        Self::Uninitialized
    }
}

impl std::fmt::Display for MicrovmState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MicrovmState::Uninitialized => write!(f, "Uninitialized"),
            MicrovmState::Configured => write!(f, "Configured"),
            MicrovmState::Running => write!(f, "Running"),
            MicrovmState::Paused => write!(f, "Paused"),
            MicrovmState::Halted => write!(f, "Halted"),
        }
    }
}

/// Summary of a running microVM
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct MicrovmSummary {
    /// Application ID
    pub app_id: Uuid,
    /// VM instance ID
    pub vm_id: Uuid,
    /// Current state of the VM
    pub state: MicrovmState,
    /// When the VM was started
    pub started_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_microvm_state_default() {
        assert_eq!(MicrovmState::default(), MicrovmState::Uninitialized);
    }

    #[test]
    fn test_microvm_state_display() {
        assert_eq!(MicrovmState::Running.to_string(), "Running");
        assert_eq!(MicrovmState::Paused.to_string(), "Paused");
    }

    #[test]
    fn test_microvm_state_serialization() {
        for state in [
            MicrovmState::Uninitialized,
            MicrovmState::Configured,
            MicrovmState::Running,
            MicrovmState::Paused,
            MicrovmState::Halted,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let decoded: MicrovmState = serde_json::from_str(&json).unwrap();
            assert_eq!(state, decoded);
        }
    }

    #[test]
    fn test_microvm_summary_debug() {
        let summary = MicrovmSummary {
            app_id: Uuid::nil(),
            vm_id: Uuid::nil(),
            state: MicrovmState::Running,
            started_at: Utc::now(),
        };
        let debug_str = format!("{:?}", summary);
        assert!(debug_str.contains("MicrovmSummary"));
    }
}
