# Plan 04: Agent Runtime Activation

## 1. Title & Overview

**Agent Runtime Activation** — Close the five gaps between the agent's structural scaffolding and an operational edge runtime: (A) harden the existing Firecracker process lifecycle with jailer integration, zombie reaping, and supervised monitoring; (B) wire the `shellwego-registry` `ImagePuller` into the reconciler's `prepare_rootfs` pipeline so the agent pulls real OCI images instead of copying a local base file; (C) connect the QUIC command pipeline end-to-end by parsing full `ScheduleApp` payloads into `DesiredApp`, sending `ActionResponse` acknowledgements, and adding missing `Message` variants; (D) implement a real per-VM health check loop (TCP/HTTP probe + process liveness) with status reported to the control plane; (E) attach eBPF firewall and QoS programs to TAP interfaces when VMs are spawned, delegating to the existing `EbpfManager`. Also fix the 5 existing compile errors so the agent builds.

This plan activates the agent from "scaffolded daemon" to "working edge node that receives commands, pulls images, launches VMs, monitors health, and enforces network policy."

## 2. Gap Summary

| # | Expected Behavior | Actual Implementation | File(s) | Severity |
|---|---|---|---|---|
| A | Firecracker processes spawned via jailer with supervised monitoring, zombie reaping, and automatic restart on crash | `VmmManager::start_firecracker_vm` (mod.rs:243) does spawn `Command::new` + `spawn()` + socket wait, but: no jailer wrapper (JailerConfig in driver.rs:619 exists but is unused), no reaper loop (zombie children accumulate), no health-monitored restart, process handle stored but `child.wait()` only called during `stop()`, no cgroup enforcement | `crates/shellwego-agent/src/vmm/mod.rs` lines 243-313, `crates/shellwego-agent/src/vmm/driver.rs` lines 610-682, `crates/shellwego-agent/src/main.rs` | **CRITICAL** |
| B | Agent pulls OCI container images from Docker Hub / GHCR / private registries and converts to ext4 rootfs for Firecracker | `Reconciler::prepare_rootfs` (reconciler.rs:182) checks if `image_path` exists locally, falls back to copying `/var/lib/shellwego/images/base.ext4`, or treats the image string as a raw file path. The `shellwego-registry` crate has a full `ImagePuller` (pull.rs:288) with auth, layer fetching, digest verification — but it is never imported or used by the agent. Agent `Cargo.toml` has no `shellwego-registry` dependency. | `crates/shellwego-agent/src/reconciler.rs` lines 182-216, `crates/shellwego-registry/src/pull.rs`, `crates/shellwego-agent/Cargo.toml` | **CRITICAL** |
| C | Agent receives full deployment specifications over QUIC (image ref, resource limits, env, volumes, network policy) and acts on them; control plane receives action acknowledgements | `Daemon::command_consumer` (daemon.rs:122) receives `Message::ScheduleApp { deployment_id, app_id, image, limits }` but discards `limits`, creates a skeleton `DesiredApp` with hardcoded `image: "default"`, `memory_mb: 128`, `cpu_shares: 1024`. No `Message::ActionResponse` is ever sent back. No `Message` variant for full desired-state sync. `Message` enum (quinn.rs:13) lacks variants for: desired-state push, image-pull progress, per-VM health report, snapshot/migration commands. | `crates/shellwego-agent/src/daemon.rs` lines 122-158, `crates/shellwego-schema/src/network/quinn.rs` lines 13-64 | **HIGH** |
| D | Periodic health check loop probes each running VM (TCP health port, process liveness) and reports unhealthy VMs to control plane | `Reconciler::health_check_loop` (reconciler.rs:282) lists VMs via `list_running()` and logs `"Health check passed"` for each — no actual TCP/HTTP probe, no process liveness check, no failure action, no reporting to control plane. `Daemon::heartbeat_loop` sends node-level metrics but no per-VM health. | `crates/shellwego-agent/src/reconciler.rs` lines 282-290, `crates/shellwego-agent/src/daemon.rs` lines 90-119 | **HIGH** |
| E | eBPF network isolation programs (XDP ingress filter, TC egress rate limiter) attached to each VM's TAP interface on spawn | `EbpfManager` (shellwego-network/src/ebpf/mod.rs:64) has `attach_firewall(iface)` and `apply_qos(iface, limit_mbps)` methods with aya integration and fallback mode. But it is never instantiated or called from the agent. `Reconciler::reconcile_network_policies` (reconciler.rs:272) is an empty loop with a comment `"self.network.update_policy(app.app_id, ...);"`. No import of `EbpfManager` in agent. | `crates/shellwego-network/src/ebpf/mod.rs`, `crates/shellwego-agent/src/reconciler.rs` lines 272-279 | **HIGH** |
| F | Agent compiles without errors | 5 compile errors: unresolved imports for types from `shellwego_schema` (likely due to missing `pub` or re-export), private struct import. Build fails at `cargo build -p shellwego-agent`. | `crates/shellwego-agent/src/vmm/mod.rs` lines 22-25, `crates/shellwego-agent/src/lib.rs` | **BLOCKER** |

## 3. Scope

### Files to Modify

| File | Change |
|---|---|
| `crates/shellwego-agent/Cargo.toml` | Add `shellwego-registry` dependency; add `aya` (optional via feature); fix any missing dep for compile errors |
| `crates/shellwego-agent/src/vmm/mod.rs` | Add `ProcessMonitor` task that reaps zombie children; add jailer integration to `start_firecracker_vm`; add `attach_ebpf` method; wire `MetricsCollector::set_microvm_count` on start/stop |
| `crates/shellwego-agent/src/vmm/driver.rs` | Wire `JailerConfig` into driver — add `spawn_with_jailer()` method that uses jailer args + adjusted socket path |
| `crates/shellwego-agent/src/reconciler.rs` | Replace `prepare_rootfs` with OCI image pull pipeline using `shellwego-registry::ImagePuller`; add `image_digest` tracking to detect image updates; wire eBPF attach on `create_microvm`; implement real `health_check_loop` with TCP probe; send `ActionResponse` via QUIC on success/failure |
| `crates/shellwego-agent/src/daemon.rs` | Parse full `ScheduleApp` payload into `DesiredApp` with limits; send `ActionResponse` for every command; add `report_vm_health` QUIC message; store `node_id` from registration response (or self-assign); handle `ScheduleApp` with full image, env, volumes |
| `crates/shellwego-agent/src/lib.rs` | Add `pub mod image;` for new image pulling module; fix any unresolved re-exports |
| `crates/shellwego-agent/src/main.rs` | Spawn `ProcessMonitor` task; pass `EbpfManager` to reconciler; instantiate `ImagePuller` and pass to reconciler |
| `crates/shellwego-agent/src/metrics.rs` | Add per-VM health status to `ResourceSnapshot`; add `shellwego_agent_vm_health_status` gauge |
| `crates/shellwego-schema/src/network/quinn.rs` | Add `Message` variants: `DesiredStatePush`, `VmHealthReport`, `ImagePullProgress`, `SnapshotCommand`, `MigrationCommand`, `ActionResponse` (already exists but needs `app_id` field); update `ScheduleApp` to carry full spec (env, volumes, boot_args) |

