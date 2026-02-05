pub mod daemon;
pub mod discovery;
pub mod metrics;
pub mod migration;
pub mod reconciler;
pub mod snapshot;
pub mod vmm;
pub mod wasm;

use secrecy::SecretString;
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub node_id: Option<Uuid>,
    pub control_plane_url: String,
    pub join_token: Option<SecretString>,
    pub region: String,
    pub zone: String,
    pub labels: HashMap<String, String>,
    pub firecracker_binary: PathBuf,
    pub kernel_image_path: PathBuf,
    pub data_dir: PathBuf,
    pub max_microvms: u32,
    pub reserved_memory_mb: u64,
    pub reserved_cpu_percent: f64,
}

impl AgentConfig {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self {
            node_id: None,
            control_plane_url: std::env::var("SHELLWEGO_CP_URL")
                .unwrap_or_else(|_| "127.0.0.1:4433".to_string()),
            join_token: std::env::var("SHELLWEGO_JOIN_TOKEN").ok().map(SecretString::from),
            region: std::env::var("SHELLWEGO_REGION").unwrap_or_else(|_| "unknown".to_string()),
            zone: std::env::var("SHELLWEGO_ZONE").unwrap_or_else(|_| "unknown".to_string()),
            labels: HashMap::new(),
            firecracker_binary: "/usr/local/bin/firecracker".into(),
            kernel_image_path: "/var/lib/shellwego/vmlinux".into(),
            data_dir: "/var/lib/shellwego".into(),
            max_microvms: 500,
            reserved_memory_mb: 512,
            reserved_cpu_percent: 10.0,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Capabilities {
    pub kvm: bool,
    pub nested_virtualization: bool,
    pub cpu_cores: u32,
    pub memory_total_mb: u64,
    pub cpu_features: Vec<String>,
}

pub fn detect_capabilities() -> anyhow::Result<Capabilities> {
    let kvm = std::fs::metadata("/dev/kvm").is_ok();
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    let cpu_cores = sys.cpus().len() as u32;
    let memory_total_mb = sys.total_memory();
    Ok(Capabilities {
        kvm,
        nested_virtualization: false,
        cpu_cores,
        memory_total_mb,
        cpu_features: vec![],
    })
}