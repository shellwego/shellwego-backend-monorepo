use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Register {
        hostname: String,
        capabilities: Vec<String>,
    },
    Heartbeat {
        node_id: Uuid,
        cpu_usage: f64,
        memory_usage: f64,
    },
    EventLog {
        app_id: Uuid,
        level: String,
        msg: String,
    },
    ScheduleApp {
        deployment_id: Uuid,
        app_id: Uuid,
        image: String,
        limits: ResourceLimits,
    },
    TerminateApp {
        app_id: Uuid,
    },
    ActionResponse {
        request_id: Uuid,
        success: bool,
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpu_milli: u32,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct QuicConfig {
    pub addr: String,
    pub cert_path: Option<std::path::PathBuf>,
    pub key_path: Option<std::path::PathBuf>,
    pub alpn_protocol: Vec<u8>,
    pub max_concurrent_streams: u32,
    pub keep_alive_interval: u64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChannelPriority {
    Critical = 0,
    Command = 1,
    Metrics = 2,
    Logs = 3,
    BestEffort = 4,
}
