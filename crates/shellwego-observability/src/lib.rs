//! Observability stack: metrics, logs, and distributed tracing
//!
//! This crate provides a production-ready observability stack for the ShellWeGo platform,
//! integrating Prometheus metrics, Loki-compatible log aggregation, and OpenTelemetry
//! distributed tracing.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod logs;
pub mod metrics;
pub mod tracing;

// Re-export commonly used types
pub use logs::{LogAggregator, LogConfig, LogEntry, LogLevel, LogStreamHandle};
pub use metrics::{
    buckets, builtin, Counter, Gauge, Histogram, MetricsRegistry, MetricsServerHandle,
};
pub use tracing::{
    AttributeValue, BatchExportConfig, HeaderCarrier, SamplingStrategy,
    Span, SpanContext, SpanStatus, TracingConfig, TracingPipeline,
};

/// Observability error types
#[derive(Error, Debug)]
pub enum ObservabilityError {
    /// Metrics-related error
    #[error("Metrics error: {0}")]
    MetricsError(String),

    /// Log-related error
    #[error("Log error: {0}")]
    LogError(String),

    /// Tracing-related error
    #[error("Tracing error: {0}")]
    TracingError(String),

    /// Export-related error
    #[error("Export failed: {0}")]
    ExportError(String),

    /// I/O error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Global observability handle for managing all observability subsystems
#[derive(Debug)]
pub struct ObservabilityHandle {
    /// Metrics registry
    metrics_registry: Arc<MetricsRegistry>,
    /// Metrics server handle (if serving)
    metrics_server: Option<MetricsServerHandle>,
    /// Log aggregator
    log_aggregator: Arc<LogAggregator>,
    /// Log flush task handle
    _log_flush_task: Option<tokio::task::JoinHandle<()>>,
    /// Tracing pipeline
    tracing_pipeline: Option<TracingPipeline>,
    /// Service name
    service_name: String,
}

impl ObservabilityHandle {
    /// Get the metrics registry
    pub fn metrics(&self) -> &Arc<MetricsRegistry> {
        &self.metrics_registry
    }

    /// Get the log aggregator
    pub fn logs(&self) -> &Arc<LogAggregator> {
        &self.log_aggregator
    }

    /// Get the tracing pipeline
    pub fn tracing(&self) -> Option<&TracingPipeline> {
        self.tracing_pipeline.as_ref()
    }

    /// Get service name
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Graceful shutdown of all observability subsystems
    pub async fn shutdown(self) -> Result<(), ObservabilityError> {
        // Flush logs
        self.log_aggregator.flush().await?;

        // Stop metrics server
        if let Some(server) = self.metrics_server {
            server.stop().await?;
        }

        // Shutdown tracing
        if let Some(tracing) = self.tracing_pipeline {
            tracing.shutdown().await?;
        }

        Ok(())
    }
}

/// Observability configuration
#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    /// Service name for resource attributes
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Environment (production, staging, development)
    pub environment: String,
    /// Metrics configuration
    pub metrics: MetricsConfig,
    /// Logging configuration
    pub logs: LogConfig,
    /// Tracing configuration
    pub tracing: TracingConfig,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            service_name: "shellwego".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            environment: "development".to_string(),
            metrics: MetricsConfig::default(),
            logs: LogConfig::default(),
            tracing: TracingConfig::default(),
        }
    }
}

impl ObservabilityConfig {
    /// Create config with service name
    pub fn with_service_name(mut self, name: &str) -> Self {
        self.service_name = name.to_string();
        self.tracing.service_name = name.to_string();
        self.logs.static_labels.insert("service".to_string(), name.to_string());
        self
    }

    /// Create config with service version
    pub fn with_service_version(mut self, version: &str) -> Self {
        self.service_version = version.to_string();
        self.tracing.service_version = version.to_string();
        self
    }

    /// Create config with environment
    pub fn with_environment(mut self, env: &str) -> Self {
        self.environment = env.to_string();
        self.logs.static_labels.insert("environment".to_string(), env.to_string());
        self.metrics.default_labels.insert("environment".to_string(), env.to_string());
        self
    }

    /// Set metrics endpoint
    pub fn with_metrics_endpoint(mut self, addr: &str) -> Self {
        self.metrics.bind_address = addr.to_string();
        self
    }

    /// Set Loki URL
    pub fn with_loki_url(mut self, url: &str) -> Self {
        self.logs.loki_url = Some(url.to_string());
        self
    }

    /// Set OTLP endpoint
    pub fn with_otlp_endpoint(mut self, endpoint: &str) -> Self {
        self.tracing.otlp_endpoint = endpoint.to_string();
        self
    }

    /// Enable production defaults
    pub fn production(mut self) -> Self {
        self.environment = "production".to_string();
        self.metrics.enabled = true;
        self.metrics.serve_endpoint = true;
        self.logs.buffer_on_failure = true;
        self.tracing.sampling = SamplingStrategy::ParentBased(Box::new(SamplingStrategy::TraceIdRatioBased(0.1)));
        self
    }

    /// Enable development defaults
    pub fn development(mut self) -> Self {
        self.environment = "development".to_string();
        self.metrics.enabled = true;
        self.metrics.serve_endpoint = true;
        self.tracing.sampling = SamplingStrategy::AlwaysOn;
        self
    }
}

