//! Virtualization mode types
//!
//! Defines the available virtualization backends for running workloads.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Virtualization mode for running workloads
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum VirtualizationMode {
    /// KVM hardware virtualization (fastest, requires /dev/kvm)
    Kvm,
    /// PVM software virtualization (universal, no KVM required)
    Pvm,
    /// WASM runtime (lightest, for functions only)
    Wasm,
}

impl Default for VirtualizationMode {
    fn default() -> Self {
        // Default to PVM as the universal fallback
        VirtualizationMode::Pvm
    }
}

impl fmt::Display for VirtualizationMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VirtualizationMode::Kvm => write!(f, "KVM"),
            VirtualizationMode::Pvm => write!(f, "PVM"),
            VirtualizationMode::Wasm => write!(f, "WASM"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtualization_mode_default() {
        assert_eq!(VirtualizationMode::default(), VirtualizationMode::Pvm);
    }

    #[test]
    fn test_virtualization_mode_display() {
        assert_eq!(format!("{}", VirtualizationMode::Kvm), "KVM");
        assert_eq!(format!("{}", VirtualizationMode::Pvm), "PVM");
        assert_eq!(format!("{}", VirtualizationMode::Wasm), "WASM");
    }

    #[test]
    fn test_virtualization_mode_serialization() {
        let mode = VirtualizationMode::Pvm;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"Pvm\"");

        let decoded: VirtualizationMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, decoded);
    }

    #[test]
    fn test_virtualization_mode_equality() {
        assert_eq!(VirtualizationMode::Kvm, VirtualizationMode::Kvm);
        assert_ne!(VirtualizationMode::Kvm, VirtualizationMode::Pvm);
    }
}
