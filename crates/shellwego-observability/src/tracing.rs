//! Distributed tracing with OpenTelemetry
//!
//! This module provides a production-ready distributed tracing implementation
//! that integrates with OpenTelemetry for:
//! - Span creation and management
//! - Context propagation across services
//! - OTLP export to collectors
//! - Integration with the `tracing` crate

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use opentelemetry::propagation::{Extractor, Injector, TextMapPropagator};
use opentelemetry::sdk::propagation::TraceContextPropagator;
use opentelemetry::sdk::resource::{EnvResourceDetector, Resource, TelemetryResourceDetector};
use opentelemetry::sdk::trace::{BatchConfig, BatchSpanProcessor, Config, RandomIdGenerator, Sampler, TracerProvider};
use opentelemetry::trace::{SpanContext as OtelSpanContext, SpanId, Status, TraceContextExt, TraceFlags, TraceId, TraceState, Tracer, TracerProvider as _, SpanKind};
use opentelemetry::{global, KeyValue, Value as OtelValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_semantic_conventions::resource::{SERVICE_NAME, SERVICE_VERSION};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::ObservabilityError;

/// Default OTLP endpoint
const DEFAULT_OTLP_ENDPOINT: &str = "http://localhost:4317";

/// Default batch queue size
const DEFAULT_MAX_QUEUE_SIZE: usize = 2048;

/// Default batch export timeout in milliseconds
const DEFAULT_EXPORT_TIMEOUT_MS: u64 = 30000;

/// Default max export batch size
const DEFAULT_MAX_EXPORT_BATCH_SIZE: usize = 512;

/// Default scheduled delay in milliseconds
const DEFAULT_SCHEDULED_DELAY_MS: u64 = 5000;

/// Tracing configuration
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// OTLP endpoint URL (e.g., "http://otel-collector:4317")
    pub otlp_endpoint: String,
    /// Service name for resource attributes
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Sampling strategy
    pub sampling: SamplingStrategy,
    /// Batch export configuration
    pub batch_config: BatchExportConfig,
    /// Additional resource attributes
    pub resource_attributes: HashMap<String, String>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: DEFAULT_OTLP_ENDPOINT.to_string(),
            service_name: "shellwego".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            sampling: SamplingStrategy::ParentBased(Box::new(SamplingStrategy::AlwaysOn)),
            batch_config: BatchExportConfig::default(),
            resource_attributes: HashMap::new(),
        }
    }
}

impl TracingConfig {
    /// Create config with service name
    pub fn with_service_name(name: &str) -> Self {
        Self {
            service_name: name.to_string(),
            ..Default::default()
        }
    }

    /// Create config with OTLP endpoint
    pub fn with_endpoint(endpoint: &str) -> Self {
        Self {
            otlp_endpoint: endpoint.to_string(),
            ..Default::default()
        }
    }
}

/// Sampling strategy configuration
#[derive(Debug, Clone)]
pub enum SamplingStrategy {
    /// Always sample (100%)
    AlwaysOn,
    /// Never sample (0%)
    AlwaysOff,
    /// Sample a fraction of traces (0.0 to 1.0)
    TraceIdRatioBased(f64),
    /// Respect parent span's sampling decision
    ParentBased(Box<SamplingStrategy>),
}

impl SamplingStrategy {
    /// Convert to OpenTelemetry sampler
    fn to_sampler(&self) -> Sampler {
        match self {
            SamplingStrategy::AlwaysOn => Sampler::AlwaysOn,
            SamplingStrategy::AlwaysOff => Sampler::AlwaysOff,
            SamplingStrategy::TraceIdRatioBased(ratio) => Sampler::TraceIdRatioBased(*ratio),
            SamplingStrategy::ParentBased(inner) => {
                Sampler::ParentBased(Box::new(inner.to_sampler()))
            }
        }
    }
}

