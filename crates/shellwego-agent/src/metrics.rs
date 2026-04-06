//! Agent-local metrics collection and export, wired to shared observability registry

use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use shellwego_observability::metrics::{MetricsRegistry, builtin};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use sysinfo::{Disks, System};
use tokio::net::TcpListener;
use tracing::info;

/// Agent metrics collector — delegates to shared MetricsRegistry
pub struct MetricsCollector {
    node_id: uuid::Uuid,
    registry: Arc<MetricsRegistry>,
    system: Arc<Mutex<System>>,
    disks: Arc<Mutex<Disks>>,
    /// Running microVM count, updated by VmmManager
    microvm_count: AtomicU32,
}

impl MetricsCollector {
    /// Create collector with shared metrics registry
    pub fn new(node_id: uuid::Uuid) -> Self {
        let registry = Arc::new(MetricsRegistry::new());
        builtin::register_builtin(registry.inner_registry())
            .expect("Failed to register builtin metrics");

        let mut system = System::new_all();
        system.refresh_all();
        let disks = Disks::new_with_refreshed_list();

        Self {
            node_id,
            registry,
            system: Arc::new(Mutex::new(system)),
            disks: Arc::new(Mutex::new(disks)),
            microvm_count: AtomicU32::new(0),
        }
    }

    /// Get reference to the shared metrics registry
    pub fn registry(&self) -> &Arc<MetricsRegistry> {
        &self.registry
    }

    /// Update the running microVM count (called by VmmManager)
    pub fn set_microvm_count(&self, count: u32) {
        self.microvm_count.store(count, Ordering::Relaxed);
        tracing::debug!("Updating microVM count to {}", count);
    }

    /// Record microVM spawn duration using shared histogram
    pub fn record_spawn(&self, duration_ms: u64, success: bool) {
        let node_name = &self.node_id.to_string();
        let duration_secs = duration_ms as f64 / 1000.0;
        builtin::MICROVM_SPAWN_DURATION
            .with_label_values(&[node_name, "unknown"])
            .observe(duration_secs);

        let status = if success { "success" } else { "failed" };
        builtin::DEPLOYMENT_COUNT
            .with_label_values(&[node_name, status, "unknown"])
            .inc();

        info!(
            event = "microvm_spawn",
            duration_ms = duration_ms,
            success = success,
            node_id = %self.node_id
        );
    }

    /// Update all node-level metrics from current system state
    pub fn refresh_metrics(&self) {
        let node_name = &self.node_id.to_string();
        let mut sys = self.system.lock().unwrap();
        sys.refresh_cpu();
        sys.refresh_memory();

        let total_mem = sys.total_memory() as f64;
        let used_mem = sys.used_memory() as f64;
        let available_mem = (sys.available_memory()) as f64;
        let pressure = if total_mem > 0.0 { used_mem / total_mem } else { 0.0 };

        // Update memory metrics
        builtin::NODE_MEMORY_USAGE.with_label_values(&[node_name, "total"]).set(total_mem);
        builtin::NODE_MEMORY_USAGE.with_label_values(&[node_name, "used"]).set(used_mem);
        builtin::NODE_MEMORY_USAGE.with_label_values(&[node_name, "available"]).set(available_mem);
        builtin::NODE_MEMORY_PRESSURE.with_label_values(&[node_name]).set(pressure);

        // Update microVM count
        builtin::APPS_RUNNING.with_label_values(&[node_name, "running"])
            .set(self.microvm_count.load(Ordering::Relaxed) as f64);

        // Storage usage
        let disks = self.disks.lock().unwrap();
        let (disk_total, disk_used) = disks.list().iter().fold((0u64, 0u64), |acc, d| {
            (acc.0 + d.total_space(), acc.1 + (d.total_space() - d.available_space()))
        });
        let storage_ratio = if disk_total > 0 { disk_used as f64 / disk_total as f64 } else { 0.0 };
        builtin::STORAGE_POOL_USAGE.with_label_values(&[node_name, "default"]).set(storage_ratio);
    }

    /// Get current snapshot
    pub fn get_snapshot(&self) -> ResourceSnapshot {
        let mut sys = self.system.lock().unwrap();
        sys.refresh_cpu();
        sys.refresh_memory();

        let total_mem = sys.total_memory();
        let used_mem = sys.used_memory();
        let available_mem = sys.available_memory();

        let cpu_usage = sys.global_cpu_info().cpu_usage();

        let disks = self.disks.lock().unwrap();
        let (disk_total, disk_used) = disks.list().iter().fold((0, 0), |acc, disk| {
            (
                acc.0 + disk.total_space(),
                acc.1 + (disk.total_space() - disk.available_space()),
            )
        });

        ResourceSnapshot {
            memory_total: total_mem,
            memory_used: used_mem,
            memory_available: available_mem,
            cpu_cores: sys.cpus().len() as u32,
            cpu_usage_percent: cpu_usage,
            disk_total,
            disk_used,
            microvm_count: self.microvm_count.load(Ordering::Relaxed),
        }
    }

    /// Generate Prometheus formatted metrics via shared registry
    pub fn generate_prometheus(&self) -> String {
        self.registry.export_text().unwrap_or_default()
    }

    /// Start background collection loop
    pub async fn run_collection_loop(&self) -> Result<(), MetricsError> {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            self.refresh_metrics();
            let mut sys = self.system.lock().unwrap();
            sys.refresh_cpu();
            sys.refresh_memory();
            let mut disks = self.disks.lock().unwrap();
            disks.refresh_list();
        }
    }
}

/// Start the Prometheus exporter HTTP server
pub async fn start_metrics_server(
    collector: Arc<MetricsCollector>,
    port: u16,
) -> Result<(), MetricsError> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| MetricsError::ExportFailed(e.to_string()))?;

    info!("Metrics server listening on http://{}", addr);

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|e| MetricsError::ExportFailed(e.to_string()))?;

        let io = TokioIo::new(stream);
        let collector = collector.clone();

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(
                    io,
                    service_fn(move |_req: Request<hyper::body::Incoming>| {
                        let body = collector.generate_prometheus();
                        async move {
                            Ok::<_, anyhow::Error>(Response::new(Full::new(Bytes::from(body))))
                        }
                    }),
                )
                .await
            {
                info!("Error serving metrics: {:?}", err);
            }
        });
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