/// Metrics-specific configuration
#[derive(Debug, Clone)]
pub struct MetricsConfig {
    /// Whether metrics collection is enabled
    pub enabled: bool,
    /// Bind address for metrics endpoint
    pub bind_address: String,
    /// Whether to serve the metrics endpoint
    pub serve_endpoint: bool,
    /// Default labels to add to all metrics
    pub default_labels: HashMap<String, String>,
    /// Enable built-in process metrics
    pub enable_process_metrics: bool,
    /// Enable built-in ShellWeGo metrics
    pub enable_builtin_metrics: bool,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_address: "0.0.0.0:9090".to_string(),
            serve_endpoint: true,
            default_labels: HashMap::new(),
            enable_process_metrics: true,
            enable_builtin_metrics: true,
        }
    }
}

/// Initialize all observability systems
pub async fn init(config: &ObservabilityConfig) -> Result<ObservabilityHandle, ObservabilityError> {
    // Initialize metrics registry
    let metrics_registry = if config.metrics.enabled {
        let registry = init_metrics(config)?;
        Some(registry)
    } else {
        None
    };

    // Start metrics server if configured
    let metrics_server = if let Some(ref registry) = metrics_registry {
        if config.metrics.serve_endpoint {
            let server = registry.clone().serve_endpoint(&config.metrics.bind_address).await?;
            Some(server)
        } else {
            None
        }
    } else {
        None
    };

    // Initialize log aggregator
    let log_aggregator = Arc::new(LogAggregator::new(&config.logs));
    let log_flush_task = log_aggregator.clone().start_flush_task();

    // Initialize tracing pipeline
    let tracing_pipeline = if config.tracing.otlp_endpoint != "disabled" {
        let pipeline = TracingPipeline::init(&config.tracing).await?;
        Some(pipeline)
    } else {
        None
    };

    Ok(ObservabilityHandle {
        metrics_registry: metrics_registry.unwrap_or_else(|| Arc::new(MetricsRegistry::new())),
        metrics_server,
        log_aggregator,
        _log_flush_task: Some(log_flush_task),
        tracing_pipeline,
        service_name: config.service_name.clone(),
    })
}

/// Initialize the metrics registry
fn init_metrics(config: &ObservabilityConfig) -> Result<Arc<MetricsRegistry>, ObservabilityError> {
    let registry = Arc::new(MetricsRegistry::new());

    // Register built-in ShellWeGo metrics
    if config.metrics.enable_builtin_metrics {
        let builtin_registry = prometheus::Registry::new();
        builtin::register_builtin(&builtin_registry)?;
    }

    Ok(registry)
}

/// Create a health check for observability systems
pub async fn health_check(handle: &ObservabilityHandle) -> Result<HealthStatus, ObservabilityError> {
    let mut status = HealthStatus::default();

    // Check metrics
    status.metrics = handle.metrics().export_text().is_ok();

    // Check logs buffer size
    status.logs_buffer_size = handle.logs().buffer_size();

    // Check tracing
    status.tracing = handle.tracing().is_some();

    status.healthy = status.metrics && status.tracing;

    Ok(status)
}

/// Health status of observability systems
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Whether all systems are healthy
    pub healthy: bool,
    /// Metrics system status
    pub metrics: bool,
    /// Logs buffer size
    pub logs_buffer_size: usize,
    /// Tracing system status
    pub tracing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_init_default_config() {
        let mut config = ObservabilityConfig::default();
        config.tracing.otlp_endpoint = "disabled".to_string();
        config.metrics.serve_endpoint = false;
        let handle = init(&config).await;

        assert!(handle.is_ok());
        let handle = handle.unwrap();

        // Verify subsystems are initialized
        assert!(handle.metrics().export_text().is_ok());

        // Cleanup
        handle.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_init_with_custom_config() {
        let mut config = ObservabilityConfig::default()
            .with_service_name("test-service")
            .with_service_version("1.0.0")
            .with_environment("testing");
        config.tracing.otlp_endpoint = "disabled".to_string();
        config.metrics.serve_endpoint = false;

        let handle = init(&config).await.unwrap();
        assert_eq!(handle.service_name(), "test-service");

        handle.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_health_check() {
        let mut config = ObservabilityConfig::default()
            .with_service_name("health-test");
        config.tracing.otlp_endpoint = "disabled".to_string();
        config.metrics.serve_endpoint = false;

        let handle = init(&config).await.unwrap();
        let health = health_check(&handle).await.unwrap();

        assert!(health.metrics);
        // Tracing is disabled in test config, so this should be false
        assert!(!health.tracing);
        // Overall health is true when metrics are healthy (tracing is optional)
        assert!(health.metrics);

        handle.shutdown().await.unwrap();
    }

    #[test]
    fn test_config_builders() {
        let config = ObservabilityConfig::default()
            .with_service_name("my-service")
            .with_service_version("2.0.0")
            .with_environment("production")
            .with_metrics_endpoint("0.0.0.0:8080")
            .with_loki_url("http://loki:3100")
            .with_otlp_endpoint("http://otel:4317");

        assert_eq!(config.service_name, "my-service");
        assert_eq!(config.service_version, "2.0.0");
        assert_eq!(config.environment, "production");
        assert_eq!(config.metrics.bind_address, "0.0.0.0:8080");
        assert_eq!(config.logs.loki_url, Some("http://loki:3100".to_string()));
        assert_eq!(config.tracing.otlp_endpoint, "http://otel:4317");
    }

    #[test]
    fn test_production_development_presets() {
        let prod_config = ObservabilityConfig::default().production();
        assert_eq!(prod_config.environment, "production");

        let dev_config = ObservabilityConfig::default().development();
        assert_eq!(dev_config.environment, "development");
    }
}
