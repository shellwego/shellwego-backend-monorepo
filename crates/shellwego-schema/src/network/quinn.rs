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
    /// Subscribe to a topic pattern.
    Subscribe {
        /// Client-provided subscription ID (for unsubscribe later).
        subscription_id: SubscriptionId,
        /// Topic pattern (may contain wildcards `*` and `>`).
        topic_pattern: String,
    },
    /// Unsubscribe from a topic.
    Unsubscribe {
        /// The subscription ID to cancel.
        subscription_id: SubscriptionId,
    },
    /// Acknowledgment of a received message (for at-least-once delivery).
    Ack {
        /// The msg_id being acknowledged.
        msg_id: Uuid,
    },
    /// Negative acknowledgment — message could not be processed.
    Nack {
        /// The msg_id that failed.
        msg_id: Uuid,
        /// Reason for failure.
        reason: String,
    },
    /// Publish a message to a topic on the bus.
    Publish {
        /// The bus message envelope.
        bus_message: BusMessageEnvelope,
    },
    /// Ping — internal keepalive.
    Ping {
        /// Timestamp of the ping.
        timestamp: DateTime<Utc>,
    },
    /// Pong — response to a Ping.
    Pong {
        /// Timestamp of the original ping.
        ping_timestamp: DateTime<Utc>,
        /// Timestamp of the pong response.
        pong_timestamp: DateTime<Utc>,
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

// ---------------------------------------------------------------------------
// Message Bus Types (Plan 03)
// ---------------------------------------------------------------------------

/// Error type for topic validation.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum TopicError {
    #[error("topic name is empty")]
    Empty,
    #[error("topic too long: {0} bytes (max {MAX_LEN})", MAX_LEN = 256)]
    TooLong(usize),
    #[error("empty segment in topic: {0}")]
    EmptySegment(String),
    #[error("invalid characters in segment: {0}")]
    InvalidChars(String),
}

/// A validated topic name for the message bus.
///
/// Format: `segment.segment.segment` where each segment matches `[a-zA-Z0-9_-]+`.
/// Maximum length: 256 bytes total.
/// Reserved prefixes: `system.` (internal control messages).
///
/// Wildcards:
/// - `agent.*` — matches any single segment (e.g., `agent.heartbeat` but not `agent.cmd.schedule`).
/// - `node.>` — multi-level wildcard, matches all sub-topics (must be last segment).
///
/// Examples:
/// - `agent.cmd.schedule` — schedule an app on an agent
/// - `agent.heartbeat` — agent heartbeats
/// - `system.ping` — internal keepalive
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Topic(String);

impl Topic {
    pub const MAX_LEN: usize = 256;
    pub const SYSTEM_PREFIX: &str = "system.";

    /// Create a new topic with validation.
    pub fn new(name: impl Into<String>) -> Result<Self, TopicError> {
        let s = name.into();
        if s.is_empty() {
            return Err(TopicError::Empty);
        }
        if s.len() > Self::MAX_LEN {
            return Err(TopicError::TooLong(s.len()));
        }
        for segment in s.split('.') {
            if segment.is_empty() {
                return Err(TopicError::EmptySegment(s));
            }
            if segment == "*" || segment == ">" {
                continue; // wildcards allowed in patterns
            }
            if !segment.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                return Err(TopicError::InvalidChars(segment.to_string()));
            }
        }
        Ok(Self(s))
    }

    /// Access the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true if this topic pattern matches a concrete topic.
    /// - Exact match: `agent.heartbeat` == `agent.heartbeat`
    /// - Single-segment wildcard: `agent.*` matches `agent.heartbeat` but not `agent.cmd.schedule`
    /// - Multi-level wildcard: `agent.>` matches `agent.heartbeat`, `agent.cmd.schedule`, etc.
    /// The `>` wildcard must be the last segment.
    pub fn matches(&self, concrete: &Topic) -> bool {
        let pattern_parts: Vec<&str> = self.0.split('.').collect();
        let concrete_parts: Vec<&str> = concrete.0.split('.').collect();

        for (i, pat) in pattern_parts.iter().enumerate() {
            match *pat {
                ">" => return true, // matches everything remaining (must be last)
                "*" => {
                    if i >= concrete_parts.len() {
                        return false;
                    }
                    // matches exactly one segment
                }
                exact => {
                    if concrete_parts.get(i) != Some(&exact) {
                        return false;
                    }
                }
            }
        }
        pattern_parts.len() == concrete_parts.len()
    }

    /// Returns true if this topic contains wildcard characters.
    pub fn is_wildcard(&self) -> bool {
        self.0.contains('*') || self.0.contains('>')
    }

    /// Returns true if this is a system-reserved topic.
    pub fn is_system(&self) -> bool {
        self.0.starts_with(Self::SYSTEM_PREFIX)
    }
}

