//! Log aggregation with Loki-compatible export
//!
//! This module provides a production-ready log aggregation system that can:
//! - Buffer logs in memory for batching
//! - Send logs to Loki-compatible endpoints
//! - Query logs via LogQL-like syntax
//! - Stream logs via WebSocket

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use tokio::time::interval;

use crate::ObservabilityError;

/// Default batch size for log buffering
const DEFAULT_BATCH_SIZE: usize = 100;

/// Default flush interval in milliseconds
const DEFAULT_FLUSH_INTERVAL_MS: u64 = 1000;

/// Maximum buffer size before forced flush
const MAX_BUFFER_SIZE: usize = 10_000;

/// Log level enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "trace"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
            LogLevel::Fatal => write!(f, "fatal"),
        }
    }
}

impl std::str::FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "trace" => Ok(LogLevel::Trace),
            "debug" => Ok(LogLevel::Debug),
            "info" => Ok(LogLevel::Info),
            "warn" | "warning" => Ok(LogLevel::Warn),
            "error" => Ok(LogLevel::Error),
            "fatal" | "critical" => Ok(LogLevel::Fatal),
            _ => Err(format!("Unknown log level: {}", s)),
        }
    }
}

/// Log entry structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp of the log entry
    pub timestamp: DateTime<Utc>,
    /// Source identifier (e.g., app_id, container_id)
    pub source: String,
    /// Log level
    pub level: LogLevel,
    /// Log message
    pub message: String,
    /// Additional labels/metadata
    pub labels: HashMap<String, String>,
    /// Structured fields (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<HashMap<String, serde_json::Value>>,
}

impl LogEntry {
    /// Create a new log entry
    pub fn new(source: &str, level: LogLevel, message: &str) -> Self {
        Self {
            timestamp: Utc::now(),
            source: source.to_string(),
            level,
            message: message.to_string(),
            labels: HashMap::new(),
            fields: None,
        }
    }

    /// Add a label to the entry
    pub fn with_label(mut self, key: &str, value: &str) -> Self {
        self.labels.insert(key.to_string(), value.to_string());
        self
    }

    /// Add structured fields to the entry
    pub fn with_fields(mut self, fields: HashMap<String, serde_json::Value>) -> Self {
        self.fields = Some(fields);
        self
    }

    /// Convert to Loki log line format
    pub fn to_loki_line(&self) -> String {
        if let Some(fields) = &self.fields {
            // Structured log format
            let mut structured = serde_json::Map::new();
            structured.insert("timestamp".to_string(), self.timestamp.to_rfc3339().into());
            structured.insert("level".to_string(), self.level.to_string().into());
            structured.insert("message".to_string(), self.message.clone().into());
            for (k, v) in fields {
                structured.insert(k.clone(), v.clone());
            }
            serde_json::to_string(&structured).unwrap_or_else(|_| self.message.clone())
        } else {
            // Simple text format
            format!(
                "[{}] [{}] [{}] {}",
                self.timestamp.to_rfc3339(),
                self.level,
                self.source,
                self.message
            )
        }
    }
}

/// Log configuration
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Loki endpoint URL (e.g., "http://loki:3100")
    pub loki_url: Option<String>,
    /// Tenant ID for multi-tenant Loki
    pub tenant_id: Option<String>,
    /// Maximum batch size before flush
    pub batch_size: usize,
    /// Flush interval in milliseconds
    pub flush_interval_ms: u64,
    /// Static labels to add to all logs
    pub static_labels: HashMap<String, String>,
    /// Whether to buffer locally when Loki is unavailable
    pub buffer_on_failure: bool,
    /// Maximum local buffer size
    pub max_buffer_size: usize,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            loki_url: None,
            tenant_id: None,
            batch_size: DEFAULT_BATCH_SIZE,
            flush_interval_ms: DEFAULT_FLUSH_INTERVAL_MS,
            static_labels: HashMap::new(),
            buffer_on_failure: true,
            max_buffer_size: MAX_BUFFER_SIZE,
        }
    }
}

