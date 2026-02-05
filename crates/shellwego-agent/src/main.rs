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

mod daemon;
mod reconciler;
mod vmm;
mod wasm;        // WASM runtime support
mod snapshot;    // VM snapshot management
mod migration;   // Live migration support
mod metrics;

use daemon::Daemon;
use vmm::VmmManager;
use shellwego_network::CniNetwork;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // TODO: Parse CLI args (config path, log level, node-id if recovering)
    // TODO: Initialize structured logging (JSON for production)
    tracing_subscriber::fmt::init();
    
    info!("ShellWeGo Agent starting...");
    
    // Load configuration
    let config = AgentConfig::load()?;
    info!("Node ID: {:?}", config.node_id);
    info!("Control plane: {}", config.control_plane_url);
    
    // Detect capabilities (KVM, CPU features, etc)
    let capabilities = detect_capabilities()?;
    info!("Capabilities: KVM={}, CPUs={}", capabilities.kvm, capabilities.cpu_cores);
    
    // Initialize VMM manager (Firecracker)
    let vmm = VmmManager::new(&config).await?;
    
    // Initialize WASM Runtime
    let _wasm_runtime = wasm::WasmRuntime::new(&wasm::WasmConfig { max_memory_mb: 512 }).await?;

    // Initialize Metrics
    let metrics = Arc::new(metrics::MetricsCollector::new(config.node_id.unwrap_or_default()));

    // Initialize Networking (CNI)
    let network = Arc::new(CniNetwork::new("sw0", "10.0.0.0/16").await?);

    // Initialize daemon (control plane communication)
    let daemon = Daemon::new(config.clone(), capabilities, vmm.clone()).await?;
    
    // Start reconciler (desired state enforcement)
    let reconciler = reconciler::Reconciler::new(vmm.clone(), network, daemon.state_client());
    
    // Additional initialization
    additional_initialization().await?;

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

    let metrics_handle = tokio::spawn({
        let metrics = metrics.clone();
        async move {
            if let Err(e) = metrics.run_collection_loop().await {
                error!("Metrics collection failed: {}", e);
            }
        }
    });
    
    let mut term_signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received SIGINT, shutting down gracefully...");
        }
        _ = term_signal.recv() => {
            info!("Received SIGTERM, shutting down gracefully...");
        }
    }
    
    // Graceful shutdown
    // TODO: Drain running VMs or hand off to another node
    // TODO: Flush metrics and logs
    
    heartbeat_handle.abort();
    reconciler_handle.abort();
    command_handle.abort();
    metrics_handle.abort();
    
    info!("Agent shutdown complete");
    Ok(())
}

/// Additional initialization for WASM, snapshots, and migration
async fn additional_initialization() -> anyhow::Result<()> {
    // Placeholder for future advanced setup (e.g., pre-warming WASM cache)
    Ok(())
}

/// Agent configuration
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub node_id: Option<uuid::Uuid>, // None = new registration
    pub control_plane_url: String,
    pub join_token: Option<SecretString>,
    pub region: String,
    pub zone: String,
    pub labels: std::collections::HashMap<String, String>,
    
    // Paths
    pub firecracker_binary: std::path::PathBuf,
    pub kernel_image_path: std::path::PathBuf,
    pub data_dir: std::path::PathBuf,
    
    // Resource limits
    pub max_microvms: u32,
    pub reserved_memory_mb: u64,
    pub reserved_cpu_percent: f64,
}

impl AgentConfig {
    pub fn load() -> anyhow::Result<Self> {
        // TODO: Load from /etc/shellwego/agent.toml
        // TODO: Override with env vars
        // TODO: Validate paths exist
        
        Ok(Self {
            node_id: None,
            control_plane_url: std::env::var("SHELLWEGO_CP_URL")
                .unwrap_or_else(|_| "127.0.0.1:4433".to_string()),
            join_token: std::env::var("SHELLWEGO_JOIN_TOKEN").ok().map(SecretString::from),
            region: std::env::var("SHELLWEGO_REGION").unwrap_or_else(|_| "unknown".to_string()),
            zone: std::env::var("SHELLWEGO_ZONE").unwrap_or_else(|_| "unknown".to_string()),
            labels: std::collections::HashMap::new(),
            
            firecracker_binary: "/usr/local/bin/firecracker".into(),
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
    
    // Check KVM access
    let kvm = fs::metadata("/dev/kvm").is_ok();
    
    // Get CPU info via sysinfo
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    
    let cpu_cores = sys.cpus().len() as u32;
    let memory_total_mb = sys.total_memory();
    
    // TODO: Check /proc/cpuinfo for vmx/svm flags
    // TODO: Detect nested virt support
    
    Ok(Capabilities {
        kvm,
        nested_virtualization: false, // TODO
        cpu_cores,
        memory_total_mb,
        cpu_features: vec![], // TODO
    })
}