### New Files to Create

| File | Purpose |
|---|---|
| `crates/shellwego-agent/src/image.rs` | `ImageService` struct wrapping `shellwego-registry::ImagePuller` + `LayerCache`; manages OCI-to-ext4 conversion; tracks pulled image digests; provides `pull_or_cache(image_ref) -> PathBuf` |
| `crates/shellwego-agent/src/health.rs` | `HealthChecker` struct with configurable TCP/HTTP probes; per-VM health state tracking; `check(app_id) -> HealthStatus`; reports unhealthy VMs to daemon for QUIC reporting |
| `crates/shellwego-agent/src/vmm/monitor.rs` | `ProcessMonitor` that runs a background tokio task to reap zombie Firecracker/jailer children via `waitpid`; reports unexpected exits; triggers restart based on restart policy |

## 4. Prerequisites

1. **Agent must compile** — Fix the 5 existing compile errors first (Phase F, step F1). The errors are unresolved imports from `shellwego_schema` in `vmm/mod.rs` lines 22-25. The re-exports in `lib.rs` lines 19-45 list these types, but some may not actually be `pub` in `shellwego_schema`. Verify each type exists and is `pub` in the schema crate.

2. **Plan 03: QUIC Message Bus must be landed** — This plan extends the `Message` enum and relies on a functioning bidirectional QUIC connection between agent and control plane. If Plan 03 adds authentication, multiplexed streams, or reconnect logic, this plan builds on top of it. Specifically:
   - The `QuinnClient` must support concurrent `send` and `receive` (it does today via `open_bi` / `accept_bi`).
   - Registration must assign a `node_id` (currently `node_id` in daemon is `Option<Uuid>` that stays `None` — Plan 03 should add a `RegisterResponse` message carrying the assigned node ID).

3. **`shellwego-registry` crate must compile** — It uses `reqwest`, `sha2`, `serde_json`, and `shellwego_schema::oci` types. Verify `cargo build -p shellwego-registry` succeeds. The `LayerCache::import_image` method (called from `pull_with_progress`) must exist and work.

4. **eBPF toolchain optional** — The `EbpfManager` operates in fallback mode when the `ebpf` feature is disabled or the binary is empty. This plan wires it in but does NOT require the `aya` toolchain. The eBPF attachment code must gracefully degrade.

5. **Linux host required for integration tests** — Process spawning, TAP device creation, and eBPF attachment require Linux. Unit tests can run on any platform with mocks.

## 5. Detailed Implementation Steps

### Phase F: Fix Compile Errors (BLOCKER)

**F1. Diagnose and fix 5 unresolved imports**

File: `crates/shellwego-agent/src/vmm/mod.rs` lines 22-25

The re-exports reference these types from `shellwego_schema`:
```rust
pub use shellwego_schema::{
    DriveConfig, MicrovmConfig, MicrovmMetrics, MicrovmState, MicrovmSummary, NetworkInterface,
    RateLimiterConfig, VirtualizationMode,
};
```

For each type, verify it is `pub` in the schema crate:
- `DriveConfig` → check `crates/shellwego-schema/src/firecracker/drives.rs`
- `MicrovmConfig` → check `crates/shellwego-schema/src/vmm/config.rs`
- `MicrovmMetrics` → check `crates/shellwego-schema/src/vmm/metrics.rs`
- `MicrovmState` → check `crates/shellwego-schema/src/vmm/state.rs`
- `MicrovmSummary` → check `crates/shellwego-schema/src/vmm/state.rs`
- `NetworkInterface` → check `crates/shellwego-schema/src/firecracker/network.rs`
- `RateLimiterConfig` → check `crates/shellwego-schema/src/firecracker/network.rs`
- `VirtualizationMode` → check `crates/shellwego-schema/src/vmm/virtualization.rs`

If any type is missing from `shellwego_schema::lib.rs` re-exports, add it there. If the type doesn't exist in schema, define it in the agent's `vmm/config.rs` and re-export from there.

Also fix any private struct import errors — if `FirecrackerClient` methods use types that are not `pub`, make them `pub`.

**F2. Verify clean build**

```bash
cargo build -p shellwego-agent 2>&1 | head -50
```

All errors must be zero before proceeding.

### Phase A: Firecracker Process Lifecycle Hardening

**A1. Create ProcessMonitor for zombie reaping**

File: `crates/shellwego-agent/src/vmm/monitor.rs` (NEW)

```rust
//! Firecracker process monitor
//!
//! Background task that reaps zombie Firecracker/jailer child processes,
//! reports unexpected exits, and triggers restarts per policy.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Event emitted when a VM process exits unexpectedly
#[derive(Debug, Clone)]
pub enum VmExitEvent {
    /// VM process exited with code
    Exited { app_id: Uuid, exit_code: Option<i32> },
    /// VM process killed by signal
    Signaled { app_id: Uuid, signal: i32 },
}

/// Restart policy for crashed VMs
#[derive(Debug, Clone, Copy, Default)]
pub enum RestartPolicy {
    /// Do not restart
    Never,
    /// Always restart (default)
    #[default]
    Always,
    /// Restart up to N times
    OnFailure(u32),
}

/// Handle to the process monitor for sending events
#[derive(Clone)]
pub struct ProcessMonitorHandle {
    /// Channel to register new child processes
    register_tx: mpsc::Sender<(Uuid, tokio::process::Child, RestartPolicy)>,
}

impl ProcessMonitorHandle {
    /// Register a child process for monitoring
    pub async fn register(
        &self,
        app_id: Uuid,
        child: tokio::process::Child,
        policy: RestartPolicy,
    ) {
        let _ = self.register_tx.send((app_id, child, policy)).await;
    }
}

/// Spawn the process monitor background task
pub fn spawn_process_monitor(
) -> (ProcessMonitorHandle, mpsc::Receiver<VmExitEvent>) {
    let (register_tx, mut register_rx) = mpsc::channel::<(Uuid, tokio::process::Child, RestartPolicy)>(64);
    let (event_tx, event_rx) = mpsc::channel::<VmExitEvent>(64);

    let handle = ProcessMonitorHandle { register_tx };

    tokio::spawn(async move {
        let mut children: HashMap<Uuid, (tokio::process::Child, RestartPolicy, u32)> = HashMap::new();
        let mut check_interval = interval(Duration::from_secs(2));

        loop {
            tokio::select! {
                // New child registration
                Some((app_id, child, policy)) = register_rx.recv() => {
                    info!("Monitoring process for app {}", app_id);
                    children.insert(app_id, (child, policy, 0));
                }
                // Periodic reap check
                _ = check_interval.tick() => {
                    let mut to_restart = Vec::new();

                    for (app_id, (child, policy, restart_count)) in children.iter_mut() {
                        // Try non-blocking wait to detect exited children
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                let event = if status.success() {
                                    info!("App {} process exited cleanly", app_id);
                                    VmExitEvent::Exited { app_id: *app_id, exit_code: status.code() }
                                } else {
                                    error!("App {} process exited with error: {}", app_id, status);
                                    VmExitEvent::Signaled { app_id: *app_id, signal: 0 }
                                };
                                let _ = event_tx.send(event).await;
                                to_restart.push((*app_id, *policy, *restart_count));
                            }
                            Ok(None) => {
                                // Still running
                            }
                            Err(e) => {
                                warn!("Error checking process for {}: {}", app_id, e);
                            }
                        }
                    }

                    // Remove exited children
                    for (app_id, policy, restart_count) in &to_restart {
                        children.remove(app_id);
                    }
                }
            }
        }
    });

    (handle, event_rx)
}
```

