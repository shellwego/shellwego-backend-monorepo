//! ShellWeGo Agent
//! 
//! Runs on every worker node. Responsible for:
//! - Maintaining heartbeat with control plane
//! - Spawning/managing Firecracker microVMs
//! - Enforcing desired state (reconciliation loop)
//! - Reporting resource usage and health

use std::sync::Arc;
use tokio::signal;
use tracing::{info, error};
use secrecy::SecretString;
use clap::Parser;

mod daemon;
mod reconciler;
mod vmm;
mod wasm;        // WASM runtime support
mod snapshot;    // VM snapshot management
mod migration;   // Live migration support

use daemon::Daemon;
use vmm::VmmManager;
use shellwego_network::CniNetwork;
use shellwego_storage::zfs::ZfsManager;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "/etc/shellwego/agent.toml")]
    config: std::path::PathBuf,

    /// Log level (info, debug, trace, etc.)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Output logs in JSON format for production
    #[arg(long)]
    json_logs: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    if args.json_logs {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(&args.log_level)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(&args.log_level)
            .init();
    }
    
    info!("ShellWeGo Agent starting...");
    
    // Load configuration
    let mut config = AgentConfig::load()?;
    // Override config with CLI args or env if needed

    info!("Node ID: {:?}", config.node_id);
    info!("Control plane: {}", config.control_plane_url);
    
    // Detect capabilities (KVM, CPU features, etc)
    let capabilities = detect_capabilities()?;
    info!("Capabilities: KVM={}, CPUs={}", capabilities.kvm, capabilities.cpu_cores);
    
    // Initialize VMM manager (Firecracker)
    let vmm = VmmManager::new(&config).await?;
    
    // Initialize Storage (ZFS)
    let storage = Arc::new(ZfsManager::new("tank").await?);

    // Initialize Networking (CNI)
    let network = Arc::new(CniNetwork::new("sw0", "10.0.0.0/16").await?);

    // Initialize daemon (control plane communication)
    let daemon = Daemon::new(config.clone(), capabilities, vmm.clone()).await?;
    
    // Start reconciler (desired state enforcement)
    let reconciler = reconciler::Reconciler::new(vmm.clone(), network, storage, daemon.state_client());
    
    // Spawn concurrent tasks
    let heartbeat_handle = tokio::spawn({
        let daemon = daemon.clone();
        async move {
            if let Err(e) = daemon.heartbeat_loop().await {
                error!("Heartbeat loop failed: {}", e);
            }
        }
    });
    
    let reconciler_handle = tokio::spawn({
        let reconciler = reconciler.clone();
        async move {
            if let Err(e) = reconciler.run().await {
                error!("Reconciler failed: {}", e);
            }
        }
    });
    
    let command_handle = tokio::spawn({
        let daemon = daemon.clone();
        async move {
            if let Err(e) = daemon.command_consumer().await {
                error!("Command consumer failed: {}", e);
            }
        }
    });
    
    // Wait for shutdown signal
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received SIGINT, shutting down gracefully...");
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM, shutting down gracefully...");
        }
    }
    
    heartbeat_handle.abort();
    reconciler_handle.abort();
    command_handle.abort();
    
    info!("Agent shutdown complete");
    Ok(())
}

/// Agent configuration
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub node_id: Option<uuid::Uuid>,
    pub control_plane_url: String,
    pub join_token: Option<SecretString>,
    pub region: String,
    pub zone: String,
    pub labels: std::collections::HashMap<String, String>,
    
    pub firecracker_binary: std::path::PathBuf,
    pub kernel_image_path: std::path::PathBuf,
    pub data_dir: std::path::PathBuf,
    
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
            labels: std::collections::HashMap::new(),
            
            firecracker_binary: std::env::var("SHELLWEGO_FIRECRACKER_PATH")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| "/usr/local/bin/firecracker".into()),
            kernel_image_path: "/var/lib/shellwego/vmlinux".into(),
            data_dir: "/var/lib/shellwego".into(),
            
            max_microvms: 500,
            reserved_memory_mb: 512,
            reserved_cpu_percent: 10.0,
        })
    }
}

/// Hardware capabilities detection
#[derive(Debug, Clone)]
pub struct Capabilities {
    pub kvm: bool,
    pub nested_virtualization: bool,
    pub cpu_cores: u32,
    pub memory_total_mb: u64,
    pub cpu_features: Vec<String>,
}

fn detect_capabilities() -> anyhow::Result<Capabilities> {
    use std::fs;
    let kvm = fs::metadata("/dev/kvm").is_ok();
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