/// Batch export configuration
#[derive(Debug, Clone)]
pub struct BatchExportConfig {
    /// Maximum queue size
    pub max_queue_size: usize,
    /// Scheduled delay between exports (ms)
    pub scheduled_delay_ms: u64,
    /// Export timeout (ms)
    pub export_timeout_ms: u64,
    /// Maximum batch size per export
    pub max_export_batch_size: usize,
}

impl Default for BatchExportConfig {
    fn default() -> Self {
        Self {
            max_queue_size: DEFAULT_MAX_QUEUE_SIZE,
            scheduled_delay_ms: DEFAULT_SCHEDULED_DELAY_MS,
            export_timeout_ms: DEFAULT_EXPORT_TIMEOUT_MS,
            max_export_batch_size: DEFAULT_MAX_EXPORT_BATCH_SIZE,
        }
    }
}

impl From<BatchExportConfig> for BatchConfig {
    fn from(config: BatchExportConfig) -> Self {
        BatchConfig::default()
            .with_max_queue_size(config.max_queue_size)
            .with_scheduled_delay(Duration::from_millis(config.scheduled_delay_ms))
            .with_export_timeout(Duration::from_millis(config.export_timeout_ms))
            .with_max_export_batch_size(config.max_export_batch_size)
    }
}

/// Tracing pipeline manager
pub struct TracingPipeline {
    /// Tracer provider
    tracer_provider: TracerProvider,
    /// Tracer instance
    tracer: opentelemetry::sdk::trace::Tracer,
    /// Service name
    service_name: String,
}

impl std::fmt::Debug for TracingPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TracingPipeline")
            .field("service_name", &self.service_name)
            .finish()
    }
}

impl TracingPipeline {
    /// Initialize OpenTelemetry tracing
    pub async fn init(config: &TracingConfig) -> Result<Self, ObservabilityError> {
        // Build resource
        let resource = Resource::from_detectors(
            Duration::from_secs(5),
            vec![
                Box::new(TelemetryResourceDetector),
                Box::new(EnvResourceDetector::new()),
            ],
        )
        .merge(&Resource::new([
            KeyValue::new(SERVICE_NAME, config.service_name.clone()),
            KeyValue::new(SERVICE_VERSION, config.service_version.clone()),
        ]))
        .merge(&Resource::new(
            config
                .resource_attributes
                .iter()
                .map(|(k, v)| KeyValue::new(k.clone(), v.clone()))
                .collect::<Vec<_>>(),
        ));

        // Create OTLP exporter
        let exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(&config.otlp_endpoint)
            .with_timeout(Duration::from_millis(config.batch_config.export_timeout_ms))
            .build_span_exporter()
            .map_err(|e| ObservabilityError::TracingError(format!("Failed to create exporter: {}", e)))?;

        // Create batch processor
        let batch_processor = BatchSpanProcessor::builder(exporter, opentelemetry::sdk::runtime::Tokio)
            .with_batch_config(BatchConfig::from(config.batch_config.clone()))
            .build();

        // Create tracer provider
        let tracer_provider = TracerProvider::builder()
            .with_config(Config::default().with_sampler(config.sampling.to_sampler()))
            .with_resource(resource)
            .with_span_processor(batch_processor)
            .build();

        // Get tracer
        let tracer = tracer_provider.tracer(config.service_name.clone());

        // Set as global tracer provider
        global::set_tracer_provider(tracer_provider.clone());

        Ok(Self {
            tracer_provider,
            tracer,
            service_name: config.service_name.clone(),
        })
    }