**A2. Integrate monitor into VmmManager**

File: `crates/shellwego-agent/src/vmm/mod.rs`

Changes:
- Add `monitor: ProcessMonitorHandle` field to `VmmManager`
- In `VmmManager::new()`, call `spawn_process_monitor()` and store the handle
- In `start_firecracker_vm()` after `cmd.spawn()`, call `self.monitor.register(config.app_id, child, RestartPolicy::Always).await`
- In `stop()`, no additional change needed (child is already killed and reaped)
- Add `pub fn monitor_handle(&self) -> &ProcessMonitorHandle` for external consumers
- Update `VmmInner` to remove the `process: Option<tokio::process::Child>` field since the monitor owns it now — instead, track `process_alive: bool`

**A3. Wire jailer into driver spawn path**

File: `crates/shellwego-agent/src/vmm/driver.rs`

Add a new method to `FirecrackerDriver`:

```rust
impl FirecrackerDriver {
    /// Spawn Firecracker via jailer for isolation
    pub fn spawn_with_jailer(
        &self,
        jailer_config: &JailerConfig,
        app_id: &uuid::Uuid,
        socket_path: &Path,
    ) -> anyhow::Result<tokio::process::Child> {
        let jailer_binary = "/usr/local/bin/jailer";
        if !Path::new(jailer_binary).exists() {
            anyhow::bail!("Jailer binary not found at {}", jailer_binary);
        }

        let args = jailer_config.build_args(app_id);
        let mut cmd = tokio::process::Command::new(jailer_binary);
        cmd.args(&args);

        // Set PVM environment if applicable
        if self.mode == Some(crate::VirtualizationMode::Pvm) {
            cmd.env("FIRECRACKER_PVM", "1");
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let child = cmd.spawn()?;
        info!("Spawned jailer for app {} with socket {:?}", app_id, socket_path);

        Ok(child)
    }

    /// Get the jailer-adjusted socket path
    pub fn jailer_socket_path(
        &self,
        jailer_config: &JailerConfig,
        app_id: &uuid::Uuid,
    ) -> PathBuf {
        jailer_config.socket_path(app_id)
    }
}
```

File: `crates/shellwego-agent/src/vmm/mod.rs` in `start_firecracker_vm()`:

Add `use_jailer: bool` to `VmmManager` config. When `use_jailer` is true:
1. Call `self.driver.spawn_with_jailer(&jailer_config, &config.app_id, &socket_path)`
2. Use `self.driver.jailer_socket_path(&jailer_config, &config.app_id)` instead of `vm_dir.join("firecracker.sock")`
3. Wait for jailer socket (same polling loop as existing)

When `use_jailer` is false, use the existing `Command::new` path.

**A4. Wire `MetricsCollector::set_microvm_count`**

In `VmmManager::start_firecracker_vm()`, after successful VM insert:
```rust
self.metrics.set_microvm_count(inner.vms.len() as u32);
```

In `VmmManager::stop()`, after successful removal:
```rust
self.metrics.set_microvm_count(inner.vms.len() as u32);
```

### Phase B: OCI Image Pulling Pipeline

**B1. Add `shellwego-registry` dependency**

File: `crates/shellwego-agent/Cargo.toml`

Add under `[dependencies]`:
```toml
shellwego-registry = { path = "../shellwego-registry" }
```

**B2. Create ImageService**

File: `crates/shellwego-agent/src/image.rs` (NEW)