impl std::fmt::Display for Topic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for Topic {
    type Err = TopicError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// Unique subscription identifier returned by the bus on subscribe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct SubscriptionId(pub u64);

/// Wire envelope for a bus message (compact form for transport).
///
/// The payload is serialized `Message` bytes to avoid circular type references.
/// Serialized with postcard over QUIC streams. Layout:
///
///   [u32: payload_len] [u8: priority] [u16: topic_len] [topic_bytes] [postcard(Message)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct BusMessageEnvelope {
    /// Unique message identifier (for dedup and ack).
    pub msg_id: Uuid,
    /// Topic this message was published to.
    pub topic: String,
    /// Priority level — affects send ordering.
    pub priority: ChannelPriority,
    /// Timestamp when the message was created (sender wall-clock).
    pub timestamp: DateTime<Utc>,
    /// Node ID of the publisher.
    pub source_node: Option<Uuid>,
    /// If this is a reply, the msg_id of the original message.
    pub reply_to: Option<Uuid>,
    /// Serialized payload bytes (postcard-encoded `Message`).
    pub payload_bytes: Vec<u8>,
}

/// Full bus message with deserialized payload.
///
/// This is the in-memory representation used by the router and subscribers.
/// The wire format uses `BusMessageEnvelope` (with `payload_bytes: Vec<u8>`).
#[derive(Debug, Clone)]
pub struct BusMessage {
    /// Unique message identifier (for dedup and ack).
    pub msg_id: Uuid,
    /// Topic this message was published to.
    pub topic: Topic,
    /// Priority level — affects send ordering.
    pub priority: ChannelPriority,
    /// Timestamp when the message was created (sender wall-clock).
    pub timestamp: DateTime<Utc>,
    /// Node ID of the publisher.
    pub source_node: Option<Uuid>,
    /// If this is a reply, the msg_id of the original message.
    pub reply_to: Option<Uuid>,
    /// The application-level payload.
    pub payload: Message,
}

impl BusMessage {
    /// Create a new BusMessage.
    pub fn new(topic: Topic, payload: Message, priority: ChannelPriority) -> Self {
        Self {
            msg_id: Uuid::new_v4(),
            topic,
            priority,
            timestamp: Utc::now(),
            source_node: None,
            reply_to: None,
            payload,
        }
    }

    /// Create a BusMessage with an explicit source node.
    pub fn with_source(mut self, node_id: Uuid) -> Self {
        self.source_node = Some(node_id);
        self
    }

    /// Create a BusMessage that is a reply to another message.
    pub fn with_reply_to(mut self, original_msg_id: Uuid) -> Self {
        self.reply_to = Some(original_msg_id);
        self
    }

    /// Convert to wire envelope (serialize the payload).
    pub fn to_envelope(&self) -> anyhow::Result<BusMessageEnvelope> {
        Ok(BusMessageEnvelope {
            msg_id: self.msg_id,
            topic: self.topic.as_str().to_string(),
            priority: self.priority,
            timestamp: self.timestamp,
            source_node: self.source_node,
            reply_to: self.reply_to,
            payload_bytes: postcard::to_allocvec(&self.payload)?,
        })
    }

    /// Convert from wire envelope (deserialize the payload).
    pub fn from_envelope(envelope: BusMessageEnvelope) -> anyhow::Result<Self> {
        let payload: Message = postcard::from_bytes(&envelope.payload_bytes)?;
        Ok(Self {
            msg_id: envelope.msg_id,
            topic: Topic::new(envelope.topic)?,
            priority: envelope.priority,
            timestamp: envelope.timestamp,
            source_node: envelope.source_node,
            reply_to: envelope.reply_to,
            payload,
        })
    }
}

/// Configuration for the QUIC message bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct BusConfig {
    /// Maximum number of in-flight unacknowledged messages per connection.
    /// Default: 1024.
    pub max_inflight: u32,

    /// Acknowledgment timeout in milliseconds. If no `Ack` is received
    /// within this duration, the message is retried.
    /// Default: 5000.
    pub ack_timeout_ms: u64,

    /// Maximum retry attempts for a message before it goes to the dead-letter queue.
    /// Default: 3.
    pub max_retries: u32,

    /// Base retry delay in milliseconds (exponential backoff: delay * 2^attempt).
    /// Default: 100.
    pub retry_base_delay_ms: u64,

    /// Per-subscriber inbox capacity (bounded mpsc channel size).
    /// Default: 8192.
    pub subscriber_buffer_size: usize,

    /// Maximum number of subscribers per topic.
    /// Default: 1000.
    pub max_subscribers_per_topic: usize,

    /// Whether to enable the dead-letter queue for failed messages.
    /// Default: true.
    pub dead_letter_enabled: bool,
}

impl Default for BusConfig {
    fn default() -> Self {
        Self {
            max_inflight: 1024,
            ack_timeout_ms: 5000,
            max_retries: 3,
            retry_base_delay_ms: 100,
            subscriber_buffer_size: 8192,
            max_subscribers_per_topic: 1000,
            dead_letter_enabled: true,
        }
    }
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
