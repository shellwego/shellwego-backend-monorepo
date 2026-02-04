//! Agent-local metrics collection and export

use std::collections::HashMap;

/// Agent metrics collector
pub struct MetricsCollector {
    // TODO: Add registry, node_id, control_plane_client
}

impl MetricsCollector {
    /// Create collector
    pub fn new(node_id: uuid::Uuid) -> Self {
        // TODO: Initialize with node identification
        unimplemented!("MetricsCollector::new")
    }

    /// Record microVM spawn duration
    pub fn record_spawn(&self, duration_ms: u64, success: bool) {
        // TODO: Increment counter with labels
        unimplemented!("MetricsCollector::record_spawn")
    }

    /// Update resource gauges
    pub async fn update_resources(&self) {
        // TODO: Read /proc/meminfo, /proc/stat
        // TODO: Read ZFS pool usage
        // TODO: Update gauges
        unimplemented!("MetricsCollector::update_resources")
    }

    /// Export metrics to control plane
    pub async fn export(&self) -> Result<(), MetricsError> {
        // TODO: Serialize current metrics
        // TODO: POST to control plane metrics endpoint
        unimplemented!("MetricsCollector::export")
    }

    /// Start background collection loop
    pub async fn run_collection_loop(&self) -> Result<(), MetricsError> {
        // TODO: Periodic resource updates
        // TODO: Periodic export to control plane
        unimplemented!("MetricsCollector::run_collection_loop")
    }
}

/// Metrics error
#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("Export failed: {0}")]
    ExportFailed(String),
}

/// Node resource snapshot
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResourceSnapshot {
    // TODO: Add memory_total, memory_used, memory_available
    // TODO: Add cpu_cores, cpu_usage_percent
    // TODO: Add disk_total, disk_used
    // TODO: Add microvm_count, network_io, disk_io
}