```rust
//! OCI image pulling and caching for the agent
//!
//! Wraps `shellwego-registry::ImagePuller` with local caching,
//! digest tracking, and OCI-to-ext4 rootfs conversion.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

use shellwego_registry::{ImagePuller, LayerCache, PulledImage, PullProgress, RegistryAuth};

/// Image service managing OCI image pulls and local cache
#[derive(Clone)]
pub struct ImageService {
    puller: Arc<ImagePuller>,
    cache_dir: PathBuf,
    /// Map of image_ref -> (digest, rootfs_path) for update detection
    digests: Arc<RwLock<HashMap<String, ImageDigest>>>,
}

#[derive(Debug, Clone)]
struct ImageDigest {
    digest: String,
    rootfs_path: PathBuf,
    pulled_at: chrono::DateTime<chrono::Utc>,
}

impl ImageService {
    pub fn new(cache_dir: PathBuf) -> anyhow::Result<Self> {
        tokio::fs::create_dir_all(&cache_dir)?;

        let cache = LayerCache::new(cache_dir.join("layers"));

        let mut puller = ImagePuller::with_cache(cache);
        // Add registry auth from environment if configured
        if let Ok(registry) = std::env::var("SHELLWEGO_REGISTRY") {
            if let Ok(username) = std::env::var("SHELLWEGO_REGISTRY_USER") {
                if let Ok(password) = std::env::var("SHELLWEGO_REGISTRY_PASS") {
                    puller.add_auth(&registry, RegistryAuth::basic(&username, &password));
                }
            }
        }

        Ok(Self {
            puller: Arc::new(puller),
            cache_dir,
            digests: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Pull an image or return cached rootfs path
    ///
    /// Returns the path to the ext4 rootfs ready for Firecracker.
    /// If the image digest matches the cache, returns the cached path.
    pub async fn pull_or_cache(&self, image_ref: &str) -> anyhow::Result<PathBuf> {
        // Check cache
        let digests = self.digests.read().await;
        if let Some(cached) = digests.get(image_ref) {
            if cached.rootfs_path.exists() {
                info!("Using cached rootfs for {}: {:?}", image_ref, cached.rootfs_path);
                return Ok(cached.rootfs_path.clone());
            }
        }
        drop(digests);

        // Pull from registry
        info!("Pulling OCI image: {}", image_ref);
        let pulled = self.puller.pull(image_ref, None).await
            .map_err(|e| anyhow::anyhow!("Failed to pull image {}: {}", image_ref, e))?;

        // Convert to ext4 rootfs
        let rootfs_path = self.image_to_rootfs(image_ref, &pulled).await?;

        // Update cache
        let mut digests = self.digests.write().await;
        let digest = pulled.manifest.layers.first()
            .map(|l| l.digest.clone())
            .unwrap_or_default();

        digests.insert(image_ref.to_string(), ImageDigest {
            digest,
            rootfs_path: rootfs_path.clone(),
            pulled_at: chrono::Utc::now(),
        });

        Ok(rootfs_path)
    }

    /// Check if an image update is available
    pub async fn has_update(&self, image_ref: &str) -> anyhow::Result<bool> {
        // For now, always return false since we don't track remote digests
        // A full implementation would HEAD the registry manifest and compare
        Ok(false)
    }

    /// Get the current digest for a cached image
    pub async fn get_digest(&self, image_ref: &str) -> Option<String> {
        let digests = self.digests.read().await;
        digests.get(image_ref).map(|d| d.digest.clone())
    }

    /// Convert a pulled OCI image to an ext4 rootfs
    ///
    /// This uses the LayerCache's import_image method if available,
    /// or falls back to creating a minimal ext4 from the image layers.
    async fn image_to_rootfs(
        &self,
        image_ref: &str,
        pulled: &PulledImage,
    ) -> anyhow::Result<PathBuf> {
        let safe_name = image_ref.replace(|c: char| !c.is_alphanumeric(), "_");
        let rootfs_path = self.cache_dir.join(format!("{}.ext4", safe_name));

        if rootfs_path.exists() {
            return Ok(rootfs_path);
        }

        // If the puller returned a rootfs_path from cache, use it
        if let Some(ref cached_path) = pulled.rootfs_path {
            if cached_path.exists() {
                info!("Using registry cache rootfs: {:?}", cached_path);
                // Copy to our expected location
                tokio::fs::copy(cached_path, &rootfs_path).await?;
                return Ok(rootfs_path);
            }
        }

        // Fall back to creating an ext4 image from layers
        // This requires root privileges and `mkfs.ext4` + `mount`
        info!("Creating ext4 rootfs for {} ({} bytes of layers)", image_ref, pulled.size_bytes);
        self.create_ext4_from_layers(&safe_name, &pulled.layer_digests, pulled.size_bytes).await?;

        Ok(rootfs_path)
    }

    /// Create ext4 filesystem from OCI layers
    async fn create_ext4_from_layers(
        &self,
        name: &str,
        _layer_digests: &[String],
        size_bytes: u64,
    ) -> anyhow::Result<()> {
        let rootfs_path = self.cache_dir.join(format!("{}.ext4", name));
        let staging_dir = self.cache_dir.join(format!("{}_staging", name));

        // Create a sparse ext4 image (2x layer size, minimum 256MB)
        let image_size = ((size_bytes as f64 * 2.0).max(256.0 * 1024.0 * 1024.0)) as u64;

        // Use dd + mkfs.ext4
        let output = tokio::process::Command::new("dd")
            .args(["if=/dev/zero", "of=rootfs.img", "bs=1", "count=0", "seek=1"])
            .arg(format!("seek={}", image_size))
            .current_dir(&self.cache_dir)
            .output().await?;
        if !output.status.success() {
            anyhow::bail!("dd failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let output = tokio::process::Command::new("mkfs.ext4")
            .args(["-F", "rootfs.img"])
            .current_dir(&self.cache_dir)
            .output().await?;
        if !output.status.success() {
            anyhow::bail!("mkfs.ext4 failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        // Mount and extract layers
        tokio::fs::create_dir_all(&staging_dir).await?;

        let mount_output = tokio::process::Command::new("mount")
            .args(["-o", "loop", "rootfs.img", &staging_dir.to_string_lossy()])
            .current_dir(&self.cache_dir)
            .output().await?;
        if !mount_output.status.success() {
            // Clean up the image file
            let _ = tokio::fs::remove_file(&rootfs_path).await;
            anyhow::bail!("mount failed: {}", String::from_utf8_lossy(&mount_output.stderr));
        }

        // Extract layers into staging (requires tar + layer tooling)
        // For production, use umoci or skopeo; for now, create minimal structure
        let _ = tokio::process::Command::new("mkdir")
            .args(["-p", &format!("{}/bin", staging_dir.display()), &format!("{}/lib", staging_dir.display())])
            .output().await;

        // Unmount
        let _ = tokio::process::Command::new("umount")
            .arg(&staging_dir.to_string_lossy())
            .output().await;

        // Rename to final path
        let temp_path = self.cache_dir.join("rootfs.img");
        tokio::fs::rename(&temp_path, &rootfs_path).await?;

        info!("Created ext4 rootfs at {:?} ({} bytes)", rootfs_path, image_size);
        Ok(())
    }
}
```

**B3. Replace `prepare_rootfs` in Reconciler**

File: `crates/shellwego-agent/src/reconciler.rs`

Replace the `prepare_rootfs` method (lines 182-216):

```rust
async fn prepare_rootfs(&self, image: &str) -> anyhow::Result<std::path::PathBuf> {
    // Delegate to ImageService for OCI pull + cache
    self.image_service.pull_or_cache(image).await
}
```

Add `image_service: ImageService` field to `Reconciler` struct and update `Reconciler::new()` to accept it.

**B4. Track running image digest for update detection**

Add `running_images: Arc<RwLock<HashMap<Uuid, String>>>` to `Reconciler`.

In `create_microvm()`, after successful pull:
```rust
if let Some(digest) = self.image_service.get_digest(&app.image).await {
    self.running_images.write().await.insert(app.app_id, digest);
}
```

In `check_image_updates()`:
```rust
pub async fn check_image_updates(&self, app: &DesiredApp) -> anyhow::Result<bool> {
    if self.image_service.has_update(&app.image).await? {
        return true;
    }
    // Also check if running digest differs from current remote digest
    let running = self.running_images.read().await;
    if let Some(current_digest) = running.get(&app.app_id) {
        // Pull latest manifest digest and compare
        // (requires adding a method to ImageService that only fetches manifest)
    }
    Ok(false)
}
```

### Phase C: QUIC Command Pipeline End-to-End

**C1. Extend `Message` enum with needed variants**

File: `crates/shellwego-schema/src/network/quinn.rs`

Add these variants to the `Message` enum (after the existing `ActionResponse`):