    /// Create a new span
    pub fn start_span(&self, name: &str, parent: Option<SpanContext>) -> Span {
        let mut builder = self.tracer.span_builder(name.to_string());

        if let Some(ctx) = parent {
            let span_context = OtelSpanContext::new(
                TraceId::from_hex(&ctx.trace_id).unwrap_or(TraceId::INVALID),
                SpanId::from_hex(&ctx.span_id).unwrap_or(SpanId::INVALID),
                if ctx.sampled {
                    TraceFlags::SAMPLED
                } else {
                    TraceFlags::default()
                },
                false,
                TraceState::default(),
            );
            let parent_cx = opentelemetry::Context::new()
                .with_remote_span_context(span_context);
            builder = builder.with_parent_context(parent_cx);
        }
        
        builder = builder.with_span_kind(SpanKind::Internal);

        let cx = opentelemetry::Context::current();
        let span = self.tracer.build_with_context(builder, cx);
        
        let span_id = span.span_context().span_id().to_string();
        
        Span {
            inner: Some(span),
            span_id,
        }
    }

    /// Create a child span from the current span context
    pub fn start_child_span(&self, name: &str) -> Span {
        let current_context = Self::current_span_context();
        self.start_span(name, current_context)
    }

    /// Get current span context from the active span
    pub fn current_span_context() -> Option<SpanContext> {
        let context = opentelemetry::Context::current();
        let span = context.span();
        let span_context = span.span_context();
        if span_context.is_valid() {
            Some(SpanContext {
                trace_id: format!("{:032x}", span_context.trace_id()),
                span_id: format!("{:016x}", span_context.span_id()),
                sampled: span_context.is_sampled(),
                trace_state: HashMap::new(),
            })
        } else {
            None
        }
    }

    /// Inject span context into carrier (for HTTP headers, etc.)
    pub fn inject_context(context: &SpanContext, carrier: &mut impl Carrier) {
        let span_context = OtelSpanContext::new(
            TraceId::from_hex(&context.trace_id).unwrap_or(TraceId::INVALID),
            SpanId::from_hex(&context.span_id).unwrap_or(SpanId::INVALID),
            if context.sampled {
                TraceFlags::SAMPLED
            } else {
                TraceFlags::default()
            },
            true, // is_remote
            TraceState::default(),
        );

        let cx = opentelemetry::Context::new().with_remote_span_context(span_context);
        let propagator = TraceContextPropagator::new();
        propagator.inject_context(&cx, carrier);
    }

    /// Extract span context from carrier
    pub fn extract_context(carrier: &impl Carrier) -> Option<SpanContext> {
        let propagator = TraceContextPropagator::new();
        let cx = propagator.extract(carrier);
        let span = cx.span();
        let span_context = span.span_context();
        
        if span_context.is_valid() {
            Some(SpanContext {
                trace_id: format!("{:032x}", span_context.trace_id()),
                span_id: format!("{:016x}", span_context.span_id()),
                sampled: span_context.is_sampled(),
                trace_state: HashMap::new(),
            })
        } else {
            None
        }
    }

    /// Force flush all pending spans
    pub async fn force_flush(&self) -> Result<(), ObservabilityError> {
        self.tracer_provider
            .force_flush()
            .map_err(|e| ObservabilityError::TracingError(format!("Force flush failed: {}", e)))?;
        Ok(())
    }

    /// Shutdown tracing pipeline
    pub async fn shutdown(self) -> Result<(), ObservabilityError> {
        self.force_flush().await?;
        self.tracer_provider
            .shutdown()
            .map_err(|e| ObservabilityError::TracingError(format!("Shutdown failed: {}", e)))?;
        Ok(())
    }

    /// Get the tracer instance
    pub fn tracer(&self) -> &opentelemetry::sdk::trace::Tracer {
        &self.tracer
    }
}

/// Span context for propagation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanContext {
    /// Trace ID (32 hex characters)
    pub trace_id: String,
    /// Span ID (16 hex characters)
    pub span_id: String,
    /// Whether the span is sampled
    pub sampled: bool,
    /// Trace state for vendor-specific data
    #[serde(default)]
    pub trace_state: HashMap<String, String>,
}

impl SpanContext {
    /// Create a new span context
    pub fn new(trace_id: &str, span_id: &str, sampled: bool) -> Self {
        Self {
            trace_id: trace_id.to_string(),
            span_id: span_id.to_string(),
            sampled,
            trace_state: HashMap::new(),
        }
    }

