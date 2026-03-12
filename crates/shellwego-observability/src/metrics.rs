//! Prometheus metrics collection and export
//!
//! This module provides a production-ready metrics registry with support for
//! counters, gauges, and histograms. It integrates with Prometheus for scraping
//! and supports push gateway for serverless environments.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;
use prometheus::{
    Counter as PromCounter, CounterVec, Encoder, Gauge as PromGauge, GaugeVec,
    Histogram as PromHistogram, HistogramOpts, HistogramVec, Opts, Registry, TextEncoder,
};
use tokio::sync::broadcast;

use crate::ObservabilityError;

/// Global metrics registry singleton
static mut GLOBAL_REGISTRY: Option<Arc<MetricsRegistry>> = None;

/// Get the global metrics registry (lazily initialized)
pub fn global_registry() -> Option<Arc<MetricsRegistry>> {
    // SAFETY: Only called after initialization
    unsafe { GLOBAL_REGISTRY.clone() }
}

/// Initialize the global metrics registry
pub fn init_global_registry() -> Arc<MetricsRegistry> {
    let registry = Arc::new(MetricsRegistry::new());
    // SAFETY: Called once during startup
    unsafe {
        GLOBAL_REGISTRY = Some(registry.clone());
    }
    registry
}

/// Metrics registry wrapper with process collectors
#[derive(Debug)]
pub struct MetricsRegistry {
    /// Inner Prometheus registry
    registry: Registry,
    /// Registered counters by name
    counters: RwLock<HashMap<String, Counter>>,
    /// Registered gauges by name
    gauges: RwLock<HashMap<String, Gauge>>,
    /// Registered histograms by name
    histograms: RwLock<HashMap<String, Histogram>>,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRegistry {
    /// Create new registry with default process collectors
    pub fn new() -> Self {
        let registry = Registry::new();

        Self {
            registry,
            counters: RwLock::new(HashMap::new()),
            gauges: RwLock::new(HashMap::new()),
            histograms: RwLock::new(HashMap::new()),
        }
    }

    /// Create registry with custom Prometheus registry
    pub fn with_registry(registry: Registry) -> Self {
        Self {
            registry,
            counters: RwLock::new(HashMap::new()),
            gauges: RwLock::new(HashMap::new()),
            histograms: RwLock::new(HashMap::new()),
        }
    }

    /// Register custom counter
    pub fn register_counter(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
    ) -> Result<Counter, ObservabilityError> {
        let opts = Opts::new(name, help);
        let counter_vec =
            CounterVec::new(opts, labels).map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;

        self.registry
            .register(Box::new(counter_vec.clone()))
            .map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;

        let counter = Counter {
            inner: counter_vec,
            name: name.to_string(),
            label_names: labels.iter().map(|s| s.to_string()).collect(),
        };

        self.counters.write().insert(name.to_string(), counter.clone());
        Ok(counter)
    }

    /// Register custom gauge
    pub fn register_gauge(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
    ) -> Result<Gauge, ObservabilityError> {
        let opts = Opts::new(name, help);
        let gauge_vec =
            GaugeVec::new(opts, labels).map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;

        self.registry
            .register(Box::new(gauge_vec.clone()))
            .map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;

        let gauge = Gauge {
            inner: gauge_vec,
            name: name.to_string(),
            label_names: labels.iter().map(|s| s.to_string()).collect(),
        };

        self.gauges.write().insert(name.to_string(), gauge.clone());
        Ok(gauge)
    }

    /// Register histogram with custom buckets
    pub fn register_histogram(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
        buckets: &[f64],
    ) -> Result<Histogram, ObservabilityError> {
        let opts = HistogramOpts::new(name, help).buckets(buckets.to_vec());
        let histogram_vec =
            HistogramVec::new(opts, labels).map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;

        self.registry
            .register(Box::new(histogram_vec.clone()))
            .map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;

        let histogram = Histogram {
            inner: histogram_vec,
            name: name.to_string(),
            label_names: labels.iter().map(|s| s.to_string()).collect(),
        };

        self.histograms.write().insert(name.to_string(), histogram.clone());
        Ok(histogram)
    }

    /// Get or create a counter (idempotent)
    pub fn get_or_create_counter(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
    ) -> Result<Counter, ObservabilityError> {
        if let Some(counter) = self.counters.read().get(name) {
            return Ok(counter.clone());
        }
        self.register_counter(name, help, labels)
    }

    /// Get or create a gauge (idempotent)
    pub fn get_or_create_gauge(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
    ) -> Result<Gauge, ObservabilityError> {
        if let Some(gauge) = self.gauges.read().get(name) {
            return Ok(gauge.clone());
        }
        self.register_gauge(name, help, labels)
    }

    /// Get or create a histogram (idempotent)
    pub fn get_or_create_histogram(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
        buckets: &[f64],
    ) -> Result<Histogram, ObservabilityError> {
        if let Some(histogram) = self.histograms.read().get(name) {
            return Ok(histogram.clone());
        }
        self.register_histogram(name, help, labels, buckets)
    }

    /// Start HTTP server for Prometheus scraping
    pub async fn serve_endpoint(
        self: Arc<Self>,
        bind_addr: &str,
    ) -> Result<MetricsServerHandle, ObservabilityError> {
        let addr: SocketAddr = bind_addr
            .parse()
            .map_err(|e| ObservabilityError::MetricsError(format!("Invalid bind address: {}", e)))?;

        let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);
        let registry = self.registry.clone();

        // Create the server task
        let shutdown_tx_clone = shutdown_tx.clone();
        tokio::spawn(async move {
            let make_svc = hyper::service::make_service_fn(move |_conn| {
                let registry = registry.clone();
                async move {
                    Ok::<_, hyper::Error>(hyper::service::service_fn(move |req| {
                        let registry = registry.clone();
                        async move {
                            match (req.method(), req.uri().path()) {
                                (&hyper::Method::GET, "/metrics") => {
                                    let encoder = TextEncoder::new();
                                    let metric_families = registry.gather();
                                    let mut buffer = Vec::new();
                                    match encoder.encode(&metric_families, &mut buffer) {
                                        Ok(()) => Ok(hyper::Response::builder()
                                            .status(hyper::StatusCode::OK)
                                            .header("Content-Type", "text/plain; version=0.0.4")
                                            .body(hyper::Body::from(buffer))),
                                        Err(e) => Ok(hyper::Response::builder()
                                            .status(hyper::StatusCode::INTERNAL_SERVER_ERROR)
                                            .body(hyper::Body::from(format!("Encoding error: {}", e)))),
                                    }
                                }
                                (&hyper::Method::GET, "/health") => Ok(hyper::Response::builder()
                                    .status(hyper::StatusCode::OK)
                                    .body(hyper::Body::from("OK"))),
                                _ => Ok(hyper::Response::builder()
                                    .status(hyper::StatusCode::NOT_FOUND)
                                    .body(hyper::Body::from("Not Found"))),
                            }
                        }
                    }))
                }
            });

            let server = hyper::Server::bind(&addr).serve(make_svc);

            tokio::select! {
                result = server => {
                    if let Err(e) = result {
                        tracing::error!("Metrics server error: {}", e);
                    }
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("Metrics server shutting down");
                }
            }
        });

        Ok(MetricsServerHandle {
            shutdown_tx: Some(shutdown_tx_clone),
        })
    }