```rust
    /// Full desired state push from control plane
    DesiredStatePush {
        /// Full list of desired apps
        apps: Vec<DesiredAppMessage>,
        /// Generation number (for ordering)
        generation: u64,
    },
    /// Per-VM health report from agent to control plane
    VmHealthReport {
        /// Node ID
        node_id: Uuid,
        /// Individual VM health statuses
        vm_health: Vec<VmHealthEntry>,
    },
    /// Image pull progress from agent
    ImagePullProgress {
        /// App ID being pulled for
        app_id: Uuid,
        /// Image reference
        image: String,
        /// Pull stage
        stage: ImagePullStage,
        /// Progress 0-100
        progress_percent: f64,
        /// Error message if failed
        error: Option<String>,
    },
```

Add supporting types:

```rust
/// Desired app specification as sent over QUIC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesiredAppMessage {
    pub app_id: Uuid,
    pub image: String,
    pub command: Option<Vec<String>>,
    pub memory_mb: u32,
    pub cpu_shares: u32,
    pub env: Vec<(String, String)>,
    pub volumes: Vec<String>,
    pub boot_args: Option<String>,
}

/// VM health entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmHealthEntry {
    pub app_id: Uuid,
    pub healthy: bool,
    pub status: String,
    pub last_check: DateTime<Utc>,
}

/// Image pull stages
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ImagePullStage {
    Manifest,
    Layers,
    Converting,
    Complete,
    Failed,
}
```

**C2. Enhance `ScheduleApp` to carry full spec**

File: `crates/shellwego-schema/src/network/quinn.rs`

Extend the existing `ScheduleApp` variant:

```rust
    /// Schedule app on agent
    ScheduleApp {
        deployment_id: Uuid,
        app_id: Uuid,
        image: String,
        limits: ResourceLimits,
        /// Command to run in the container
        command: Option<Vec<String>>,
        /// Environment variables
        env: Vec<(String, String)>,
        /// Volume mount specs
        volumes: Vec<String>,
        /// Custom kernel boot arguments
        boot_args: Option<String>,
    },
```

**C3. Parse full ScheduleApp payload in Daemon**

File: `crates/shellwego-agent/src/daemon.rs`

Replace the `ScheduleApp` handler in `command_consumer()` (lines 125-145):

```rust
Ok(Message::ScheduleApp {
    app_id,
    image,
    limits,
    command,
    env,
    volumes,
    boot_args,
    ..
}) => {
    info!("CP ordered: Schedule app {} (image={}, memory={}bytes, cpu={}mcores)",
        app_id, image, limits.memory_bytes, limits.cpu_milli);

    let memory_mb = (limits.memory_bytes / 1024 / 1024).max(32) as u32;
    let cpu_shares = (limits.cpu_milli / 10).max(64) as u32;

    let env_map: std::collections::HashMap<String, String> = env.into_iter().collect();

    let volume_mounts: Vec<shellwego_schema::VolumeMount> = volumes.iter().map(|v| {
        let parts: Vec<&str> = v.splitn(2, ':').collect();
        shellwego_schema::VolumeMount {
            volume_id: uuid::Uuid::new_v4(),
            mount_path: parts.get(1).unwrap_or(&"/data").to_string(),
            device: std::path::PathBuf::from(parts.get(0).unwrap_or(&v)),
            read_only: false,
        }
    }).collect();

    let desired_app = DesiredApp {
        app_id,
        image,
        command: command.map(|c| c.join(" ")),
        memory_mb,
        cpu_shares,
        env: env_map,
        volumes: volume_mounts,
    };

    let mut cache = self.state_cache.write().await;
    // Upsert (update if exists, insert if new)
    if let Some(existing) = cache.apps.iter_mut().find(|a| a.app_id == app_id) {
        *existing = desired_app;
    } else {
        cache.apps.push(desired_app);
    }

    // Send acknowledgement
    let response = Message::ActionResponse {
        request_id: deployment_id, // Use deployment_id as request_id
        success: true,
        error: None,
    };
    if let Err(e) = self.quic.lock().await.send(response).await {
        error!("Failed to send ScheduleApp ack: {}", e);
    }
}
```

**C4. Add `VmHealthReport` sending to heartbeat loop**

File: `crates/shellwego-agent/src/daemon.rs`

In `heartbeat_loop()`, after the existing heartbeat, also send VM health report:

```rust
// Report per-VM health if health checker is available
if let Some(ref health_report) = self.get_health_report().await {
    let health_msg = Message::VmHealthReport {
        node_id: node_id.unwrap_or_default(),
        vm_health: health_report,
    };
    let _ = self.quic.lock().await.send(health_msg).await;
}
```

**C5. Handle `RegisterResponse` for node_id assignment**

Currently, `register()` sends `Message::Register` but never reads a response. The control plane should respond with a `RegisterResponse` carrying the assigned `node_id`. Modify `register()`:

```rust
async fn register(&self) -> anyhow::Result<()> {
    // ... existing connect + send code ...

    // Wait for registration response (with timeout)
    let response = tokio::time::timeout(
        Duration::from_secs(10),
        self.quic.lock().await.receive()
    ).await;

    if let Ok(Ok(Message::ActionResponse { request_id: _, success, error })) = response {
        if success {
            info!("Registration acknowledged by control plane");
        } else {
            anyhow::bail!("Registration rejected: {:?}", error);
        }
    } else {
        warn!("No registration response received, proceeding with local node ID");
    }

    // Use locally generated node_id if not assigned by CP
    let mut node_id = self.node_id.write().await;
    if node_id.is_none() {
        *node_id = Some(uuid::Uuid::new_v4());
    }

    Ok(())
}
```

### Phase D: Health Check Loop

**D1. Create HealthChecker module**

File: `crates/shellwego-agent/src/health.rs` (NEW)

