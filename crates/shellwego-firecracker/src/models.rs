use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstanceInfo {
    pub app_id: Option<String>,
    pub id: String,
    pub state: String,
    pub vmm_version: String,
    pub app_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmState {
    NotStarted,
    Starting,
    Running,
    Paused,
    Halted,
    Configured,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BootSource {
    pub kernel_image_path: String,
    pub boot_args: Option<String>,
    pub initrd_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MachineConfig {
    pub vcpu_count: i64,
    pub mem_size_mib: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smt: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_dirty_pages: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Drive {
    pub drive_id: String,
    pub path_on_host: String,
    pub is_root_device: bool,
    pub is_read_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partuuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_type: Option<String>, // "Unsafe", "Writeback"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_engine: Option<String>, // "Sync", "Async"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limiter: Option<RateLimiter>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkInterface {
    pub iface_id: String,
    pub host_dev_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_mac: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx_rate_limiter: Option<RateLimiter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_rate_limiter: Option<RateLimiter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_mmds_requests: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateLimiter {
    pub bandwidth: Option<TokenBucket>,
    pub ops: Option<TokenBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenBucket {
    pub size: i64,
    pub one_time_burst: Option<i64>,
    pub refill_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActionInfo {
    pub action_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotCreateParams {
    pub mem_file_path: String,
    pub snapshot_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotLoadParams {
    pub mem_file_path: String,
    pub snapshot_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_diff_snapshots: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_vm: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Vm {
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metrics {
    pub metrics_path: String,
}

// Actual metrics data structure emitted by Firecracker to the FIFO
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FirecrackerMetrics {
    pub utc_time_ms: u64,
    pub api_server: Option<serde_json::Value>,
    pub vmm: Option<VmmMetrics>,
    pub net: Option<HashMap<String, NetMetrics>>,
    pub block: Option<HashMap<String, BlockMetrics>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmmMetrics {
    #[serde(default)]
    pub rx_bytes: u64,
    #[serde(default)]
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetMetrics {
    pub rx_bytes_count: u64,
    pub tx_bytes_count: u64,
    pub rx_packets_count: u64,
    pub tx_packets_count: u64,
    pub rx_drops_count: u64,
    pub tx_drops_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlockMetrics {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_count: u64,
    pub write_count: u64,
    pub flush_count: u64,
}