    /// Check if the context is valid
    pub fn is_valid(&self) -> bool {
        !self.trace_id.is_empty() && !self.span_id.is_empty()
    }

    /// Convert to W3C traceparent header format
    pub fn to_traceparent(&self) -> String {
        let flags = if self.sampled { "01" } else { "00" };
        format!("00-{}-{}-{}", self.trace_id, self.span_id, flags)
    }

    /// Parse from W3C traceparent header format
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return None;
        }

        // Validate version
        if parts[0] != "00" {
            return None;
        }

        let trace_id = parts[1].to_string();
        let span_id = parts[2].to_string();
        let sampled = parts[3] == "01";

        // Validate lengths
        if trace_id.len() != 32 || span_id.len() != 16 {
            return None;
        }

        Some(Self {
            trace_id,
            span_id,
            sampled,
            trace_state: HashMap::new(),
        })
    }
}

/// Active span handle
pub struct Span {
    inner: Option<opentelemetry::sdk::trace::Span>,
    span_id: String,
}

impl std::fmt::Debug for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Span")
            .field("span_id", &self.span_id)
            .field("is_recording", &self.is_recording())
            .finish()
    }
}

impl Span {
    /// Get the span ID
    pub fn span_id(&self) -> &str {
        &self.span_id
    }

    /// Add attribute to span
    pub fn set_attribute(&self, key: &str, value: AttributeValue) {
        if let Some(ref span) = self.inner {
            span.set_attribute(KeyValue::new(key, value.into()));
        }
    }

    /// Add multiple attributes to span
    pub fn set_attributes(&self, attrs: &HashMap<String, AttributeValue>) {
        for (key, value) in attrs {
            self.set_attribute(key, value.clone());
        }
    }

    /// Add event to span
    pub fn add_event(&self, name: &str, attributes: &HashMap<String, AttributeValue>) {
        if let Some(ref span) = self.inner {
            let attrs: Vec<KeyValue> = attributes
                .iter()
                .map(|(k, v)| KeyValue::new(k.clone(), OtelValue::from(v.clone())))
                .collect();
            span.add_event(name.to_string(), attrs);
        }
    }

    /// Record error on span
    pub fn record_error(&self, error: &dyn std::error::Error) {
        if let Some(ref span) = self.inner {
            span.record_error(error);
            span.set_status(Status::error(error.to_string()));
        }
    }

    /// Set span status
    pub fn set_status(&self, status: SpanStatus) {
        if let Some(ref span) = self.inner {
            let otel_status = match status {
                SpanStatus::Ok => Status::Ok,
                SpanStatus::Error(msg) => Status::error(msg),
                SpanStatus::Unset => Status::Unset,
            };
            span.set_status(otel_status);
        }
    }

    /// Mark span as ok
    pub fn ok(&self) {
        self.set_status(SpanStatus::Ok);
    }

    /// Update span name
    pub fn update_name(&self, name: &str) {
        if let Some(ref span) = self.inner {
            span.update_name(name.to_string());
        }
    }

    /// End span
    pub fn end(self) {
        if let Some(span) = self.inner {
            span.end();
        }
    }

    /// End span with specific status
    pub fn end_with_status(self, status: SpanStatus) {
        self.set_status(status);
        self.end();
    }

    /// Get span context
    pub fn context(&self) -> Option<SpanContext> {
        if let Some(ref span) = self.inner {
            let sc = span.span_context();
            Some(SpanContext {
                trace_id: format!("{:032x}", sc.trace_id()),
                span_id: format!("{:016x}", sc.span_id()),
                sampled: sc.is_sampled(),
                trace_state: HashMap::new(),
            })
        } else {
            None
        }
    }

    /// Check if span is recording
    pub fn is_recording(&self) -> bool {
        self.inner.as_ref().map(|s| s.is_recording()).unwrap_or(false)
    }
}

