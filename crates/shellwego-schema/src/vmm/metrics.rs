//! MicroVM metrics types
//!
//! Defines metrics collected from running microVMs.

use serde::{Deserialize, Serialize};

/// Metrics from a running microVM
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct MicrovmMetrics {
    /// CPU usage in microseconds
    pub cpu_usage_usec: u64,
    /// Resident set memory in bytes
    pub memory_rss_bytes: u64,
    /// Network bytes received
    pub network_rx_bytes: u64,
    /// Network bytes transmitted
    pub network_tx_bytes: u64,
    /// Block device bytes read
    pub block_read_bytes: u64,
    /// Block device bytes written
    pub block_write_bytes: u64,
}

impl MicrovmMetrics {
    /// Create new empty metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Get CPU usage as a percentage (requires total CPU time)
    pub fn cpu_usage_percent(&self, total_cpu_usec: u64) -> f64 {
        if total_cpu_usec == 0 {
            return 0.0;
        }
        (self.cpu_usage_usec as f64 / total_cpu_usec as f64) * 100.0
    }

    /// Get memory usage in MB
    pub fn memory_mb(&self) -> u64 {
        self.memory_rss_bytes / (1024 * 1024)
    }

    /// Get total network I/O in bytes
    pub fn total_network_bytes(&self) -> u64 {
        self.network_rx_bytes + self.network_tx_bytes
    }

    /// Get total block I/O in bytes
    pub fn total_block_bytes(&self) -> u64 {
        self.block_read_bytes + self.block_write_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_microvm_metrics_default() {
        let metrics = MicrovmMetrics::default();

        assert_eq!(metrics.cpu_usage_usec, 0);
        assert_eq!(metrics.memory_rss_bytes, 0);
        assert_eq!(metrics.network_rx_bytes, 0);
        assert_eq!(metrics.network_tx_bytes, 0);
        assert_eq!(metrics.block_read_bytes, 0);
        assert_eq!(metrics.block_write_bytes, 0);
    }

    #[test]
    fn test_microvm_metrics_new() {
        let metrics = MicrovmMetrics::new();
        assert_eq!(metrics.cpu_usage_usec, 0);
    }

    #[test]
    fn test_microvm_metrics_cpu_percent() {
        let metrics = MicrovmMetrics {
            cpu_usage_usec: 500_000,
            ..Default::default()
        };

        let percent = metrics.cpu_usage_percent(1_000_000);
        assert!((percent - 50.0).abs() < 0.001);
    }

    #[test]
    fn test_microvm_metrics_memory_mb() {
        let metrics = MicrovmMetrics {
            memory_rss_bytes: 128 * 1024 * 1024, // 128 MB
            ..Default::default()
        };

        assert_eq!(metrics.memory_mb(), 128);
    }

    #[test]
    fn test_microvm_metrics_serialization() {
        let metrics = MicrovmMetrics {
            cpu_usage_usec: 1_000_000,
            memory_rss_bytes: 128 * 1024 * 1024,
            network_rx_bytes: 10_000_000,
            network_tx_bytes: 5_000_000,
            block_read_bytes: 20_000_000,
            block_write_bytes: 10_000_000,
        };

        let json = serde_json::to_string(&metrics).unwrap();
        let decoded: MicrovmMetrics = serde_json::from_str(&json).unwrap();

        assert_eq!(metrics.cpu_usage_usec, decoded.cpu_usage_usec);
        assert_eq!(metrics.memory_rss_bytes, decoded.memory_rss_bytes);
    }
}
