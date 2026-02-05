//! Agent-local metrics collection and export

use std::sync::{Arc, Mutex};
use sysinfo::{Disks, System};
use tracing::info;

/// Agent metrics collector
pub struct MetricsCollector {
    node_id: uuid::Uuid,
    system: Arc<Mutex<System>>,
    disks: Arc<Mutex<Disks>>,
}

impl MetricsCollector {
    /// Create collector
    pub fn new(node_id: uuid::Uuid) -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        let disks = Disks::new_with_refreshed_list();
        
        Self {
            node_id,
            system: Arc::new(Mutex::new(system)),
            disks: Arc::new(Mutex::new(disks)),
        }
    }

    /// Record microVM spawn duration
    pub fn record_spawn(&self, duration_ms: u64, success: bool) {
        // In a real Prometheus setup, we would update a Histogram here.
        // For now, we log structured data that can be scraped or piped.
        info!(
            event = "microvm_spawn",
            duration_ms = duration_ms,
            success = success,
            node_id = %self.node_id
        );
    }

    /// Update resource gauges
    pub async fn update_resources(&self) {
        let mut sys = self.system.lock().unwrap();
        sys.refresh_cpu();
        sys.refresh_memory();
        
        let mut disks = self.disks.lock().unwrap();
        disks.refresh_list();
    }

    /// Export metrics to control plane
    pub async fn export(&self) -> Result<(), MetricsError> {
        // This would typically push to an endpoint or expose a /metrics endpoint.
        // For the agent, we might piggyback on the heartbeat.
        Ok(())
    }

    /// Get current snapshot
    pub fn get_snapshot(&self) -> ResourceSnapshot {
        let mut sys = self.system.lock().unwrap();
        // Refresh specific components if needed, or rely on update loop
        sys.refresh_cpu();
        sys.refresh_memory();

        let total_mem = sys.total_memory();
        let used_mem = sys.used_memory();
        let available_mem = sys.available_memory();
        
        let cpu_usage = sys.global_cpu_info().cpu_usage();
        
        // Simple disk summation
        let disks = self.disks.lock().unwrap();
        let (disk_total, disk_used) = disks.list().iter().fold((0, 0), |acc, disk| {
            (acc.0 + disk.total_space(), acc.1 + (disk.total_space() - disk.available_space()))
        });

        ResourceSnapshot {
            memory_total: total_mem,
            memory_used: used_mem,
            memory_available: available_mem,
            cpu_cores: sys.cpus().len() as u32,
            cpu_usage_percent: cpu_usage,
            disk_total,
            disk_used,
            microvm_count: 0, // Needs VMM integration to get this accurate
        }
    }

    /// Start background collection loop
    pub async fn run_collection_loop(&self) -> Result<(), MetricsError> {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            self.update_resources().await;
        }
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
    pub memory_total: u64,
    pub memory_used: u64,
    pub memory_available: u64,
    pub cpu_cores: u32,
    pub cpu_usage_percent: f32,
    pub disk_total: u64,
    pub disk_used: u64,
    pub microvm_count: u32,
}