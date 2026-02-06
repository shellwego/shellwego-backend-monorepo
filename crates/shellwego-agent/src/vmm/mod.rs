//! Virtual Machine Manager
//! 
//! Firecracker microVM lifecycle: start, stop, pause, resume.
//! Communicates with Firecracker via Unix socket HTTP API.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{info, error, warn};
use zeroize::{Zeroize, ZeroizeOnDrop};

mod driver;
mod config;

pub use driver::FirecrackerDriver;
pub use config::{MicrovmConfig, MicrovmState, DriveConfig, NetworkInterface, MicrovmMetrics};

use crate::metrics::MetricsCollector;

/// Manages all microVMs on this node
#[derive(Clone)]
pub struct VmmManager {
    inner: Arc<RwLock<VmmInner>>,
    driver: FirecrackerDriver,
    data_dir: PathBuf,
    metrics: Arc<MetricsCollector>,
}

struct VmmInner {
    vms: HashMap<uuid::Uuid, RunningVm>,
    // TODO: Add metrics collector
}

#[derive(Zeroize, ZeroizeOnDrop)]
struct RunningVm {
    #[zeroize(skip)]
    config: MicrovmConfig,
    #[zeroize(skip)]
    process: Option<tokio::process::Child>,
    #[zeroize(skip)]
    socket_path: PathBuf,
    #[zeroize(skip)]
    state: MicrovmState,
    #[zeroize(skip)]
    started_at: chrono::DateTime<chrono::Utc>,
}

impl VmmManager {
    pub async fn new(config: &crate::AgentConfig, metrics: Arc<MetricsCollector>) -> anyhow::Result<Self> {
        let driver = FirecrackerDriver::new(&config.firecracker_binary).await?;
        
        // Ensure runtime directories exist
        tokio::fs::create_dir_all(&config.data_dir).await?;
        tokio::fs::create_dir_all(config.data_dir.join("vms")).await?;
        tokio::fs::create_dir_all(config.data_dir.join("run")).await?;
        
        Ok(Self {
            inner: Arc::new(RwLock::new(VmmInner {
                vms: HashMap::new(),
            })),
            driver,
            data_dir: config.data_dir.clone(),
            metrics,
        })
    }

    /// Start a new microVM
    pub async fn start(&self, config: MicrovmConfig) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;
        
        if inner.vms.contains_key(&config.app_id) {
            anyhow::bail!("VM for app {} already exists", config.app_id);
        }
        
        let vm_dir = self.data_dir.join("vms").join(config.app_id.to_string());
        tokio::fs::create_dir_all(&vm_dir).await?;
        
        let socket_path = vm_dir.join("firecracker.sock");
        let log_path = vm_dir.join("firecracker.log");
        
        // Spawn Firecracker process
        let mut child = Command::new(&self.driver.binary_path())
            .arg("--api-sock")
            .arg(&socket_path)
            .arg("--id")
            .arg(config.app_id.to_string())
            .arg("--log-path")
            .arg(&log_path)
            .arg("--level")
            .arg("Debug")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
            
