//! Virtual Machine Manager
//!
//! Firecracker microVM lifecycle: start, stop, pause, resume.
//! Supports KVM (hardware), PVM (software), and WASM backends.
//! Communicates with Firecracker via Unix socket HTTP API.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use zeroize::{Zeroize, ZeroizeOnDrop};

mod config;
mod driver;
mod pvm;

// Re-export types from schema (these were moved there)
pub use driver::FirecrackerDriver;
pub use pvm::{PvmConfig, PvmRecommendations};
pub use shellwego_schema::{
    DriveConfig, MicrovmConfig, MicrovmMetrics, MicrovmState, MicrovmSummary, NetworkInterface,
    RateLimiterConfig, VirtualizationMode,
};

use crate::metrics::MetricsCollector;
use crate::{detect_capabilities, AgentConfig};

/// Manages all microVMs on this node with support for multiple virtualization backends
#[derive(Clone)]
pub struct VmmManager {
    inner: Arc<RwLock<VmmInner>>,
    driver: FirecrackerDriver,
    data_dir: PathBuf,
    metrics: Arc<MetricsCollector>,
    /// Active virtualization mode
    mode: VirtualizationMode,
    /// Path to the binary for the active mode
    binary_path: PathBuf,
}

struct VmmInner {
    vms: HashMap<uuid::Uuid, RunningVm>,
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
    /// Create a new VMM manager with automatic backend detection
    pub async fn new(config: &AgentConfig, metrics: Arc<MetricsCollector>) -> anyhow::Result<Self> {
        // Detect system capabilities
        let capabilities = detect_capabilities()?;

        // Determine virtualization mode (respect force_mode if set)
        let mode = config
            .force_mode
            .unwrap_or(capabilities.virtualization_mode);

        info!(
            "Initializing VMM manager with {} mode (KVM: {}, PVM: {}, WASM: {})",
            mode,
            capabilities.kvm_available,
            capabilities.pvm_available,
            capabilities.wasm_available
        );

        // Get the appropriate binary for this mode
        let binary_path = config.firecracker_binary_for_mode(mode).clone();

        // Setup environment based on mode
        match mode {
            VirtualizationMode::Kvm => {
                if !capabilities.kvm_available {
                    anyhow::bail!(
                        "KVM mode requested but /dev/kvm not accessible. \
                         Use PVM mode for VPS environments: SHELLWEGO_FORCE_MODE=pvm"
                    );
                }
                info!("Using KVM hardware virtualization");
            }
            VirtualizationMode::Pvm => {
                // Setup PVM environment
                let pvm_config = PvmConfig::with_binary(binary_path.clone());
                pvm::setup_pvm_environment(&pvm_config)?;
                info!("Using PVM software virtualization (works on any VPS)");
            }
            VirtualizationMode::Wasm => {
                info!("Using WASM runtime (microVMs will use WASM functions)");
            }
        }

        // Verify the binary exists for KVM/PVM modes
        if mode != VirtualizationMode::Wasm && !binary_path.exists() {
            let fallback_msg = match mode {
                VirtualizationMode::Kvm => "Install firecracker or use PVM mode",
                VirtualizationMode::Pvm => "Install firecracker-pvm package",
                VirtualizationMode::Wasm => unreachable!(),
            };
            anyhow::bail!(
                "Firecracker binary not found at {:?}. {}",
                binary_path,
                fallback_msg
            );
        }

        // Create driver (only needed for KVM/PVM)
        let driver = if mode != VirtualizationMode::Wasm {
            FirecrackerDriver::new(&binary_path).await?
        } else {
            // Create a placeholder driver for WASM mode (won't be used)
            FirecrackerDriver::new(&PathBuf::from("/bin/true")).await?
        };

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
            mode,
            binary_path,
        })
    }

    /// Get the active virtualization mode
    pub fn mode(&self) -> VirtualizationMode {
        self.mode
    }

    /// Get the binary path for the active mode
    pub fn binary_path(&self) -> &PathBuf {
        &self.binary_path
    }

    /// Start a new microVM
    pub async fn start(&self, config: MicrovmConfig) -> anyhow::Result<()> {
        match self.mode {
            VirtualizationMode::Kvm | VirtualizationMode::Pvm => {
                self.start_firecracker_vm(config).await
            }
            VirtualizationMode::Wasm => self.start_wasm_function(config).await,
        }
    }

    /// Start a Firecracker microVM (KVM or PVM mode)
    async fn start_firecracker_vm(&self, mut config: MicrovmConfig) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;

        if inner.vms.contains_key(&config.app_id) {
            anyhow::bail!("VM for app {} already exists", config.app_id);
        }

        // Adjust config for PVM if needed
        if self.mode == VirtualizationMode::Pvm {
            pvm::adjust_config_for_pvm(&mut config);
        }

        let vm_dir = self.data_dir.join("vms").join(config.app_id.to_string());
        tokio::fs::create_dir_all(&vm_dir).await?;

        let socket_path = vm_dir.join("firecracker.sock");

        // Build Firecracker command
        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("--api-sock").arg(&socket_path);

        // PVM-specific environment variables
        if self.mode == VirtualizationMode::Pvm {
            cmd.env("FIRECRACKER_PVM", "1");
        }

        let mut child = cmd
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

        let spawn_time = start.elapsed();

        info!(
            "Started {} microVM {} for app {} ({}MB, {} CPU) in {:?}",
            self.mode, config.vm_id, config.app_id, config.memory_mb, config.cpu_shares, spawn_time
        );

        self.metrics
            .record_spawn(spawn_time.as_millis() as u64, true);

        inner.vms.insert(
            config.app_id,
            RunningVm {
                config,
                process: Some(child),
                socket_path,
                state: MicrovmState::Running,
                started_at: chrono::Utc::now(),
            },
        );

        Ok(())
    }

    /// Start a WASM function (WASM mode)
    async fn start_wasm_function(&self, config: MicrovmConfig) -> anyhow::Result<()> {
        // Delegate to WASM runtime
        debug!("Starting WASM function for app {}", config.app_id);

        let mut inner = self.inner.write().await;

        if inner.vms.contains_key(&config.app_id) {
            anyhow::bail!("Function for app {} already exists", config.app_id);
        }

        // For WASM mode, we create a placeholder VM entry
        // The actual WASM execution is handled by the wasm module
        let app_id = config.app_id;
        inner.vms.insert(
            app_id,
            RunningVm {
                config,
                process: None,
                socket_path: PathBuf::new(),
                state: MicrovmState::Running,
                started_at: chrono::Utc::now(),
            },
        );

        info!("Started WASM function for app {}", app_id);
        Ok(())
    }

    /// Restore a microVM from a snapshot
    pub async fn restore_from_snapshot(
        &self,
        app_id: uuid::Uuid,
        mem_path: PathBuf,
        snap_path: PathBuf,
    ) -> anyhow::Result<()> {
        if self.mode == VirtualizationMode::Wasm {
            anyhow::bail!("Snapshot restore not supported in WASM mode");
        }

        let mut inner = self.inner.write().await;

        if inner.vms.contains_key(&app_id) {
            anyhow::bail!("VM for app {} already exists", app_id);
        }

        let vm_dir = self.data_dir.join("vms").join(app_id.to_string());
        tokio::fs::create_dir_all(&vm_dir).await?;

        let socket_path = vm_dir.join("firecracker.sock");
        let metrics_path = vm_dir.join("metrics.fifo");

        // Spawn Firecracker process
        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("--api-sock").arg(&socket_path);

        if self.mode == VirtualizationMode::Pvm {
            cmd.env("FIRECRACKER_PVM", "1");
        }

        let mut child = cmd
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

        let driver = self
            .driver
            .for_socket(&socket_path)
            .with_metrics_path(metrics_path);

        // Load Snapshot
        driver
            .load_snapshot(
                mem_path.to_string_lossy().as_ref(),
                snap_path.to_string_lossy().as_ref(),
                false,
            )
            .await?;

        // Create placeholder config
        let recovered_config = MicrovmConfig {
            app_id,
            vm_id: app_id,
            memory_mb: 0,
            cpu_shares: 0,
            kernel_path: PathBuf::new(),
            kernel_boot_args: String::new(),
            drives: vec![],
            network_interfaces: vec![],
            vsock_path: String::new(),
        };

        inner.vms.insert(
            app_id,
            RunningVm {
                config: recovered_config,
                process: Some(child),
                socket_path,
                state: MicrovmState::Running,
                started_at: chrono::Utc::now(),
            },
        );

        info!(
            "Restored {} microVM for app {} from snapshot",
            self.mode, app_id
        );
        Ok(())
    }

    /// Stop and remove a microVM
    pub async fn stop(&self, app_id: uuid::Uuid) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;

        let Some(mut vm) = inner.vms.remove(&app_id) else {
            anyhow::bail!("VM for app {} not found", app_id);
        };

        // Graceful shutdown via API (only for KVM/PVM)
        if self.mode != VirtualizationMode::Wasm {
            let driver = self.driver.for_socket(&vm.socket_path);
            if let Err(e) = driver.stop_vm().await {
                warn!("Graceful shutdown failed: {}, forcing", e);
            }
        }

        // Wait for process exit or timeout
        let timeout = tokio::time::Duration::from_secs(10);
        let child_opt = vm.process.take();
        if let Some(mut child) = child_opt {
            if tokio::time::timeout(timeout, child.wait()).await.is_err() {
                warn!("Firecracker shutdown timeout, forcing SIGKILL");
                if let Err(e) = child.start_kill() {
                    error!("Failed to kill firecracker process: {}", e);
                }
                let _ = child.wait().await;
            }
        }

        // Cleanup socket and logs
        if let Some(parent) = vm.socket_path.parent() {
            let _ = tokio::fs::remove_dir_all(parent).await;
        }

        info!("Stopped {} microVM for app {}", self.mode, app_id);
        Ok(())
    }

    /// List all running microVMs
    pub async fn list_running(&self) -> anyhow::Result<Vec<MicrovmSummary>> {
        let inner = self.inner.read().await;

        Ok(inner
            .vms
            .values()
            .map(|vm| MicrovmSummary {
                app_id: vm.config.app_id,
                vm_id: vm.config.vm_id,
                state: vm.state,
                started_at: vm.started_at,
            })
            .collect())
    }

    /// Get detailed state of a specific microVM
    pub async fn get_state(&self, app_id: uuid::Uuid) -> anyhow::Result<Option<MicrovmState>> {
        let inner = self.inner.read().await;
        Ok(inner.vms.get(&app_id).map(|vm| vm.state))
    }

    /// Pause microVM (for live migration prep)
    pub async fn pause(&self, app_id: uuid::Uuid) -> anyhow::Result<()> {
        if self.mode == VirtualizationMode::Wasm {
            anyhow::bail!("Pause not supported in WASM mode");
        }

        let inner = self.inner.read().await;
        if let Some(vm) = inner.vms.get(&app_id) {
            let driver = self.driver.for_socket(&vm.socket_path);
            driver.pause_vm().await?;
            info!("Paused {} microVM for app {}", self.mode, app_id);
            Ok(())
        } else {
            anyhow::bail!("VM not found");
        }
    }

    /// Resume microVM
    pub async fn resume(&self, app_id: uuid::Uuid) -> anyhow::Result<()> {
        if self.mode == VirtualizationMode::Wasm {
            anyhow::bail!("Resume not supported in WASM mode");
        }

        let inner = self.inner.read().await;
        if let Some(vm) = inner.vms.get(&app_id) {
            let driver = self.driver.for_socket(&vm.socket_path);
            driver.resume_vm().await?;
            info!("Resumed {} microVM for app {}", self.mode, app_id);
            Ok(())
        } else {
            anyhow::bail!("VM not found");
        }
    }

    /// Execute snapshot on the VMM level
    pub async fn snapshot_vm_state(
        &self,
        app_id: uuid::Uuid,
        mem_path: PathBuf,
        snap_path: PathBuf,
    ) -> anyhow::Result<()> {
        if self.mode == VirtualizationMode::Wasm {
            anyhow::bail!("Snapshot not supported in WASM mode");
        }

        let inner = self.inner.read().await;
        if let Some(vm) = inner.vms.get(&app_id) {
            let driver = self.driver.for_socket(&vm.socket_path);
            driver
                .create_snapshot(
                    mem_path.to_string_lossy().as_ref(),
                    snap_path.to_string_lossy().as_ref(),
                )
                .await?;
            Ok(())
        } else {
            anyhow::bail!("VM not found for snapshotting");
        }
    }

    /// Create snapshot for live migration
    ///
    /// This is a convenience method that creates a full snapshot including:
    /// 1. Memory state (via Firecracker API)
    /// 2. Disk state (via ZFS if available)
    ///
    /// The VM is paused during snapshot creation for consistency.
    pub async fn create_snapshot(
        &self,
        app_id: uuid::Uuid,
        snapshot_path: PathBuf,
    ) -> anyhow::Result<()> {
        if self.mode == VirtualizationMode::Wasm {
            anyhow::bail!("Snapshot not supported in WASM mode");
        }

        info!(
            "Creating snapshot for app {} at {:?}",
            app_id, snapshot_path
        );

        // Ensure snapshot directory exists
        if let Some(parent) = snapshot_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mem_path = snapshot_path.with_extension("mem");
        let snap_path = snapshot_path.with_extension("snap");

        // 1. Pause the VM
        debug!("Pausing VM {} for snapshot", app_id);
        self.pause(app_id).await?;

        // Track if we need to resume on error
        let mut should_resume = true;

        // 2. Create memory/disk snapshot via Firecracker
        let snapshot_result = {
            let inner = self.inner.read().await;
            if let Some(vm) = inner.vms.get(&app_id) {
                let driver = self.driver.for_socket(&vm.socket_path);
                driver
                    .create_snapshot(
                        mem_path.to_string_lossy().as_ref(),
                        snap_path.to_string_lossy().as_ref(),
                    )
                    .await
            } else {
                should_resume = false;
                anyhow::bail!("VM {} not found for snapshotting", app_id);
            }
        };

        if let Err(e) = snapshot_result {
            error!("Failed to create snapshot: {}", e);
            if should_resume {
                // Try to resume on failure
                if let Err(resume_err) = self.resume(app_id).await {
                    error!("Failed to resume VM after snapshot failure: {}", resume_err);
                }
            }
            return Err(e);
        }

        // 3. Resume the VM
        debug!("Resuming VM {} after snapshot", app_id);
        self.resume(app_id).await?;

        info!(
            "Successfully created snapshot for app {} at {:?}",
            app_id, snapshot_path
        );

        Ok(())
    }

    /// Get PVM recommendations for the current system
    pub fn get_pvm_recommendations() -> PvmRecommendations {
        pvm::get_recommended_settings()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_virtualization_mode_display() {
        assert_eq!(VirtualizationMode::Kvm.to_string(), "KVM");
        assert_eq!(VirtualizationMode::Pvm.to_string(), "PVM");
        assert_eq!(VirtualizationMode::Wasm.to_string(), "WASM");
    }

    #[test]
    fn test_microvm_config_default() {
        let config = MicrovmConfig::default();
        assert_eq!(config.memory_mb, 128);
        assert_eq!(config.vcpu_count(), 1);
    }

    #[test]
    fn test_microvm_state_default() {
        assert_eq!(MicrovmState::default(), MicrovmState::Uninitialized);
    }

    #[tokio::test]
    async fn test_vmm_manager_wasm_mode_creation() {
        let config = AgentConfig {
            node_id: Some(Uuid::new_v4()),
            force_mode: Some(VirtualizationMode::Wasm),
            data_dir: std::env::temp_dir().join("shellwego-test-wasm"),
            max_microvms: 10,
            ..Default::default()
        };

        let metrics = Arc::new(MetricsCollector::new(Uuid::new_v4()));

        let result = VmmManager::new(&config, metrics).await;
        assert!(result.is_ok(), "VmmManager should initialize in WASM mode");

        let vmm = result.unwrap();
        assert_eq!(vmm.mode(), VirtualizationMode::Wasm);
    }
}
