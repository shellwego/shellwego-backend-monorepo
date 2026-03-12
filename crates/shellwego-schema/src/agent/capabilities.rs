//! Agent capabilities types
//!
//! Types for describing agent node capabilities.

use serde::{Deserialize, Serialize};
use crate::vmm::VirtualizationMode;

/// System capabilities detected at runtime
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Capabilities {
    /// Detected virtualization mode
    pub virtualization_mode: VirtualizationMode,
    /// KVM is available
    pub kvm_available: bool,
    /// PVM is available
    pub pvm_available: bool,
    /// WASM runtime is available
    pub wasm_available: bool,
    /// Number of CPU cores
    pub cpu_cores: u32,
    /// Total memory in MB
    pub memory_total_mb: u64,
    /// CPU features detected
    pub cpu_features: Vec<String>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            virtualization_mode: VirtualizationMode::default(),
            kvm_available: false,
            pvm_available: false,
            wasm_available: true, // WASM is always available
            cpu_cores: 1,
            memory_total_mb: 1024,
            cpu_features: vec![],
        }
    }
}

impl Capabilities {
    /// Create a new capabilities struct
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a specific virtualization mode is available
    pub fn is_mode_available(&self, mode: VirtualizationMode) -> bool {
        match mode {
            VirtualizationMode::Kvm => self.kvm_available,
            VirtualizationMode::Pvm => self.pvm_available,
            VirtualizationMode::Wasm => self.wasm_available,
        }
    }

    /// Get the best available virtualization mode
    pub fn best_available_mode(&self) -> VirtualizationMode {
        if self.kvm_available {
            VirtualizationMode::Kvm
        } else if self.pvm_available {
            VirtualizationMode::Pvm
        } else {
            VirtualizationMode::Wasm
        }
    }

    /// Set CPU cores
    pub fn with_cpu_cores(mut self, cores: u32) -> Self {
        self.cpu_cores = cores;
        self
    }

    /// Set total memory
    pub fn with_memory(mut self, mb: u64) -> Self {
        self.memory_total_mb = mb;
        self
    }

    /// Set KVM availability
    pub fn with_kvm(mut self, available: bool) -> Self {
        self.kvm_available = available;
        if available {
            self.virtualization_mode = VirtualizationMode::Kvm;
        }
        self
    }

    /// Set PVM availability
    pub fn with_pvm(mut self, available: bool) -> Self {
        self.pvm_available = available;
        if available && !self.kvm_available {
            self.virtualization_mode = VirtualizationMode::Pvm;
        }
        self
    }

    /// Add a CPU feature
    pub fn with_cpu_feature(mut self, feature: &str) -> Self {
        self.cpu_features.push(feature.to_string());
        self
    }

    /// Calculate available memory for workloads (total - reserved)
    pub fn available_memory_mb(&self, reserved_mb: u64) -> u64 {
        self.memory_total_mb.saturating_sub(reserved_mb)
    }

    /// Calculate available CPU percentage (100% - reserved)
    pub fn available_cpu_percent(&self, reserved_percent: f64) -> f64 {
        (100.0 - reserved_percent).max(0.0)
    }
}

/// Node resource capacity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct NodeCapacity {
    /// CPU cores
    pub cpu_cores: u32,
    /// Total memory in MB
    pub memory_mb: u64,
    /// Available memory for workloads
    pub available_memory_mb: u64,
    /// Total disk space in GB
    pub disk_gb: u64,
    /// Maximum microVMs
    pub max_microvms: u32,
    /// Current microVM count
    pub current_microvms: u32,
}

impl Default for NodeCapacity {
    fn default() -> Self {
        Self {
            cpu_cores: 1,
            memory_mb: 1024,
            available_memory_mb: 512,
            disk_gb: 100,
            max_microvms: 500,
            current_microvms: 0,
        }
    }
}

impl NodeCapacity {
    /// Calculate remaining capacity
    pub fn remaining_capacity(&self) -> (u32, u64) {
        let remaining_vms = self.max_microvms.saturating_sub(self.current_microvms);
        let remaining_memory = self.available_memory_mb;
        (remaining_vms, remaining_memory)
    }

    /// Check if the node can accept more workloads
    pub fn can_accept(&self, memory_mb: u64) -> bool {
        self.current_microvms < self.max_microvms && self.available_memory_mb >= memory_mb
    }

    /// Get utilization percentage
    pub fn utilization_percent(&self) -> f64 {
        if self.max_microvms == 0 {
            return 0.0;
        }
        (self.current_microvms as f64 / self.max_microvms as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_default() {
        let caps = Capabilities::default();
        assert_eq!(caps.virtualization_mode, VirtualizationMode::Pvm);
        assert!(caps.wasm_available);
        assert!(!caps.kvm_available);
    }

    #[test]
    fn test_capabilities_is_mode_available() {
        let caps = Capabilities {
            kvm_available: true,
            pvm_available: true,
            wasm_available: true,
            ..Default::default()
        };

        assert!(caps.is_mode_available(VirtualizationMode::Kvm));
        assert!(caps.is_mode_available(VirtualizationMode::Pvm));
        assert!(caps.is_mode_available(VirtualizationMode::Wasm));
    }

    #[test]
    fn test_capabilities_best_available_mode() {
        // KVM is best
        let caps = Capabilities {
            kvm_available: true,
            pvm_available: true,
            wasm_available: true,
            ..Default::default()
        };
        assert_eq!(caps.best_available_mode(), VirtualizationMode::Kvm);

        // PVM is second best
        let caps = Capabilities {
            kvm_available: false,
            pvm_available: true,
            wasm_available: true,
            ..Default::default()
        };
        assert_eq!(caps.best_available_mode(), VirtualizationMode::Pvm);

        // WASM is fallback
        let caps = Capabilities {
            kvm_available: false,
            pvm_available: false,
            wasm_available: true,
            ..Default::default()
        };
        assert_eq!(caps.best_available_mode(), VirtualizationMode::Wasm);
    }

    #[test]
    fn test_node_capacity_can_accept() {
        let capacity = NodeCapacity {
            max_microvms: 100,
            current_microvms: 50,
            available_memory_mb: 1024,
            ..Default::default()
        };

        assert!(capacity.can_accept(512));
        assert!(!capacity.can_accept(2048));
    }

    #[test]
    fn test_node_capacity_utilization() {
        let capacity = NodeCapacity {
            max_microvms: 100,
            current_microvms: 50,
            ..Default::default()
        };

        assert!((capacity.utilization_percent() - 50.0).abs() < 0.001);
    }
}