    /// Push metrics to remote Prometheus pushgateway
    pub async fn push_to_gateway(
        &self,
        gateway_url: &str,
        job: &str,
        grouping: HashMap<String, String>,
    ) -> Result<(), ObservabilityError> {
        let metric_families = self.registry.gather();
        let encoder = TextEncoder::new();
        let mut buffer = Vec::new();
        encoder
            .encode(&metric_families, &mut buffer)
            .map_err(|e| ObservabilityError::ExportError(e.to_string()))?;

        // Build URL with grouping labels
        let mut url = format!("{}/metrics/job/{}", gateway_url.trim_end_matches('/'), job);
        for (key, value) in grouping {
            url.push_str(&format!("/{}/{}", key, value));
        }

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header("Content-Type", "text/plain")
            .body(buffer)
            .send()
            .await
            .map_err(|e| ObservabilityError::ExportError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ObservabilityError::ExportError(format!(
                "Push gateway returned status: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Export current metrics as text
    pub fn export_text(&self) -> Result<String, ObservabilityError> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder
            .encode(&metric_families, &mut buffer)
            .map_err(|e| ObservabilityError::ExportError(e.to_string()))?;
        String::from_utf8(buffer).map_err(|e| ObservabilityError::ExportError(e.to_string()))
    }

    /// Get raw metric families for custom export
    pub fn gather(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }
}

/// Counter metric handle
#[derive(Clone, Debug)]
pub struct Counter {
    inner: CounterVec,
    name: String,
    label_names: Vec<String>,
}

impl Counter {
    /// Increment counter by 1
    pub fn inc(&self, labels: &HashMap<String, String>) {
        self.with_labels(labels).inc();
    }

    /// Add value to counter
    pub fn add(&self, value: u64, labels: &HashMap<String, String>) {
        self.with_labels(labels).inc_by(value as f64);
    }

    /// Get counter with specific labels
    fn with_labels(&self, labels: &HashMap<String, String>) -> PromCounter {
        let label_values: Vec<&str> = self
            .label_names
            .iter()
            .map(|name| labels.get(name).map(|s| s.as_str()).unwrap_or(""))
            .collect();
        self.inner.with_label_values(&label_values)
    }

    /// Get metric name
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Gauge metric handle
#[derive(Clone, Debug)]
pub struct Gauge {
    inner: GaugeVec,
    name: String,
    label_names: Vec<String>,
}

impl Gauge {
    /// Set gauge value
    pub fn set(&self, value: f64, labels: &HashMap<String, String>) {
        self.with_labels(labels).set(value);
    }

    /// Increment gauge by 1
    pub fn inc(&self, labels: &HashMap<String, String>) {
        self.with_labels(labels).inc();
    }

    /// Decrement gauge by 1
    pub fn dec(&self, labels: &HashMap<String, String>) {
        self.with_labels(labels).dec();
    }

    /// Add value to gauge
    pub fn add(&self, value: f64, labels: &HashMap<String, String>) {
        self.with_labels(labels).add(value);
    }

    /// Subtract value from gauge
    pub fn sub(&self, value: f64, labels: &HashMap<String, String>) {
        self.with_labels(labels).sub(value);
    }

    /// Get gauge with specific labels
    fn with_labels(&self, labels: &HashMap<String, String>) -> PromGauge {
        let label_values: Vec<&str> = self
            .label_names
            .iter()
            .map(|name| labels.get(name).map(|s| s.as_str()).unwrap_or(""))
            .collect();
        self.inner.with_label_values(&label_values)
    }

    /// Get metric name
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Histogram metric handle
#[derive(Clone, Debug)]
pub struct Histogram {
    inner: HistogramVec,
    name: String,
    label_names: Vec<String>,
}

impl Histogram {
    /// Observe value
    pub fn observe(&self, value: f64, labels: &HashMap<String, String>) {
        self.with_labels(labels).observe(value);
    }

    /// Time closure and observe duration in seconds
    pub fn time<F, R>(&self, labels: &HashMap<String, String>, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let result = f();
        let duration = start.elapsed().as_secs_f64();
        self.observe(duration, labels);
        result
    }

    /// Time async closure and observe duration in seconds
    pub async fn time_async<F, Fut, R>(&self, labels: &HashMap<String, String>, f: F) -> R
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = R>,
    {
        let start = Instant::now();
        let result = f().await;
        let duration = start.elapsed().as_secs_f64();
        self.observe(duration, labels);
        result
    }

    /// Get histogram with specific labels
    fn with_labels(&self, labels: &HashMap<String, String>) -> PromHistogram {
        let label_values: Vec<&str> = self
            .label_names
            .iter()
            .map(|name| labels.get(name).map(|s| s.as_str()).unwrap_or(""))
            .collect();
        self.inner.with_label_values(&label_values)
    }

    /// Get metric name
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Handle to running metrics server
#[derive(Debug)]
pub struct MetricsServerHandle {
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl MetricsServerHandle {
    /// Stop metrics server gracefully
    pub async fn stop(self) -> Result<(), ObservabilityError> {
        if let Some(tx) = self.shutdown_tx {
            let _ = tx.send(());
        }
        Ok(())
    }
}

impl Drop for MetricsServerHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Default histogram buckets for common use cases
pub mod buckets {
    /// Default latency buckets (in seconds) for HTTP requests
    pub const LATENCY: &[f64] = &[
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ];

    /// Default buckets for size measurements (in bytes)
    pub const SIZE: &[f64] = &[100.0, 1000.0, 10_000.0, 100_000.0, 1_000_000.0, 10_000_000.0];

    /// Linear buckets generator
    pub fn linear(start: f64, width: f64, count: usize) -> Vec<f64> {
        (0..count).map(|i| start + (i as f64 * width)).collect()
    }

    /// Exponential bucket generator
    pub fn exponential(start: f64, factor: f64, count: usize) -> Vec<f64> {
        (0..count).map(|i| start * factor.powi(i as i32)).collect()
    }
}

/// Predefined ShellWeGo metrics
pub mod builtin {
    use super::*;
    use lazy_static::lazy_static;
    use prometheus::{HistogramOpts, HistogramVec, Opts};

    lazy_static! {
        /// MicroVM spawn duration histogram
        pub static ref MICROVM_SPAWN_DURATION: HistogramVec = HistogramVec::new(
            HistogramOpts::new("shellwego_microvm_spawn_duration_seconds", "Time taken to spawn a microVM")
                .buckets(super::buckets::LATENCY.to_vec()),
            &["node", "runtime"]
        ).expect("Failed to create MICROVM_SPAWN_DURATION metric");

        /// Node memory usage gauge
        pub static ref NODE_MEMORY_USAGE: prometheus::GaugeVec = prometheus::GaugeVec::new(
            Opts::new("shellwego_node_memory_usage_bytes", "Current memory usage of a node"),
            &["node", "type"]
        ).expect("Failed to create NODE_MEMORY_USAGE metric");

        /// Running applications gauge
        pub static ref APPS_RUNNING: prometheus::GaugeVec = prometheus::GaugeVec::new(
            Opts::new("shellwego_apps_running", "Number of currently running applications"),
            &["node", "status"]
        ).expect("Failed to create APPS_RUNNING metric");

        /// Network bytes counter
        pub static ref NETWORK_BYTES_TOTAL: prometheus::CounterVec = prometheus::CounterVec::new(
            Opts::new("shellwego_network_bytes_total", "Total network bytes transferred"),
            &["node", "direction", "interface"]
        ).expect("Failed to create NETWORK_BYTES_TOTAL metric");

        /// Deployment counter
        pub static ref DEPLOYMENT_COUNT: prometheus::CounterVec = prometheus::CounterVec::new(
            Opts::new("shellwego_deployment_count_total", "Total number of deployments"),
            &["node", "status", "runtime"]
        ).expect("Failed to create DEPLOYMENT_COUNT metric");

        /// HTTP request duration histogram
        pub static ref HTTP_REQUEST_DURATION: HistogramVec = HistogramVec::new(
            HistogramOpts::new("shellwego_http_request_duration_seconds", "HTTP request duration")
                .buckets(super::buckets::LATENCY.to_vec()),
            &["method", "path", "status"]
        ).expect("Failed to create HTTP_REQUEST_DURATION metric");

        /// Active connections gauge
        pub static ref ACTIVE_CONNECTIONS: prometheus::GaugeVec = prometheus::GaugeVec::new(
            Opts::new("shellwego_active_connections", "Number of active connections"),
            &["node", "type"]
        ).expect("Failed to create ACTIVE_CONNECTIONS metric");
    }

    /// Register all built-in metrics with a registry
    pub fn register_builtin(registry: &Registry) -> Result<(), ObservabilityError> {
        registry
            .register(Box::new(MICROVM_SPAWN_DURATION.clone()))
            .map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;
        registry
            .register(Box::new(NODE_MEMORY_USAGE.clone()))
            .map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;
        registry
            .register(Box::new(APPS_RUNNING.clone()))
            .map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;
        registry
            .register(Box::new(NETWORK_BYTES_TOTAL.clone()))
            .map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;
        registry
            .register(Box::new(DEPLOYMENT_COUNT.clone()))
            .map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;
        registry
            .register(Box::new(HTTP_REQUEST_DURATION.clone()))
            .map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;
        registry
            .register(Box::new(ACTIVE_CONNECTIONS.clone()))
            .map_err(|e| ObservabilityError::MetricsError(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_operations() {
        let registry = MetricsRegistry::new();
        let counter = registry.register_counter("test_counter", "A test counter", &["label1"]).unwrap();
        
        let mut labels = HashMap::new();
        labels.insert("label1".to_string(), "value1".to_string());
        
        counter.inc(&labels);
        counter.add(5, &labels);
        
        let exported = registry.export_text().unwrap();
        assert!(exported.contains("test_counter"));
    }

    #[test]
    fn test_gauge_operations() {
        let registry = MetricsRegistry::new();
        let gauge = registry.register_gauge("test_gauge", "A test gauge", &["label1"]).unwrap();
        
        let mut labels = HashMap::new();
        labels.insert("label1".to_string(), "value1".to_string());
        
        gauge.set(42.0, &labels);
        gauge.inc(&labels);
        gauge.dec(&labels);
        
        let exported = registry.export_text().unwrap();
        assert!(exported.contains("test_gauge"));
    }

    #[test]
    fn test_histogram_operations() {
        let registry = MetricsRegistry::new();
        let histogram = registry
            .register_histogram("test_histogram", "A test histogram", &["label1"], buckets::LATENCY)
            .unwrap();
        
        let mut labels = HashMap::new();
        labels.insert("label1".to_string(), "value1".to_string());
        
        histogram.observe(0.1, &labels);
        histogram.observe(0.5, &labels);
        
        let result = histogram.time(&labels, || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            42
        });
        assert_eq!(result, 42);
        
        let exported = registry.export_text().unwrap();
        assert!(exported.contains("test_histogram"));
    }

    #[tokio::test]
    async fn test_async_histogram_timing() {
        let registry = MetricsRegistry::new();
        let histogram = registry
            .register_histogram("async_histogram", "An async test histogram", &[], buckets::LATENCY)
            .unwrap();
        
        let labels = HashMap::new();
        let result = histogram.time_async(&labels, || async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            123
        }).await;
        
        assert_eq!(result, 123);
    }
}
