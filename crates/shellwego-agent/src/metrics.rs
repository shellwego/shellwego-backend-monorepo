//! Agent-local metrics collection and export

use std::sync::{Arc, Mutex};
use std::net::SocketAddr;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use http_body_util::Full;
use bytes::Bytes;
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

    /// Generate Prometheus formatted metrics
    pub fn generate_prometheus(&self) -> String {
        let snap = self.get_snapshot();
        let mut buffer = String::new();

        // Node resources
        let _ = std::fmt::Write::write_fmt(&mut buffer, format_args!(
            "# HELP shellwego_node_memory_bytes Node memory stats\n\
             # TYPE shellwego_node_memory_bytes gauge\n\
             shellwego_node_memory_bytes{{type=\"total\"}} {}\n\
             shellwego_node_memory_bytes{{type=\"used\"}} {}\n\
             shellwego_node_memory_bytes{{type=\"available\"}} {}\n",
            snap.memory_total, snap.memory_used, snap.memory_available
        ));

        let _ = std::fmt::Write::write_fmt(&mut buffer, format_args!(
            "# HELP shellwego_node_cpu_percent Node CPU usage\n\
             # TYPE shellwego_node_cpu_percent gauge\n\
             shellwego_node_cpu_percent {}\n",
            snap.cpu_usage_percent
        ));

        let _ = std::fmt::Write::write_fmt(&mut buffer, format_args!(
            "# HELP shellwego_microvm_count Number of running microVMs\n\
             # TYPE shellwego_microvm_count gauge\n\
             shellwego_microvm_count {}\n",
            snap.microvm_count
        ));

        // TODO: Add metrics per microVM (needs VMM integration here)
        
        buffer
    }

    /// Start background collection loop
    pub async fn run_collection_loop(&self) -> Result<(), MetricsError> {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            // Background refresh logic
            {
                let mut sys = self.system.lock().unwrap();
                sys.refresh_cpu();
                sys.refresh_memory();
                let mut disks = self.disks.lock().unwrap();
                disks.refresh_list();
            }
        }
    }
}

/// Start the Prometheus exporter HTTP server
pub async fn start_metrics_server(
    collector: Arc<MetricsCollector>, 
    port: u16
) -> Result<(), MetricsError> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await
        .map_err(|e| MetricsError::ExportFailed(e.to_string()))?;
        
    info!("Metrics server listening on http://{}", addr);

    loop {
        let (stream, _) = listener.accept().await
            .map_err(|e| MetricsError::ExportFailed(e.to_string()))?;
            
        let io = TokioIo::new(stream);
        let collector = collector.clone();
        
        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(move |_req: Request<hyper::body::Incoming>| {
                    let body = collector.generate_prometheus();
                    async move {
                        Ok::<_, anyhow::Error>(Response::new(Full::new(Bytes::from(body))))
                    }
                }))
                .await
            {
                info!("Error serving metrics: {:?}", err);
            }
        });
    }
}

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