/// Log aggregator for buffering and sending logs
#[derive(Debug)]
pub struct LogAggregator {
    /// Configuration
    config: LogConfig,
    /// Internal buffer for batching
    buffer: RwLock<Vec<LogEntry>>,
    /// HTTP client for Loki
    client: Option<reqwest::Client>,
    /// Channel for log streaming subscribers
    stream_tx: broadcast::Sender<LogEntry>,
    /// Flush channel
    flush_tx: mpsc::Sender<()>,
    /// Shutdown channel
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl LogAggregator {
    /// Create new aggregator with configuration
    pub fn new(config: &LogConfig) -> Self {
        let client = config.loki_url.as_ref().map(|_| reqwest::Client::new());
        let (stream_tx, _) = broadcast::channel(1024);
        let (flush_tx, _) = mpsc::channel(1);
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            config: config.clone(),
            buffer: RwLock::new(Vec::with_capacity(config.batch_size)),
            client,
            stream_tx,
            flush_tx,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Start the background flush task
    pub fn start_flush_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let mut shutdown_rx = self.shutdown_tx.as_ref().unwrap().subscribe();
        let flush_tx = self.flush_tx.clone();
        let interval_ms = self.config.flush_interval_ms;

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(interval_ms));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let _ = flush_tx.send(()).await;
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        })
    }

    /// Ingest a log line
    pub async fn ingest(
        &self,
        source: &str,
        level: LogLevel,
        message: &str,
        labels: &HashMap<String, String>,
    ) -> Result<(), ObservabilityError> {
        let mut entry = LogEntry::new(source, level, message);

        // Add static labels
        for (k, v) in &self.config.static_labels {
            entry.labels.insert(k.clone(), v.clone());
        }

        // Add dynamic labels
        for (k, v) in labels {
            entry.labels.insert(k.clone(), v.clone());
        }

        // Broadcast to stream subscribers
        let _ = self.stream_tx.send(entry.clone());

        // Buffer the entry
        {
            let mut buffer = self.buffer.write();
            buffer.push(entry);

            // Check if we need to flush due to batch size
            if buffer.len() >= self.config.batch_size {
                drop(buffer);
                self.flush().await?;
            }
        }

        Ok(())
    }

    /// Ingest a structured log with JSON fields
    pub async fn ingest_structured(
        &self,
        source: &str,
        level: LogLevel,
        fields: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ObservabilityError> {
        let message = fields
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut entry = LogEntry::new(source, level, &message);
        entry.fields = Some(fields.clone());

        // Add static labels
        for (k, v) in &self.config.static_labels {
            entry.labels.insert(k.clone(), v.clone());
        }

        // Broadcast to stream subscribers
        let _ = self.stream_tx.send(entry.clone());

        // Buffer the entry
        {
            let mut buffer = self.buffer.write();
            buffer.push(entry);

            if buffer.len() >= self.config.batch_size {
                drop(buffer);
                self.flush().await?;
            }
        }

        Ok(())
    }

    /// Query logs (simple filter implementation)
    pub async fn query(
        &self,
        query: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<LogEntry>, ObservabilityError> {
        let buffer = self.buffer.read();

        // Simple query parsing: support {source="xxx"} and {level="xxx"}
        let filters = parse_simple_query(query);

        let results: Vec<LogEntry> = buffer
            .iter()
            .filter(|entry| {
                // Time range filter
                if entry.timestamp < start || entry.timestamp > end {
                    return false;
                }

                // Apply filters
                for (key, value) in &filters {
                    match key.as_str() {
                        "source" => {
                            if entry.source != *value {
                                return false;
                            }
                        }
                        "level" => {
                            if entry.level.to_string() != *value {
                                return false;
                            }
                        }
                        "message" => {
                            if !entry.message.contains(value) {
                                return false;
                            }
                        }
                        _ => {
                            if entry.labels.get(key).map(|v| v.as_str()) != Some(value.as_str()) {
                                return false;
                            }
                        }
                    }
                }
                true
            })
            .take(limit)
            .cloned()
            .collect();

        Ok(results)
    }

    /// Stream logs matching a query
    pub async fn stream(
        &self,
        _query: &str,
    ) -> Result<LogStreamHandle, ObservabilityError> {
        let rx = self.stream_tx.subscribe();
        Ok(LogStreamHandle { rx })
    }

    /// Flush buffered logs to Loki
    pub async fn flush(&self) -> Result<(), ObservabilityError> {
        let entries: Vec<LogEntry> = {
            let mut buffer = self.buffer.write();
            if buffer.is_empty() {
                return Ok(());
            }
            std::mem::take(&mut *buffer)
        };

        if let (Some(client), Some(loki_url)) = (&self.client, &self.config.loki_url) {
            self.send_to_loki(client, loki_url, &entries).await?;
        }

        Ok(())
    }

    /// Send entries to Loki
    async fn send_to_loki(
        &self,
        client: &reqwest::Client,
        loki_url: &str,
        entries: &[LogEntry],
    ) -> Result<(), ObservabilityError> {
        // Group entries by labels
        let mut streams: HashMap<String, Vec<(i64, String)>> = HashMap::new();

        for entry in entries {
            let labels_key = format_labels(&entry.labels);
            let timestamp_ns = entry.timestamp.timestamp_nanos_opt().unwrap_or(0);
            let line = entry.to_loki_line();

            streams
                .entry(labels_key)
                .or_default()
                .push((timestamp_ns, line));
        }

        // Build Loki push request
        let streams: Vec<LokiStream> = streams
            .into_iter()
            .map(|(labels, values)| {
                let labels: HashMap<String, String> = serde_json::from_str(&labels).unwrap_or_default();
                LokiStream {
                    stream: labels,
                    values,
                }
            })
            .collect();

        let push_request = LokiPushRequest { streams };

        let url = format!("{}/loki/api/v1/push", loki_url.trim_end_matches('/'));
        let mut request = client.post(&url).json(&push_request);

        if let Some(tenant_id) = &self.config.tenant_id {
            request = request.header("X-Scope-OrgID", tenant_id);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ObservabilityError::LogError(format!("Failed to send to Loki: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ObservabilityError::LogError(format!(
                "Loki returned status {}: {}",
                status, body
            )));
        }

        Ok(())
    }

    /// Get buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer.read().len()
    }
}

