//! Common types for QUIC communication between Control Plane and Agent

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Message types for CP<->Agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// Agent heartbeat with metrics
    Heartbeat {
        node_id: String,
        cpu_usage: f64,
        memory_usage: f64,
        disk_usage: f64,
        network_metrics: NetworkMetrics,
    },
    
    /// Agent reporting its status
    Status {
        node_id: String,
        status: AgentStatus,
        uptime_seconds: u64,
    },
    
    /// Control plane sending command to agent
    Command {
        command_id: String,
        command: CommandType,
        payload: serde_json::Value,
    },
    
    /// Command execution result from agent
    CommandResult {
        command_id: String,
        success: bool,
        output: String,
        error: Option<String>,
    },
    
    /// Agent requesting join
    JoinRequest {
        node_id: String,
        join_token: String,
        capabilities: Vec<String>,
    },
    
    /// Control plane acknowledging join
    JoinResponse {
        accepted: bool,
        node_id: String,
        error: Option<String>,
    },
    
    /// Resource state sync from agent
    ResourceState {
        node_id: String,
        resources: Vec<ResourceInfo>,
    },
    
    /// Log streaming from agent
    Logs {
        node_id: String,
        app_id: String,
        stream: LogStream,
    },
    
    /// Metric stream from agent
    Metrics {
        node_id: String,
        metrics: serde_json::Value,
    },
}

/// Agent status enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentStatus {
    Online,
    Offline,
    Maintenance,
    Updating,
}

/// Command types agents can execute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandType {
    Deploy { app_id: String, image: String },
    Scale { app_id: String, replicas: u32 },
    Restart { app_id: String },
    Stop { app_id: String },
    Backup { app_id: String, volume_id: String },
    Restore { app_id: String, backup_id: String },
    Exec { app_id: String, command: Vec<String> },
}

/// Network metrics reported by agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    pub bytes_sent: u64,
    pub bytes_recv: u64,
    pub packets_sent: u64,
    pub packets_recv: u64,
    pub connections: u32,
}

/// Resource information for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub resource_type: String,
    pub resource_id: String,
    pub name: String,
    pub state: String,
    pub cpu_usage: f64,
    pub memory_usage: f64,
}

/// Log stream types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogStream {
    Stdout,
    Stderr,
    Event,
}

/// QUIC connection configuration
#[derive(Debug, Clone)]
pub struct QuicConfig {
    /// Server address to connect/bind to
    pub addr: SocketAddr,
    
    /// TLS certificate path
    pub cert_path: Option<std::path::PathBuf>,
    
    /// TLS key path
    pub key_path: Option<std::path::PathBuf>,
    
    /// Application protocol identifier
    pub alpn_protocol: Vec<u8>,
    
    /// Maximum number of concurrent streams
    pub max_concurrent_streams: u32,
    
    /// Keep-alive interval in seconds
    pub keep_alive_interval: u64,
    
    /// Connection timeout in seconds
    pub connection_timeout: u64,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([0, 0, 0, 0], 443)),
            cert_path: None,
            key_path: None,
            alpn_protocol: b"shellwego/1".to_vec(),
            max_concurrent_streams: 100,
            keep_alive_interval: 5,
            connection_timeout: 30,
        }
    }
}

/// Stream ID for multiplexing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StreamId(u64);

impl StreamId {
    //TODO: Create stream ID constants for different message channels
}

/// Channel priorities for streams
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChannelPriority {
    Critical = 0,
    Command = 1,
    Metrics = 2,
    Logs = 3,
    BestEffort = 4,
}