/// Span status
#[derive(Debug, Clone)]
pub enum SpanStatus {
    /// Operation completed successfully
    Ok,
    /// Operation failed with error message
    Error(String),
    /// Status not set
    Unset,
}

/// Attribute value types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    /// String value
    String(String),
    /// Boolean value
    Bool(bool),
    /// Integer value
    I64(i64),
    /// Float value
    F64(f64),
}

impl From<AttributeValue> for OtelValue {
    fn from(value: AttributeValue) -> Self {
        match value {
            AttributeValue::String(s) => OtelValue::String(s.into()),
            AttributeValue::Bool(b) => OtelValue::Bool(b),
            AttributeValue::I64(i) => OtelValue::I64(i),
            AttributeValue::F64(f) => OtelValue::F64(f),
        }
    }
}

impl From<&str> for AttributeValue {
    fn from(s: &str) -> Self {
        AttributeValue::String(s.to_string())
    }
}

impl From<String> for AttributeValue {
    fn from(s: String) -> Self {
        AttributeValue::String(s)
    }
}

impl From<bool> for AttributeValue {
    fn from(b: bool) -> Self {
        AttributeValue::Bool(b)
    }
}

impl From<i64> for AttributeValue {
    fn from(i: i64) -> Self {
        AttributeValue::I64(i)
    }
}

impl From<f64> for AttributeValue {
    fn from(f: f64) -> Self {
        AttributeValue::F64(f)
    }
}

/// Carrier trait for context propagation
pub trait Carrier: Extractor + Injector {}

/// Implementation for HashMap
impl Carrier for HashMap<String, String> {}

/// HTTP header carrier implementation
#[derive(Debug, Default)]
pub struct HeaderCarrier {
    headers: HashMap<String, String>,
}

impl HeaderCarrier {
    /// Create a new header carrier
    pub fn new() -> Self {
        Self {
            headers: HashMap::new(),
        }
    }

    /// Create from existing headers
    pub fn from_headers(headers: HashMap<String, String>) -> Self {
        Self { headers }
    }

    /// Get all headers
    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    /// Convert to HTTP header map
    pub fn into_headers(self) -> HashMap<String, String> {
        self.headers
    }
}

impl Extractor for HeaderCarrier {
    fn get(&self, key: &str) -> Option<&str> {
        self.headers.get(key).map(|s| s.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.keys().map(|s| s.as_str()).collect()
    }
}

impl Injector for HeaderCarrier {
    fn set(&mut self, key: &str, value: String) {
        self.headers.insert(key.to_string(), value);
    }
}

impl Carrier for HeaderCarrier {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_context() {
        let ctx = SpanContext::new(
            "0af7651916cd43dd8448eb211c80319c",
            "b7ad6b7169203331",
            true,
        );

        assert!(ctx.is_valid());
        assert_eq!(
            ctx.to_traceparent(),
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
        );
    }

    #[test]
    fn test_parse_traceparent() {
        let header = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let ctx = SpanContext::from_traceparent(header).unwrap();

        assert_eq!(ctx.trace_id, "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(ctx.span_id, "b7ad6b7169203331");
        assert!(ctx.sampled);
    }

    #[test]
    fn test_header_carrier() {
        let mut carrier = HeaderCarrier::new();
        carrier.set("traceparent", "00-abc123-def456-01".to_string());

        assert_eq!(carrier.get("traceparent"), Some("00-abc123-def456-01"));
    }

    #[test]
    fn test_attribute_value_conversions() {
        let string_val: AttributeValue = "test".into();
        assert!(matches!(string_val, AttributeValue::String(_)));

        let bool_val: AttributeValue = true.into();
        assert!(matches!(bool_val, AttributeValue::Bool(true)));

        let int_val: AttributeValue = 42i64.into();
        assert!(matches!(int_val, AttributeValue::I64(42)));

        let float_val: AttributeValue = 3.14.into();
        assert!(matches!(float_val, AttributeValue::F64(f) if f > 3.0));
    }
}