```rust
//! VM health checking
//!
//! Probes running microVMs for liveness via TCP/HTTP health endpoints
//! and process status monitoring.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Health status of a VM
#[derive(Debug, Clone, serde::Serialize)]
pub struct VmHealthStatus {
    pub app_id: Uuid,
    pub healthy: bool,
    pub status: HealthState,
    pub last_check: DateTime<Utc>,
    pub consecutive_failures: u32,
    pub detail: String,
}

/// VM health states
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum HealthState {
    /// VM is healthy and responding
    Healthy,
    /// VM is starting up (grace period)
    Starting,
    /// VM health check failed
    Unhealthy,
    /// VM process has exited
    Dead,
    /// Health check not yet performed
    Unknown,
}

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// How often to check each VM
    pub interval_secs: u64,
    /// TCP connect timeout
    pub timeout_secs: u64,
    /// Number of consecutive failures before marking unhealthy
    pub failure_threshold: u32,
    /// Grace period after VM start before checking (seconds)
    pub startup_grace_secs: u64,
    /// Health check type
    pub check_type: HealthCheckType,
}

/// Type of health probe
#[derive(Debug, Clone)]
pub enum HealthCheckType {
    /// TCP connect to port
    TcpPort(u16),
    /// HTTP GET to path
    Http { port: u16, path: String, status_ok: u16 },
    /// Process liveness only (no network probe)
    ProcessOnly,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval_secs: 10,
            timeout_secs: 3,
            failure_threshold: 3,
            startup_grace_secs: 30,
            check_type: HealthCheckType::TcpPort(8080),
        }
    }
}

/// Health checker for running VMs
#[derive(Clone)]
pub struct HealthChecker {
    config: HealthCheckConfig,
    statuses: Arc<RwLock<HashMap<Uuid, VmHealthStatus>>>,
    /// Track when each VM was started for grace period
    start_times: Arc<RwLock<HashMap<Uuid, DateTime<Utc>>>>,
}

impl HealthChecker {
    pub fn new(config: HealthCheckConfig) -> Self {
        Self {
            config,
            statuses: Arc::new(RwLock::new(HashMap::new())),
            start_times: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a VM for health checking
    pub async fn register_vm(&self, app_id: Uuid) {
        self.start_times.write().await.insert(app_id, Utc::now());
        self.statuses.write().await.insert(app_id, VmHealthStatus {
            app_id,
            healthy: true, // Assume healthy until proven otherwise
            status: HealthState::Starting,
            last_check: Utc::now(),
            consecutive_failures: 0,
            detail: "VM starting".to_string(),
        });
    }

    /// Unregister a VM (on stop)
    pub async fn unregister_vm(&self, app_id: &Uuid) {
        self.start_times.write().await.remove(app_id);
        self.statuses.write().await.remove(app_id);
    }

    /// Run a single health check against a VM
    pub async fn check_vm(&self, app_id: Uuid, guest_ip: &str) -> VmHealthStatus {
        // Check grace period
        let now = Utc::now();
        let in_grace = self.start_times.read().await
            .get(&app_id)
            .map(|start| {
                (now - *start).num_seconds() < self.config.startup_grace_secs as i64
            })
            .unwrap_or(false);

        if in_grace {
            debug!("VM {} in startup grace period", app_id);
            return VmHealthStatus {
                app_id,
                healthy: true,
                status: HealthState::Starting,
                last_check: now,
                consecutive_failures: 0,
                detail: "In startup grace period".to_string(),
            };
        }

        let result = match &self.config.check_type {
            HealthCheckType::TcpPort(port) => {
                self.tcp_probe(guest_ip, *port).await
            }
            HealthCheckType::Http { port, path, status_ok } => {
                self.http_probe(guest_ip, *port, path, *status_ok).await
            }
            HealthCheckType::ProcessOnly => {
                Ok(true) // Process-only: if registered, it's alive
            }
        };

        let mut statuses = self.statuses.write().await;
        let entry = statuses.entry(app_id).or_insert_with(|| VmHealthStatus {
            app_id,
            healthy: false,
            status: HealthState::Unknown,
            last_check: now,
            consecutive_failures: 0,
            detail: String::new(),
        });

        match result {
            Ok(true) => {
                entry.healthy = true;
                entry.status = HealthState::Healthy;
                entry.consecutive_failures = 0;
                entry.detail = "Health probe passed".to_string();
            }
            Ok(false) => {
                entry.consecutive_failures += 1;
                if entry.consecutive_failures >= self.config.failure_threshold {
                    entry.healthy = false;
                    entry.status = HealthState::Unhealthy;
                    entry.detail = format!(
                        "Health probe failed {} consecutive times",
                        entry.consecutive_failures
                    );
                } else {
                    entry.detail = format!(
                        "Health probe failed (failure {}/{})",
                        entry.consecutive_failures, self.config.failure_threshold
                    );
                }
            }
            Err(e) => {
                entry.consecutive_failures += 1;
                if entry.consecutive_failures >= self.config.failure_threshold {
                    entry.healthy = false;
                    entry.status = HealthState::Unhealthy;
                }
                entry.detail = format!("Health probe error: {}", e);
            }
        }

        entry.last_check = now;
        entry.clone()
    }

    /// Mark a VM as dead (process exited)
    pub async fn mark_dead(&self, app_id: Uuid) {
        let mut statuses = self.statuses.write().await;
        if let Some(entry) = statuses.get_mut(&app_id) {
            entry.healthy = false;
            entry.status = HealthState::Dead;
            entry.detail = "Process exited".to_string();
            entry.last_check = Utc::now();
        }
    }

    /// Get all VM health statuses
    pub async fn get_all_statuses(&self) -> Vec<VmHealthStatus> {
        self.statuses.read().await.values().cloned().collect()
    }

    /// TCP probe
    async fn tcp_probe(&self, host: &str, port: u16) -> anyhow::Result<bool> {
        let addr = format!("{}:{}", host, port);
        let result = timeout(
            Duration::from_secs(self.config.timeout_secs),
            TcpStream::connect(&addr)
        ).await;

        match result {
            Ok(Ok(_stream)) => Ok(true),
            Ok(Err(_)) => Ok(false),
            Err(_) => Ok(false), // Timeout
        }
    }

    /// HTTP probe
    async fn http_probe(&self, host: &str, port: u16, path: &str, status_ok: u16) -> anyhow::Result<bool> {
        let url = format!("http://{}:{}{}", host, port, path);
        let result = timeout(
            Duration::from_secs(self.config.timeout_secs),
            reqwest::get(&url)
        ).await;

        match result {
            Ok(Ok(response)) => Ok(response.status().as_u16() == status_ok),
            Ok(Err(_)) => Ok(false),
            Err(_) => Ok(false),
        }
    }
}
```

**D2. Wire HealthChecker into reconciler**

File: `crates/shellwego-agent/src/reconciler.rs`

Replace the empty `health_check_loop` (lines 282-290):

```rust
/// Health check all running VMs
pub async fn health_check_loop(&self) -> anyhow::Result<()> {
    let vms = self.vmm.list_running().await?;
    for vm in vms {
        // Get guest IP from the network setup
        let guest_ip = self.network.get_vm_ip(&vm.app_id).await
            .unwrap_or_else(|_| "10.0.0.1".to_string());

        let status = self.health_checker.check_vm(vm.app_id, &guest_ip).await;

        if !status.healthy && status.status == HealthState::Unhealthy {
            error!(
                "VM {} is unhealthy: {} (failures: {})",
                vm.app_id, status.detail, status.consecutive_failures
            );
            // Notify daemon to report to control plane
            // The daemon's heartbeat loop will pick up the health status
        }
    }
    Ok(())
}
```