        // Wait for socket to be created
        let start = std::time::Instant::now();
        while !socket_path.exists() {
            if start.elapsed().as_secs() > 5 {
                let _ = child.kill().await;
                anyhow::bail!("Firecracker failed to start: socket timeout");
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        
        // Configure VM via API
        let driver = self.driver.for_socket(&socket_path);
        driver.configure_vm(&config).await?;
        
        // Start microVM
        driver.start_vm().await?;
        
        info!(
            "Started microVM {} for app {} ({}MB, {} CPU)",
            config.vm_id, config.app_id, config.memory_mb, config.cpu_shares
        );
        
        self.metrics.record_spawn(start.elapsed().as_millis() as u64, true);
        inner.vms.insert(config.app_id, RunningVm {
            config,
            process: Some(child),
            socket_path,
            state: MicrovmState::Running,
            started_at: chrono::Utc::now(),
        });
        
        Ok(())
    }

    /// Restore a microVM from a snapshot
    pub async fn restore_from_snapshot(
        &self, 
        app_id: uuid::Uuid, 
        mem_path: PathBuf, 
        snap_path: PathBuf
    ) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;
        
        if inner.vms.contains_key(&app_id) {
            anyhow::bail!("VM for app {} already exists", app_id);
        }

        let vm_dir = self.data_dir.join("vms").join(app_id.to_string());
        tokio::fs::create_dir_all(&vm_dir).await?;
        
        let socket_path = vm_dir.join("firecracker.sock");
        let log_path = vm_dir.join("firecracker.log");
        let metrics_path = vm_dir.join("metrics.fifo");

        // Spawn Firecracker process
        // Note: For restore, we don't pass configuration arguments typically, 
        // but we do need the process running and listening on the socket.
        let mut child = Command::new(&self.driver.binary_path())
            .arg("--api-sock")
            .arg(&socket_path)
            .arg("--id")
            .arg(app_id.to_string())
            .arg("--log-path")
            .arg(&log_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Wait for socket
        let start = std::time::Instant::now();
        while !socket_path.exists() {
            if start.elapsed().as_secs() > 5 {
                let _ = child.kill().await;
                anyhow::bail!("Firecracker failed to start for restore: socket timeout");
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        let driver = self.driver.for_socket(&socket_path).with_metrics_path(metrics_path);
        
        // Load Snapshot
        driver.load_snapshot(mem_path.to_str().unwrap(), snap_path.to_str().unwrap(), false).await?;
        
        // Resume handled implicitly by load_snapshot with resume_vm=true or explicitly here if needed
        // driver.resume_vm().await?;

        // Construct a partial config or retrieve it from metadata if possible. 
        // For now, we reconstruct a running VM entry.
        // Note: We lose the original Config here unless we persisted it in the snapshot metadata!
        // This assumes the upper layer (SnapshotManager) handles metadata linkage.
        
        // We create a placeholder config to satisfy the type system. 
        // In production, we'd deserialize the config from snapshot metadata.
        let recovered_config = MicrovmConfig {
            app_id,
            vm_id: app_id, // Reuse app_id as vm_id for simplicity in restore
            memory_mb: 0, // Unknown without metadata
            cpu_shares: 0,
            kernel_path: PathBuf::new(),
            kernel_boot_args: String::new(),
            drives: vec![],
            network_interfaces: vec![],
            vsock_path: String::new(),
        };

        inner.vms.insert(app_id, RunningVm {
            config: recovered_config,
            process: Some(child),
            socket_path,
            state: MicrovmState::Running,
            started_at: chrono::Utc::now(),
        });

        info!("Restored microVM for app {} from snapshot", app_id);
        Ok(())
    }

    /// Stop and remove a microVM
    pub async fn stop(&self, app_id: uuid::Uuid) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;
        
        let Some(mut vm) = inner.vms.remove(&app_id) else {
            anyhow::bail!("VM for app {} not found", app_id);
        };
        
        // Graceful shutdown via API
        let driver = self.driver.for_socket(&vm.socket_path);
        if let Err(e) = driver.stop_vm().await {
            warn!("Graceful shutdown failed: {}, forcing", e);
        }
        
        // Wait for process exit or timeout
        let timeout = tokio::time::Duration::from_secs(10);
        let child_opt = vm.process.take();
        if let Some(mut child) = child_opt {
            // We use child.wait() instead of wait_with_output() to keep ownership on timeout
            if let Err(_) = tokio::time::timeout(timeout, child.wait()).await {
                warn!("Firecracker shutdown timeout, forcing SIGKILL");
                // Graceful shutdown failed, kill the process directly
                if let Err(e) = child.start_kill() {
                    error!("Failed to kill firecracker process: {}", e);
                }
                // Reap the zombie
                let _ = child.wait().await;
            }
        }
        
        // Cleanup socket and logs
        let _ = tokio::fs::remove_dir_all(vm.socket_path.parent().unwrap()).await;
        
        info!("Stopped microVM for app {}", app_id);
        Ok(())
    }

    /// List all running microVMs
    pub async fn list_running(&self) -> anyhow::Result<Vec<MicrovmSummary>> {
        let inner = self.inner.read().await;
        
        Ok(inner.vms.values().map(|vm| MicrovmSummary {
            app_id: vm.config.app_id,
            vm_id: vm.config.vm_id,
            state: vm.state,
            started_at: vm.started_at,
        }).collect())
    }

    /// Get detailed state of a specific microVM
    pub async fn get_state(&self, app_id: uuid::Uuid) -> anyhow::Result<Option<MicrovmState>> {
        let inner = self.inner.read().await;
        Ok(inner.vms.get(&app_id).map(|vm| vm.state))
    }

    /// Pause microVM (for live migration prep)
    pub async fn pause(&self, _app_id: uuid::Uuid) -> anyhow::Result<()> {
        let inner = self.inner.read().await;
        if let Some(vm) = inner.vms.get(&_app_id) {
            let driver = self.driver.for_socket(&vm.socket_path);
            driver.pause_vm().await?;
            info!("Paused microVM for app {}", _app_id);
            Ok(())
        } else {
            anyhow::bail!("VM not found");
        }
    }

    /// Resume microVM
    pub async fn resume(&self, _app_id: uuid::Uuid) -> anyhow::Result<()> {
        let inner = self.inner.read().await;
        if let Some(vm) = inner.vms.get(&_app_id) {
            let driver = self.driver.for_socket(&vm.socket_path);
            driver.resume_vm().await?;
            info!("Resumed microVM for app {}", _app_id);
            Ok(())
        } else {
            anyhow::bail!("VM not found");
        }
    }

    /// Execute snapshot on the VMM level
    pub async fn snapshot_vm_state(&self, app_id: uuid::Uuid, mem_path: PathBuf, snap_path: PathBuf) -> anyhow::Result<()> {
        let inner = self.inner.read().await;
        if let Some(vm) = inner.vms.get(&app_id) {
            let driver = self.driver.for_socket(&vm.socket_path);
            driver.create_snapshot(
                mem_path.to_str().unwrap(),
                snap_path.to_str().unwrap()
            ).await?;
            return Ok(());
        } else {
            anyhow::bail!("VM not found for snapshotting");
        }
    }

    /// Create snapshot for live migration
    pub async fn create_snapshot(
        &self,
        _app_id: uuid::Uuid,
        _snapshot_path: PathBuf,
    ) -> anyhow::Result<()> {
        // TODO: Pause VM
        // TODO: Create memory snapshot
        // TODO: Create disk snapshot via ZFS
        // TODO: Resume VM
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MicrovmSummary {
    pub app_id: uuid::Uuid,
    pub vm_id: uuid::Uuid,
    pub state: MicrovmState,
    pub started_at: chrono::DateTime<chrono::Utc>,
}