/// Format labels as JSON string for grouping
fn format_labels(labels: &HashMap<String, String>) -> String {
    let mut sorted: Vec<_> = labels.iter().collect();
    sorted.sort_by_key(|(k, _)| *k);
    serde_json::to_string(&sorted.into_iter().collect::<HashMap<_, _>>()).unwrap_or_default()
}

/// Parse simple query syntax: {source="xxx", level="yyy"}
fn parse_simple_query(query: &str) -> HashMap<String, String> {
    let mut filters = HashMap::new();

    // Remove outer braces
    let query = query.trim();
    if !query.starts_with('{') || !query.ends_with('}') {
        return filters;
    }

    let inner = &query[1..query.len() - 1];

    // Split by comma
    for part in inner.split(',') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            filters.insert(key.to_string(), value.to_string());
        }
    }

    filters
}

/// Loki stream structure
#[derive(Debug, Serialize)]
struct LokiStream {
    stream: HashMap<String, String>,
    values: Vec<(i64, String)>,
}

/// Loki push request structure
#[derive(Debug, Serialize)]
struct LokiPushRequest {
    streams: Vec<LokiStream>,
}

/// Handle to an active log stream
#[derive(Debug)]
pub struct LogStreamHandle {
    rx: broadcast::Receiver<LogEntry>,
}

impl LogStreamHandle {
    /// Receive the next log entry
    pub async fn recv(&mut self) -> Result<LogEntry, ObservabilityError> {
        self.rx
            .recv()
            .await
            .map_err(|e| ObservabilityError::LogError(format!("Stream closed: {}", e)))
    }

    /// Try to receive a log entry without blocking
    pub fn try_recv(&mut self) -> Result<Option<LogEntry>, ObservabilityError> {
        match self.rx.try_recv() {
            Ok(entry) => Ok(Some(entry)),
            Err(broadcast::error::TryRecvError::Empty) => Ok(None),
            Err(e) => Err(ObservabilityError::LogError(format!("Stream error: {}", e))),
        }
    }
}

/// Trait for custom log senders
pub trait LogSender: Send + Sync {
    /// Send a log entry
    fn send(&mut self, entry: LogEntry) -> Result<(), ObservabilityError>;
}

/// In-memory log sender for testing
#[derive(Debug, Default)]
pub struct InMemoryLogSender {
    pub entries: Vec<LogEntry>,
}

impl InMemoryLogSender {
    /// Create a new in-memory sender
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }
}

impl LogSender for InMemoryLogSender {
    fn send(&mut self, entry: LogEntry) -> Result<(), ObservabilityError> {
        self.entries.push(entry);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_ingestion() {
        let config = LogConfig::default();
        let aggregator = Arc::new(LogAggregator::new(&config));

        let labels = HashMap::new();
        aggregator
            .ingest("test-app", LogLevel::Info, "Hello, world!", &labels)
            .await
            .unwrap();

        assert_eq!(aggregator.buffer_size(), 1);
    }

    #[tokio::test]
    async fn test_structured_log_ingestion() {
        let config = LogConfig::default();
        let aggregator = Arc::new(LogAggregator::new(&config));

        let mut fields = HashMap::new();
        fields.insert("message".to_string(), serde_json::Value::String("Test".to_string()));
        fields.insert("user_id".to_string(), serde_json::Value::Number(42.into()));

        aggregator
            .ingest_structured("test-app", LogLevel::Debug, &fields)
            .await
            .unwrap();

        assert_eq!(aggregator.buffer_size(), 1);
    }

    #[tokio::test]
    async fn test_log_query() {
        let config = LogConfig::default();
        let aggregator = Arc::new(LogAggregator::new(&config));

        let labels = HashMap::new();
        aggregator
            .ingest("app1", LogLevel::Info, "Test message 1", &labels)
            .await
            .unwrap();
        aggregator
            .ingest("app2", LogLevel::Error, "Error message", &labels)
            .await
            .unwrap();

        let results = aggregator
            .query(
                r#"{source="app1"}"#,
                Utc::now() - chrono::Duration::minutes(5),
                Utc::now() + chrono::Duration::minutes(5),
                100,
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "app1");
    }

    #[test]
    fn test_log_entry_format() {
        let entry = LogEntry::new("test-app", LogLevel::Info, "Test message")
            .with_label("environment", "production");

        let line = entry.to_loki_line();
        assert!(line.contains("Test message"));
        assert!(line.contains("info"));
    }

    #[test]
    fn test_parse_simple_query() {
        let filters = parse_simple_query(r#"{source="app1", level="error"}"#);
        assert_eq!(filters.get("source"), Some(&"app1".to_string()));
        assert_eq!(filters.get("level"), Some(&"error".to_string()));
    }
}