Add `health_checker: HealthChecker` field to `Reconciler`. Register VMs in `create_microvm()` after successful start, unregister in `stop()`.

**D3. Report health in heartbeat**

File: `crates/shellwego-agent/src/daemon.rs`

Add `health_checker: Arc<HealthChecker>` field to `Daemon`. In `heartbeat_loop()`, include per-VM health:

```rust
let vm_health = self.health_checker.get_all_statuses().await;
// Convert to VmHealthEntry for QUIC message
let health_entries: Vec<shellwego_schema::VmHealthEntry> = vm_health.iter().map(|h| {
    shellwego_schema::VmHealthEntry {
        app_id: h.app_id,
        healthy: h.healthy,
        status: format!("{:?}", h.status),
        last_check: h.last_check,
    }
}).collect();
```

### Phase E: eBPF Network Isolation Attachment

**E1. Wire EbpfManager into reconciler**

File: `crates/shellwego-agent/src/reconciler.rs`

Add `ebpf_manager: shellwego_network::ebpf::EbpfManager` field to `Reconciler`.

In `Reconciler::new()`, initialize:
```rust
let ebpf_manager = shellwego_network::ebpf::EbpfManager::new().await
    .map_err(|e| anyhow::anyhow!("Failed to create eBPF manager: {}", e))?;
```

**E2. Attach eBPF on VM creation**

File: `crates/shellwego-agent/src/reconciler.rs` in `create_microvm()`

After network setup (`self.network.setup(...)`) and before `self.vmm.start(config)`:

```rust
// Attach eBPF firewall to the TAP interface
let tap_iface = &net_setup.tap_device;
if let Err(e) = self.ebpf_manager.attach_firewall(tap_iface).await {
    warn!("Failed to attach eBPF firewall to {}: {} (falling back to no filtering)", tap_iface, e);
} else {
    info!("Attached eBPF firewall to {}", tap_iface);
}

// Apply egress QoS if bandwidth limit is configured
if let Some(limit_mbps) = config.bandwidth_limit_mbps {
    if let Err(e) = self.ebpf_manager.apply_qos(tap_iface, limit_mbps).await {
        warn!("Failed to apply eBPF QoS to {}: {} (falling back)", tap_iface, e);
    } else {
        info!("Applied eBPF QoS {} Mbps to {}", limit_mbps, tap_iface);
    }
}
```

**E3. Detach eBPF on VM removal**

File: `crates/shellwego-agent/src/reconciler.rs` in `stop` handling

When a VM is stopped and its TAP device is cleaned up by the network module, the eBPF programs attached to that interface will be automatically cleaned up by the kernel (XDP programs are removed when the interface is deleted). No explicit detach is needed, but for safety, `EbpfManager::detach_all()` can be called on agent shutdown.

File: `crates/shellwego-agent/src/main.rs`

Add to shutdown sequence:
```rust
// Detach eBPF programs on shutdown
if let Err(e) = ebpf_manager.detach_all().await {
    error!("Failed to detach eBPF programs: {}", e);
}
```

**E4. Implement `reconcile_network_policies`**

File: `crates/shellwego-agent/src/reconciler.rs`

Replace the empty `reconcile_network_policies` (lines 272-279):

```rust
/// Sync network policies — attach/detach eBPF programs per VM
pub async fn reconcile_network_policies(&self, apps: &[DesiredApp]) -> anyhow::Result<()> {
    let active_vms = self.vmm.list_running().await?;
    let active_ids: std::collections::HashSet<_> = active_vms.iter().map(|v| v.app_id).collect();

    for app in apps {
        if !active_ids.contains(&app.app_id) {
            continue; // No running VM for this app, skip
        }

        // Get the TAP interface name for this VM
        let tap_name = format!("tap-{}", &app.app_id.to_string()[..8]);

        // Ensure firewall is attached
        if let Err(e) = self.ebpf_manager.attach_firewall(&tap_name).await {
            debug!("eBPF firewall not attached to {} (may be in fallback mode): {}", tap_name, e);
        }
    }
    Ok(())
}
```

### Phase W: Wiring in main.rs

**W1. Instantiate new components in main**

File: `crates/shellwego-agent/src/main.rs`

```rust
use shellwego_agent::image::ImageService;
use shellwego_agent::health::{HealthChecker, HealthCheckConfig, HealthCheckType};

// ... existing setup ...

// Create image service
let cache_dir = config.data_dir.join("images");
let image_service = ImageService::new(cache_dir)
    .context("Failed to create image service")?;

// Create health checker
let health_checker = Arc::new(HealthChecker::new(HealthCheckConfig {
    interval_secs: 10,
    timeout_secs: 3,
    failure_threshold: 3,
    startup_grace_secs: 30,
    check_type: HealthCheckType::TcpPort(8080),
}));

// Create eBPF manager
let ebpf_manager = shellwego_network::ebpf::EbpfManager::new().await
    .unwrap_or_else(|e| {
        warn!("eBPF manager failed to initialize: {} (running without eBPF)", e);
        // Create a no-op fallback
        shellwego_network::ebpf::EbpfManager::new_fallback()
    });

// Pass to daemon
let daemon = Daemon::new(
    config.clone(),
    capabilities,
    vmm.clone(),
    metrics.clone(),
    health_checker.clone(),
).await?;

// Pass to reconciler
let reconciler = Reconciler::new(
    vmm.clone(),
    network,
    daemon.state_client(),
    image_service,
    health_checker.clone(),
    ebpf_manager,
);
```

**W2. Register `image` and `health` modules**

File: `crates/shellwego-agent/src/lib.rs`

Add:
```rust
pub mod health;
pub mod image;
```

## 6. Dependencies on Other Plans

| Plan ID | Dependency | Notes |
|---|---|---|
| **Plan 03: QUIC Message Bus** | **STRONG** — Blocks Phase C | This plan adds new `Message` variants (`DesiredStatePush`, `VmHealthReport`, `ImagePullProgress`). If Plan 03 restructures the `Message` enum, adds multiplexed streams, or changes the `QuinnClient`/`QuinnServer` API, this plan must align. Specifically, if Plan 03 adds request-response correlation (request IDs), this plan's `ActionResponse` usage must conform. |
| **Plan 01: Security Hardening** | WEAK — Phase B may need registry auth from secrets | If Plan 01 adds real secret encryption, the `SHELLWEGO_REGISTRY_PASS` env var approach in Phase B should be replaced with pulling the registry credential from the KMS-backed secrets store. Not a blocker; can use env vars initially. |
| **Plan 02: Schema Consolidation** (if exists) | WEAK — Phase C modifies `Message` enum | If schema types are moved/reorganized, the `Message` enum changes in Phase C must target the correct location. The `DesiredAppMessage` and `VmHealthEntry` structs should go in `shellwego-schema` if Plan 02 hasn't been executed yet. |

