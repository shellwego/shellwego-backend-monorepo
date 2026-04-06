//! Access logging for the edge proxy
//!
//! Provides structured access logging in Apache Combined Log Format or JSON format.
//! Logs can be written to a file or stdout (via tracing).

use std::io::Write;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::BufWriter;
use tracing::info;

/// Access log format
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AccessLogFormat {
    /// Apache Combined Log Format
    #[serde(rename = "combined")]
    Combined,
    /// JSON structured logging
    #[serde(rename = "json")]
    Json,
}

impl Default for AccessLogFormat {
    fn default() -> Self {
        AccessLogFormat::Combined
    }
}

/// Access logger
pub struct AccessLogger {
    writer: Arc<tokio::sync::Mutex<Option<BufWriter<File>>>>,
    format: AccessLogFormat,
}

impl AccessLogger {
    /// Create logger that writes to stdout (via tracing)
    pub fn stdout(format: AccessLogFormat) -> Self {
        Self {
            writer: Arc::new(tokio::sync::Mutex::new(None)),
            format,
        }
    }

    /// Create logger that writes to a file
    pub async fn file(path: &str, format: AccessLogFormat) -> Result<Self, std::io::Error> {
        let file = File::create(path).await?;
        let writer = BufWriter::new(file);
        Ok(Self {
            writer: Arc::new(tokio::sync::Mutex::new(Some(writer))),
            format,
        })
    }

    /// Log a single request
    pub async fn log(&self, entry: &AccessLogEntry) {
        let line = match self.format {
            AccessLogFormat::Combined => format_combined(entry),
            AccessLogFormat::Json => format_json(entry),
        };

        let mut guard = self.writer.lock().await;
        if let Some(ref mut writer) = *guard {
            let _ = writeln!(writer, "{}", line);
            let _ = writer.flush();
        } else {
            // Fallback: log via tracing
            info!("{}", line.trim());
        }
    }

    /// Get the current log format
    pub fn format(&self) -> AccessLogFormat {
        self.format
    }
}

/// Single access log entry
#[derive(Debug, Clone)]
pub struct AccessLogEntry {
    pub client_ip: String,
    pub method: String,
    pub path: String,
    pub protocol: String,
    pub status: u16,
    pub response_size: u64,
    pub latency_ms: u64,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub upstream_url: Option<String>,
}

fn format_combined(e: &AccessLogEntry) -> String {
    let ua = e.user_agent.as_deref().unwrap_or("-");
    format!(
        "{} - - [{}] \"{} {} {}\" {} {} \"{}\" \"{}\"",
        e.client_ip,
        chrono::Utc::now().format("%d/%b/%Y:%H:%M:%S +0000"),
        e.method,
        e.path,
        e.protocol,
        e.status,
        e.response_size,
        "-", // referer
        ua,
    )
}

fn format_json(e: &AccessLogEntry) -> String {
    serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "client_ip": e.client_ip,
        "method": e.method,
        "path": e.path,
        "protocol": e.protocol,
        "status": e.status,
        "response_size": e.response_size,
        "latency_ms": e.latency_ms,
        "user_agent": e.user_agent,
        "request_id": e.request_id,
        "upstream_url": e.upstream_url,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_combined() {
        let entry = AccessLogEntry {
            client_ip: "192.168.1.1".to_string(),
            method: "GET".to_string(),
            path: "/api/users".to_string(),
            protocol: "HTTP/1.1".to_string(),
            status: 200,
            response_size: 1234,
            latency_ms: 42,
            user_agent: Some("Mozilla/5.0".to_string()),
            request_id: Some("req-123".to_string()),
            upstream_url: Some("http://backend:8080".to_string()),
        };

        let line = format_combined(&entry);
        assert!(line.contains("192.168.1.1"));
        assert!(line.contains("GET"));
        assert!(line.contains("/api/users"));
        assert!(line.contains("200"));
        assert!(line.contains("1234"));
        assert!(line.contains("Mozilla/5.0"));
    }

    #[test]
    fn test_format_json() {
        let entry = AccessLogEntry {
            client_ip: "10.0.0.1".to_string(),
            method: "POST".to_string(),
            path: "/api/data".to_string(),
            protocol: "HTTP/1.1".to_string(),
            status: 201,
            response_size: 56,
            latency_ms: 10,
            user_agent: None,
            request_id: None,
            upstream_url: None,
        };

        let line = format_json(&entry);
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["client_ip"], "10.0.0.1");
        assert_eq!(parsed["method"], "POST");
        assert_eq!(parsed["path"], "/api/data");
        assert_eq!(parsed["status"], 201);
        assert_eq!(parsed["response_size"], 56);
        assert_eq!(parsed["latency_ms"], 10);
        assert!(parsed["timestamp"].is_string());
    }

    #[test]
    fn test_format_json_optional_fields() {
        let entry = AccessLogEntry {
            client_ip: "127.0.0.1".to_string(),
            method: "GET".to_string(),
            path: "/".to_string(),
            protocol: "HTTP/1.1".to_string(),
            status: 404,
            response_size: 0,
            latency_ms: 1,
            user_agent: None,
            request_id: None,
            upstream_url: None,
        };

        let line = format_json(&entry);
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["user_agent"], serde_json::Value::Null);
        assert_eq!(parsed["request_id"], serde_json::Value::Null);
        assert_eq!(parsed["upstream_url"], serde_json::Value::Null);
    }
}
