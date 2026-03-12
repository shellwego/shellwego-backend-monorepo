//! QUIC communication types
//!
//! Types for QUIC-based communication between control plane and agents.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// QUIC message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum Message {
    /// Node registration
    Register {
        /// Hostname of the agent node
        hostname: String,
        /// List of capabilities
        capabilities: Vec<String>,
    },
    /// Heartbeat from agent
    Heartbeat {
        /// Node ID
        node_id: Uuid,
        /// CPU usage (0.0 - 1.0)
        cpu_usage: f64,
        /// Memory usage (0.0 - 1.0)
        memory_usage: f64,
    },
    /// Event log from agent
    EventLog {
        /// Application ID
        app_id: Uuid,
        /// Log level
        level: String,
        /// Log message
        msg: String,
    },
    /// Schedule app on agent
    ScheduleApp {
        /// Deployment ID
        deployment_id: Uuid,
        /// Application ID
        app_id: Uuid,
        /// Container image
        image: String,
        /// Resource limits
        limits: ResourceLimits,
    },
    /// Terminate app on agent
    TerminateApp {
        /// Application ID
        app_id: Uuid,
    },
    /// Response to a request
    ActionResponse {
        /// Request ID being responded to
        request_id: Uuid,
        /// Whether the action succeeded
        success: bool,
        /// Error message if failed
        error: Option<String>,
    },
}

/// Resource limits for scheduled apps
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct ResourceLimits {
    /// CPU in millicores
    pub cpu_milli: u32,
    /// Memory in bytes
    pub memory_bytes: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_milli: 1000, // 1 vCPU
            memory_bytes: 128 * 1024 * 1024, // 128 MB
        }
    }
}

/// QUIC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct QuicConfig {
    /// Listen address (host:port)
    pub addr: String,
    /// Path to TLS certificate
    pub cert_path: Option<PathBuf>,
    /// Path to TLS private key
    pub key_path: Option<PathBuf>,
    /// ALPN protocol identifier
    pub alpn_protocol: Vec<u8>,
    /// Maximum concurrent streams
    pub max_concurrent_streams: u32,
    /// Keep-alive interval in seconds
    pub keep_alive_interval: u64,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            addr: "0.0.0.0:4433".to_string(),
            cert_path: None,
            key_path: None,
            alpn_protocol: b"shellwego/1".to_vec(),
            max_concurrent_streams: 100,
            keep_alive_interval: 5,
            connection_timeout: 30,
        }
    }
}

impl QuicConfig {
    /// Create a new QUIC config with the given address
    pub fn new(addr: &str) -> Self {
        Self {
            addr: addr.to_string(),
            ..Default::default()
        }
    }

    /// Set TLS certificate path
    pub fn with_cert(mut self, path: PathBuf) -> Self {
        self.cert_path = Some(path);
        self
    }

    /// Set TLS private key path
    pub fn with_key(mut self, path: PathBuf) -> Self {
        self.key_path = Some(path);
        self
    }
}

/// Agent connection information.
///
/// Represents a connection to an agent node from the control plane.
/// This is the unified type that consolidates connection state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct AgentConnection {
    /// Node ID (assigned after registration)
    pub node_id: Uuid,
    /// Remote address of the agent
    pub remote_addr: String,
    /// Hostname of the agent node
    pub hostname: String,
    /// Region where the agent is located
    pub region: String,
    /// Connection established timestamp
    pub connected_at: DateTime<Utc>,
    /// Last heartbeat timestamp
    pub last_heartbeat: DateTime<Utc>,
}

impl AgentConnection {
    /// Create a new agent connection
    pub fn new(node_id: Uuid, hostname: String, region: String) -> Self {
        let now = Utc::now();
        Self {
            node_id,
            remote_addr: String::new(),
            hostname,
            region,
            connected_at: now,
            last_heartbeat: now,
        }
    }

    /// Create with remote address
    pub fn with_remote_addr(mut self, addr: String) -> Self {
        self.remote_addr = addr;
        self
    }
}

/// Channel priority for multiplexed streams
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum ChannelPriority {
    /// Critical system messages
    Critical = 0,
    /// Command messages
    #[default]
    Command = 1,
    /// Metrics data
    Metrics = 2,
    /// Log data
    Logs = 3,
    /// Best effort (lowest priority)
    BestEffort = 4,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.cpu_milli, 1000);
        assert_eq!(limits.memory_bytes, 128 * 1024 * 1024);
    }

    #[test]
    fn test_quic_config_default() {
        let config = QuicConfig::default();
        assert_eq!(config.addr, "0.0.0.0:4433");
        assert_eq!(config.alpn_protocol, b"shellwego/1");
        assert_eq!(config.max_concurrent_streams, 100);
    }

    #[test]
    fn test_quic_config_builder() {
        let config = QuicConfig::new("127.0.0.1:8443")
            .with_cert(PathBuf::from("/cert.pem"))
            .with_key(PathBuf::from("/key.pem"));

        assert_eq!(config.addr, "127.0.0.1:8443");
        assert_eq!(config.cert_path, Some(PathBuf::from("/cert.pem")));
        assert_eq!(config.key_path, Some(PathBuf::from("/key.pem")));
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::Heartbeat {
            node_id: Uuid::nil(),
            cpu_usage: 0.5,
            memory_usage: 0.3,
        };

        let json = serde_json::to_string(&msg).unwrap();
        let decoded: Message = serde_json::from_str(&json).unwrap();

        match decoded {
            Message::Heartbeat { node_id, cpu_usage, memory_usage } => {
                assert_eq!(node_id, Uuid::nil());
                assert!((cpu_usage - 0.5).abs() < 0.001);
                assert!((memory_usage - 0.3).abs() < 0.001);
            }
            _ => panic!("Expected Heartbeat message"),
        }
    }

    #[test]
    fn test_channel_priority_ordering() {
        assert!(ChannelPriority::Critical < ChannelPriority::Command);
        assert!(ChannelPriority::Command < ChannelPriority::Metrics);
        assert!(ChannelPriority::Metrics < ChannelPriority::Logs);
        assert!(ChannelPriority::Logs < ChannelPriority::BestEffort);
    }
}