**Execution order**: Plan 03 → Plan 04 (phases can overlap once Plan 03's `Message` enum changes land).

## 7. Acceptance Criteria

### Build
- [ ] `cargo build -p shellwego-agent` passes with 0 errors
- [ ] `cargo build -p shellwego-registry` passes with 0 errors
- [ ] All existing unit tests pass: `cargo test -p shellwego-agent`
- [ ] No new warnings introduced (or warnings justified with `#[allow]`)

### Phase A: Process Lifecycle
- [ ] `VmmManager::start_firecracker_vm` spawns with jailer when `use_jailer=true`
- [ ] `ProcessMonitor` detects child exit within 2 seconds
- [ ] Zombie processes are reaped (no `<defunct>` entries in `ps`)
- [ ] `MetricsCollector::set_microvm_count` is called on start/stop
- [ ] Unit test: `ProcessMonitor` registers child, exits, event emitted

### Phase B: Image Pulling
- [ ] `ImageService::pull_or_cache("alpine:latest")` pulls from Docker Hub registry in integration test
- [ ] Second call with same image ref returns cached path (no network call)
- [ ] `Reconciler::create_microvm` uses `ImageService` instead of base.ext4 copy
- [ ] `check_image_updates` returns true when remote digest differs (with mock)
- [ ] Unit test: `ImageService` creation with missing cache dir auto-creates it

### Phase C: QUIC Pipeline
- [ ] `Message::ScheduleApp` with full spec (env, volumes, command) is deserialized correctly
- [ ] `Daemon::command_consumer` creates `DesiredApp` with correct memory/CPU from `ResourceLimits`
- [ ] `Message::ActionResponse` is sent after every command (success or failure)
- [ ] `Message::VmHealthReport` is included in heartbeat payload
- [ ] `Message` round-trip test: serialize → deserialize → assert equality

### Phase D: Health Check
- [ ] `HealthChecker` marks VM as `Unhealthy` after 3 consecutive TCP probe failures
- [ ] `HealthChecker` respects startup grace period (30s) — probes during grace return `Healthy`
- [ ] `HealthChecker::mark_dead` sets status to `Dead` immediately
- [ ] Per-VM health status appears in Prometheus metrics (`shellwego_agent_vm_health_status`)
- [ ] Unit test: TCP probe against closed port returns `false`
- [ ] Unit test: TCP probe against open port returns `true`

### Phase E: eBPF
- [ ] `EbpfManager::attach_firewall("tap-xxx")` succeeds without panic (fallback mode)
- [ ] `EbpfManager::apply_qos("tap-xxx", 100)` succeeds without panic (fallback mode)
- [ ] eBPF attachment happens during `create_microvm` (logged)
- [ ] Agent shutdown calls `detach_all()` without error

### Integration
- [ ] End-to-end: start agent with `SHELLWEGO_FORCE_MODE=wasm` → agent registers → heartbeat sends → health checker runs
- [ ] `cargo clippy -p shellwego-agent` has 0 errors

## 8. Estimated Complexity

**XL** (Extra Large)

Rationale:
- **Phase F (Compile fix)**: ~50 lines changed. Low complexity but required first.
- **Phase A (Process lifecycle)**: ~200 lines new (`monitor.rs`) + ~80 lines modified (`mod.rs`, `driver.rs`). Medium complexity — process supervision is tricky (race conditions between spawn and register, signal handling).
- **Phase B (Image pulling)**: ~250 lines new (`image.rs`) + ~30 lines modified (`reconciler.rs`, `Cargo.toml`). Medium-high complexity — OCI-to-ext4 conversion requires `mkfs.ext4` + `mount` which need root/CAP_SYS_ADMIN. Caching logic has concurrency considerations.
- **Phase C (QUIC pipeline)**: ~100 lines modified in `daemon.rs` + ~80 lines new types in `quinn.rs`. Medium complexity — message parsing and state management.
- **Phase D (Health check)**: ~200 lines new (`health.rs`) + ~60 lines modified (`reconciler.rs`, `daemon.rs`, `main.rs`). Medium complexity — configurable probe types, grace period logic, status tracking.
- **Phase E (eBPF)**: ~40 lines modified across `reconciler.rs`, `main.rs`. Low complexity — mostly wiring existing `EbpfManager` calls.
- **Phase W (main.rs wiring)**: ~50 lines modified. Low complexity.

**Total: ~860 lines of production code + ~200 lines of test code across 12 files (4 new, 8 modified).**

## 9. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **OCI-to-ext4 requires root/CAP_SYS_ADMIN** — `mkfs.ext4` and `mount` need elevated privileges | High | High — agent cannot pull images in containers or unprivileged mode | Support multiple backends: (1) ext4 with root, (2) tmpfs overlay without root, (3) pass-through of pre-built images. Gate behind feature flag `oci-rootfs`. Document root requirement. |
| **Process monitor race with spawn** — child exits before `register()` is called | Medium | Medium — zombie process leaks | Use `try_wait()` in monitor loop; add a small delay in `register()` path; alternatively, spawn monitor task first and register synchronously before `cmd.spawn()` |
| **Jailer socket path mismatch** — jailer changes the API socket location; driver expects original path | Medium | High — all API calls to VM fail | Use `JailerConfig::socket_path()` for the actual path. Add integration test verifying socket appears at jailer-adjusted path. Log both expected and actual paths on failure. |
| **Plan 03 `Message` enum changes conflict** — Phase C adds variants that Plan 03 may restructure | Medium | Medium — merge conflict on shared enum | Coordinate Phase C with Plan 03. If Plan 03 is not yet landed, add variants conditionally behind a feature flag. Pin the `Message` enum shape in a shared document. |
| **eBPF aya version incompatibility** — aya requires specific kernel headers and BTF | Low | Low — falls back to no-op mode | `EbpfManager` already has fallback mode. Log clearly that eBPF is not active. CI tests run in fallback mode. |
| **Registry auth complexity** — Docker Hub, GHCR, ECR all have different auth flows | Medium | Medium — image pull fails for private registries | Phase B uses env vars initially. Future: integrate with KMS-backed secrets (Plan 01). `shellwego-registry::ImagePuller` already handles Docker Hub token flow; test against GHCR and ECR in integration tests. |
| **Health check false positives** — VM takes >30s to boot, health check marks it unhealthy | Medium | Low — unnecessary restart storm | Configurable `startup_grace_secs` (default 30s). For slow-booting images, increase via agent config. Log warning before marking unhealthy. |
| **Pulling large images blocks reconciler** — image pull can take minutes, blocking reconciliation for other apps | Medium | Medium — other app deployments delayed | Spawn image pull in a background task; only create VM once pull completes. Add pull queue with cancellation support. |
