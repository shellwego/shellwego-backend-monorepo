//! WASM runtime types for ShellWeGo agent
//!
//! Configuration and types for the WebAssembly runtime used for lightweight workloads.

use serde::{Deserialize, Serialize};

/// WASM runtime configuration.
///
/// Configuration for the WebAssembly runtime used for lightweight workloads.
/// This is distinct from the VMM's WasmConfig which is for microVM fallback.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct WasmRuntimeConfig {
    /// Maximum memory in MB for WASM instances
    pub max_memory_mb: u32,
    /// Maximum compute units (fractional vCPU equivalent)
    pub max_compute_units: f64,
    /// Maximum concurrent instances
    pub max_instances: u32,
    /// Optional directory path for caching compiled WASM artifacts.
    /// If set, serialized `.cwasm` files are stored here for faster cold starts.
    /// If empty or not set, only in-memory caching is used.
    #[serde(default)]
    pub cache_dir: Option<String>,
}

impl Default for WasmRuntimeConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: 128,
            max_compute_units: 1.0,
            max_instances: 100,
            cache_dir: None,
        }
    }
}

/// WASM runtime statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct WasmRuntimeStats {
    /// Number of active WASM instances
    pub active_instances: u32,
    /// Total memory used in MB
    pub total_memory_mb: u64,
    /// Total compute units used
    pub total_compute_units: f64,
}

/// WASM instance exit status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct WasmExitStatus {
    /// Whether the instance exited successfully
    pub success: bool,
    /// Exit code (0 for success)
    pub code: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_runtime_config_default() {
        let config = WasmRuntimeConfig::default();
        assert_eq!(config.max_memory_mb, 128);
        assert_eq!(config.max_compute_units, 1.0);
        assert_eq!(config.max_instances, 100);
        assert!(config.cache_dir.is_none());
    }

    #[test]
    fn test_wasm_runtime_stats_default() {
        let stats = WasmRuntimeStats::default();
        assert_eq!(stats.active_instances, 0);
        assert_eq!(stats.total_memory_mb, 0);
    }
}
