use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstanceInfo {
    pub app_id: Option<String>,
    pub id: String,
    pub state: String,
    pub vmm_version: String,
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
    pub smt: Option<bool>,
    pub track_dirty_pages: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Drive {
    pub drive_id: String,
    pub path_on_host: String,
    pub is_root_device: bool,
    pub is_read_only: bool,
    pub rate_limiter: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkInterface {
    pub iface_id: String,
    pub host_dev_name: String,
    pub guest_mac: Option<String>,
    pub rx_rate_limiter: Option<serde_json::Value>,
    pub tx_rate_limiter: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActionInfo {
    pub action_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotCreateParams {
    pub mem_file_path: String,
    pub snapshot_path: String,
    pub snapshot_type: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotLoadParams {
    pub mem_file_path: String,
    pub snapshot_path: String,
    pub enable_diff_snapshots: Option<bool>,
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
