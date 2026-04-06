# Plan 10: Storage Volume Provisioning & Encryption

## 1. Title & Overview

**Storage Volume Provisioning & Encryption** — Close the gap between the volume entity types in `shellwego-schema` and actual on-disk storage operations. Today the control plane creates volume records in SQLite but never calls `shellwego-storage`; the agent reconciler does `tokio::fs::create_dir_all()` instead of using `ZfsManager`; the encryption module wraps AES-256-GCM keys but never integrates with ZFS native encryption or LUKS2; compression ratio is queried from ZFS but never surfaced to callers or persisted; and the snapshot manager in the agent has its own inline ZFS commands instead of using the shared `ZfsCli`. This plan wires all of these components together into a working provisioning pipeline: control plane → agent → storage crate → ZFS on disk.

## 2. Gap Summary

| # | Readme/Schema Claim | Actual Implementation | File(s) | Severity |
|---|---|---|---|---|
| A | `VolumeStatus::Creating` → `Attached` lifecycle | Control plane creates DB record, never calls `shellwego-storage` crate. Volume exists in DB only. | `crates/shellwego-control-plane/src/api/handlers.rs` (volume handlers), `crates/shellwego-control-plane/Cargo.toml` (no `shellwego-storage` dep) | **CRITICAL** |
| B | "ZFS-backed storage for application data" (volume.rs doc comment) | Agent `reconciler.rs` line 261: `tokio::fs::create_dir_all(host_path)` — no ZFS, no quota, no dataset. | `crates/shellwego-agent/src/reconciler.rs` line 258-269 | **CRITICAL** |
| C | "Encryption at rest using AES-256-GCM" (encryption.rs doc comment) | `EncryptionProvider` implements key wrapping (DEK/KEK envelope) but is never called from volume provisioning flow. `volume.encrypted: bool` field in schema is never checked. | `crates/shellwego-storage/src/encryption.rs`, `crates/shellwego-schema/src/entities/volume.rs` line 111 | **HIGH** |
| D | ZFS native encryption / LUKS2 | Not implemented at all. Only application-level key wrapping exists. No `zfs create -o encryption=...` or `cryptsetup luksFormat`. | `crates/shellwego-storage/src/encryption.rs` (no ZFS integration), no LUKS module | **HIGH** |
| E | `compression_ratio` tracking | `VolumeInfo.compression_ratio` is populated by `ZfsCli::get_info()` via `zfs list -o compressratio` (works), but `S3Backend` hardcodes `1.0`, and no code pushes the value to `volume.used_gb` in the DB entity. | `crates/shellwego-storage/src/lib.rs` line 67, `crates/shellwego-storage/src/s3.rs` lines 134,238,268 | **MEDIUM** |
| F | Agent snapshot manager has duplicate ZFS commands | `crates/shellwego-agent/src/snapshot.rs` defines its own `ZfsSnapshotManager` with inline `tokio::process::Command::new("zfs")` calls instead of using `shellwego_storage::ZfsCli`. | `crates/shellwego-agent/src/snapshot.rs` lines 40-185 | **MEDIUM** |
| G | CLI `volumes` commands are stubs | `crates/shellwego-cli/src/commands/volumes.rs` prints strings, never calls the control plane API client. | `crates/shellwego-cli/src/commands/volumes.rs` lines 28-41 | **MEDIUM** |
| H | `BackupService` uses in-memory `HashMap` | `crates/shellwego-control-plane/src/services/backup.rs` stores backups in `Arc<RwLock<HashMap>>`, no real ZFS snapshot or S3 upload. | `crates/shellwego-control-plane/src/services/backup.rs` line 116 | **LOW** |

## 3. Scope

### Files to Modify

| File | Change |
|---|---|
| `crates/shellwego-storage/src/lib.rs` | Add `VolumeProvisioner` struct that orchestrates create/snapshot/destroy with encryption; add `StorageError::Encryption` variant; add `compression_ratio` to `VolumeInfo` serialization helpers |
| `crates/shellwego-storage/src/zfs/mod.rs` | Add `create_zvol()` for block device volumes; add `set_encryption()` for ZFS native encryption; add `get_compression_ratio()` that returns parsed float; wire `EncryptionProvider` into `ZfsManager` |
| `crates/shellwego-storage/src/zfs/cli.rs` | Add `create_zvol()` CLI command; add `set_encryption()` CLI command; add `get_compression_ratio()`; add `list_volumes()` with `shellwego/volumes` prefix filter; add command timeout (30s default); add structured error code parsing from stderr |
| `crates/shellwego-storage/src/encryption.rs` | Add `VolumeEncryptor` struct that handles ZFS native encryption key management; add `EncryptionProvider::encrypt_volume_key()` and `decrypt_volume_key()` that work with ZFS `keyformat=raw`; add `EncryptionStatus` enum (Unencrypted, ZfsNative, LUKS2, ApplicationLevel) |
| `crates/shellwego-storage/src/s3.rs` | Fix `compression_ratio` — derive from object metadata or content-encoding header; fix `created` field to use object Last-Modified instead of `Utc::now()` |
| `crates/shellwego-control-plane/Cargo.toml` | Add `shellwego-storage` dependency |
| `crates/shellwego-control-plane/src/state.rs` | Add `storage: Arc<VolumeProvisioner>` to `AppState` (optional, for control-plane-local operations) |
| `crates/shellwego-control-plane/src/api/handlers.rs` | Wire volume handlers to actually call storage operations (or delegate to agent via gRPC); update `create_volume` to set `VolumeStatus::Creating` and emit provisioning event |
| `crates/shellwego-control-plane/src/services/backup.rs` | Replace `HashMap` storage with actual `ZfsManager::snapshot_volume()` calls; integrate with S3 backend for offsite |
| `crates/shellwego-agent/src/reconciler.rs` | Replace `tokio::fs::create_dir_all()` with `ZfsManager::create_volume()` in `reconcile_volumes()`; add `ZfsManager` field to `Reconciler` |
| `crates/shellwego-agent/src/snapshot.rs` | Replace inline `ZfsSnapshotManager` with `shellwego_storage::ZfsCli` and `ZfsManager`; remove duplicate `tokio::process::Command::new("zfs")` calls |
| `crates/shellwego-cli/src/commands/volumes.rs` | Wire all 7 subcommands to the API client (`list`, `create`, `get`, `delete`, `attach`, `detach`, `snapshot`) |
| `crates/shellwego-schema/src/entities/volume.rs` | Add `compression_ratio: Option<f64>` field to `Model`; add `encryption_status` field; add `VolumeProvisioningError` type |

### New Files to Create

| File | Purpose |
|---|---|
| `crates/shellwego-storage/src/provisioner.rs` | `VolumeProvisioner` — high-level orchestration: create encrypted volume, set quota, snapshot, clone, destroy. Integrates `ZfsManager` + `EncryptionProvider`. State machine for volume lifecycle. |
| `crates/shellwego-storage/src/metrics.rs` | Storage metrics collector: poll ZFS pool metrics + volume compression ratios periodically; expose as `prometheus`-compatible gauges; emit to tracing spans. |
| `crates/shellwego-storage/src/luks.rs` | LUKS2 encryption module (stub with `cryptsetup` CLI wrapper): `create_encrypted_device()`, `open_device()`, `close_device()`. Optional feature-gated behind `luks2` feature flag. |

## 4. Prerequisites

1. **Build passes** — `shellwego-storage` currently builds clean (0 errors, 0 warnings). All changes must maintain this. The agent and control plane also compile.

2. **ZFS available on agent nodes** — The ZFS CLI wrapper (`ZfsCli`) already handles the case where ZFS is not installed (`check_prereqs`). All new ZFS operations must be guarded behind availability checks. On developer machines without ZFS, provisioning should degrade gracefully (e.g., fall back to directory-based volumes with a warning).

3. **No dependency on live control plane DB** — Unit tests must work without SQLite. Use the existing `tempfile` and in-memory patterns. Integration tests that require ZFS should be feature-gated behind `#[cfg(feature = "zfs-integration")]`.

4. **Encryption key material** — The `EncryptionProvider` already requires a hex-encoded 32-byte master key. Volume provisioning must fail gracefully if the encryption config is missing but a volume is requested as encrypted. No key material should be logged.

5. **Agent depends on storage crate** — `shellwego-agent/Cargo.toml` already includes `shellwego-storage = { path = "../shellwego-storage" }`. No new dependency needed for the agent.

## 5. Detailed Implementation Steps

### Phase A: VolumeProvisioner — Orchestration Layer

**A1. Create provisioner module**

File: `crates/shellwego-storage/src/provisioner.rs`

```rust
use crate::encryption::{EncryptionConfig, EncryptionProvider, EncryptionStatus};
use crate::zfs::ZfsManager;
use crate::{StorageError, VolumeInfo, SnapshotInfo};
use std::sync::Arc;
use uuid::Uuid;

/// High-level volume provisioning orchestrator
pub struct VolumeProvisioner {
    zfs: Arc<ZfsManager>,
    encryption: Option<Arc<EncryptionProvider>>,
}

#[derive(Debug, Clone)]
pub struct ProvisionVolumeRequest {
    pub volume_id: Uuid,
    pub size_gb: u64,
    pub encrypted: bool,
    pub compression: Option<String>,  // e.g. "zstd-3", "lz4", "off"
    pub volume_type: ProvisionVolumeType,
}

#[derive(Debug, Clone, Copy)]
pub enum ProvisionVolumeType {
    Dataset,  // ZFS filesystem dataset (mounted directory)
    Zvol,     // ZFS block device volume (for raw block / Firecracker rootfs)
}

#[derive(Debug, Clone)]
pub struct ProvisionedVolume {
    pub info: VolumeInfo,
    pub encryption_status: EncryptionStatus,
    pub dataset_path: String,
    pub mountpoint: Option<std::path::PathBuf>,
}
```

Implement methods:
- `VolumeProvisioner::new(zfs: Arc<ZfsManager>, encryption: Option<Arc<EncryptionProvider>>)` — constructor
- `async fn provision(&self, req: ProvisionVolumeRequest) -> Result<ProvisionedVolume, StorageError>` — full provisioning flow
- `async fn destroy(&self, volume_id: Uuid) -> Result<(), StorageError>` — destroy with snapshots
- `async fn snapshot(&self, volume_id: Uuid, snap_name: &str) -> Result<SnapshotInfo, StorageError>`
- `async fn clone(&self, volume_id: Uuid, snap_name: &str, target_volume_id: Uuid) -> Result<ProvisionedVolume, StorageError>`
- `async fn get_status(&self, volume_id: Uuid) -> Result<ProvisionedVolume, StorageError>`

The `provision()` method:
1. Call `self.zfs.create_volume(volume_id, size_gb)` (existing method)
2. If `req.encrypted`, call `self.zfs.set_encryption(volume_id, &encryption_config)` (new, see Phase C)
3. If `req.compression` is set, call `self.zfs.set_property(...)` with the compression algorithm
4. If `req.volume_type == Zvol`, call `self.zfs.create_zvol(volume_id, size_gb)` instead
5. Query `VolumeInfo` via `self.zfs.get_info_cached()`
6. Return `ProvisionedVolume` with encryption status and compression ratio populated

**A2. Register module in lib.rs**

File: `crates/shellwego-storage/src/lib.rs`

Add:
```rust
pub mod provisioner;
pub mod metrics;
pub use provisioner::VolumeProvisioner;
```

**A3. Add `VolumeProvisioner` tests**

Add unit tests with mock ZFS (using a trait object for `ZfsManager` or feature-gating):
- `test_provision_dataset` — provision a dataset volume, verify it returns with mountpoint
- `test_provision_encrypted` — provision with encryption=true, verify encryption status is set
- `test_provision_zvol` — provision a block device volume
- `test_destroy_with_snapshots` — destroy a volume that has snapshots

### Phase B: ZFS CLI Hardening — zvols, Error Parsing, Timeouts

**B1. Add `create_zvol()` to `ZfsCli`**

File: `crates/shellwego-storage/src/zfs/cli.rs`

Add method after `create_dataset()`:
```rust
/// Create a ZFS block volume (zvol) for raw block device access
pub async fn create_zvol(
    &self,
    name: &str,
    size_gb: u64,
) -> Result<(), StorageError> {
    let mut cmd = Command::new("zfs");
    cmd.args([
        "create",
        "-V", &format!("{}G", size_gb),
        "-b", "4k",           // Block size optimized for databases
        name,
    ]);

    let output = cmd.output().await?;
    self.check_output(output, &format!("create zvol {}", name))
}
```

**B2. Add command timeout to `ZfsCli`**

Modify `ZfsCli` struct to hold a default timeout:
```rust
#[derive(Clone)]
pub struct ZfsCli {
    timeout: Duration,
}

impl ZfsCli {
    pub fn new() -> Self {
        Self { timeout: Duration::from_secs(30) }
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }
}
```

Update all `Command::new(...)` calls to add `.kill_on_drop(true)` and use `tokio::time::timeout()`:
```rust
let output = tokio::time::timeout(self.timeout, cmd.output()).await
    .map_err(|_| StorageError::ZfsCommand(format!("{} timed out after {:?}", context, self.timeout)))?
    .map_err(|e| StorageError::Io(e))?;
```

This affects ~15 call sites in `cli.rs`. Each `.output().await?` call needs wrapping.

**B3. Add structured error parsing to `check_output()`**

File: `crates/shellwego-storage/src/zfs/cli.rs`

Replace the current `check_output`:
```rust
fn check_output(&self, output: std::process::Output, context: &str) -> Result<(), StorageError> {
    if output.status.success() {
        trace!("zfs {} succeeded", context);
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Parse known ZFS error codes
        let error = match stderr.as_ref() {
            s if s.contains("dataset does not exist") => StorageError::NotFound(context.to_string()),
            s if s.contains("dataset already exists") => StorageError::AlreadyExists(context.to_string()),
            s if s.contains("permission denied") => StorageError::PermissionDenied(context.to_string()),
            s if s.contains("invalid property") || s.contains("invalid argument") => {
                StorageError::InvalidName(context.to_string())
            }
            s if s.contains("no space left") || s.contains("out of space") => {
                StorageError::InsufficientSpace { needed: 0, available: 0 }
            }
            s if s.contains("snapshot already exists") => {
                StorageError::SnapshotNotFound(context.to_string())
            }
            _ => StorageError::ZfsCommand(format!("{}: {}", context, stderr.trim())),
        };
        error!("zfs {} failed: {}", context, stderr.trim());
        Err(error)
    }
}
```

**B4. Add `list_volumes()` with prefix filter**

File: `crates/shellwego-storage/src/zfs/cli.rs`

```rust
pub async fn list_volumes(
    &self,
    base: &str,
) -> Result<Vec<VolumeInfo>, StorageError> {
    let output = Command::new("zfs")
        .args([
            "list", "-H", "-p", "-r",
            "-o", "name,used,available,referenced,compressratio,mountpoint,creation",
            "-t", "filesystem,volume",
            base,
        ])
        .output().await?;

    if !output.status.success() {
        return Err(StorageError::NotFound(base.to_string()));
    }

    let mut volumes = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let parts: Vec<&str> = line.trim().split('\t').collect();
        if parts.len() < 7 { continue; }
        // ... parse same as get_info, skip the base dataset itself
        if parts[0] == base { continue; }
        volumes.push(VolumeInfo { /* same fields as get_info */ });
    }
    Ok(volumes)
}
```

**B5. Add `get_compression_ratio()` explicit method**

File: `crates/shellwego-storage/src/zfs/cli.rs`

```rust
pub async fn get_compression_ratio(&self, dataset: &str) -> Result<f64, StorageError> {
    let raw = self.get_property(dataset, "compressratio").await?;
    // ZFS reports compressratio as "1.00x" string
    let ratio: f64 = raw.trim_end_matches('x').parse()
        .map_err(|e| StorageError::Parse(format!("Invalid compressratio '{}': {}", raw, e)))?;
    Ok(ratio)
}
```

**B6. Update `ZfsManager` to expose new methods**

File: `crates/shellwego-storage/src/zfs/mod.rs`

Add:
- `pub async fn create_zvol(&self, volume_id: Uuid, size_gb: u64) -> Result<VolumeInfo, StorageError>`
- `pub async fn list_volumes(&self) -> Result<Vec<VolumeInfo>, StorageError>` — delegates to `self.cli.list_volumes(&self.base_dataset)`
- `pub async fn get_compression_ratio(&self, volume_id: Uuid) -> Result<f64, StorageError>`

### Phase C: Encryption Integration — ZFS Native + Application-Level

**C1. Add `EncryptionStatus` enum**

File: `crates/shellwego-storage/src/encryption.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EncryptionStatus {
    /// No encryption applied
    Unencrypted,
    /// ZFS native encryption (encryption=on, keyformat=raw/passphrase)
    ZfsNative,
    /// LUKS2 container (cryptsetup)
    Luks2,
    /// Application-level envelope encryption (DEK/KEK)
    ApplicationLevel,
}
```

**C2. Add ZFS native encryption support to `ZfsCli`**

File: `crates/shellwego-storage/src/zfs/cli.rs`

```rust
/// Enable ZFS native encryption on a dataset
pub async fn enable_encryption(
    &self,
    dataset: &str,
    keyformat: &str,   // "raw" or "passphrase"
    keylocation: &str, // "prompt" or "file:///path/to/key"
) -> Result<(), StorageError> {
    let output = Command::new("zfs")
        .args([
            "create",
            "-o", &format!("encryption=on"),
            "-o", &format!("keyformat={}", keyformat),
            "-o", &format!("keylocation={}", keylocation),
            "-o", "encryptionroot=off",  // Each volume manages its own key
            dataset,
        ])
        .output().await?;
    self.check_output(output, &format!("enable encryption on {}", dataset))
}

/// Load encryption key for a ZFS dataset
pub async fn load_key(
    &self,
    dataset: &str,
    keyfile_path: &str,
) -> Result<(), StorageError> {
    let output = Command::new("zfs")
        .args(["load-key", "-L", &format!("file://{}", keyfile_path), dataset])
        .output().await?;
    self.check_output(output, &format!("load key for {}", dataset))
}

/// Unload encryption key for a ZFS dataset
pub async fn unload_key(&self, dataset: &str) -> Result<(), StorageError> {
    let output = Command::new("zfs")
        .args(["unload-key", dataset])
        .output().await?;
    self.check_output(output, &format!("unload key for {}", dataset))
}

/// Check if a dataset has encryption enabled
pub async fn is_encrypted(&self, dataset: &str) -> Result<bool, StorageError> {
    match self.get_property(dataset, "encryption").await {
        Ok(val) => Ok(val == "on" || val == "aes-256-gcm" || val == "aes-128-gcm"),
        Err(_) => Ok(false),
    }
}
```

**C3. Add `VolumeEncryptor`**

File: `crates/shellwego-storage/src/encryption.rs`

```rust
/// Manages volume-level encryption operations
pub struct VolumeEncryptor {
    provider: Arc<EncryptionProvider>,
    zfs: Arc<ZfsManager>,
    keys_dir: std::path::PathBuf,
}

impl VolumeEncryptor {
    pub fn new(
        provider: Arc<EncryptionProvider>,
        zfs: Arc<ZfsManager>,
        keys_dir: std::path::PathBuf,
    ) -> Self {
        Self { provider, zfs, keys_dir }
    }

    /// Encrypt a volume using ZFS native encryption
    /// Steps:
    /// 1. Generate a random raw key (32 bytes)
    /// 2. Encrypt (wrap) the raw key with the master key via EncryptionProvider
    /// 3. Store the wrapped key in keys_dir/<volume_id>.wrapped
    /// 4. Write the unwrapped raw key to a temp file
    /// 5. Call `zfs load-key -L file:///tmp/keyfile <dataset>`
    /// 6. Secure-delete the temp key file
    pub async fn encrypt_volume(
        &self,
        volume_id: Uuid,
    ) -> Result<EncryptionStatus, StorageError> {
        // 1. Generate raw ZFS encryption key
        let mut raw_key = vec![0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut raw_key);

        // 2. Wrap with master key
        let dek = self.provider.generate_dek().await
            .map_err(|e| StorageError::Backend(format!("DEK generation: {}", e)))?;
        let iv = self.provider.generate_iv(); // Need to make this pub or add encrypt_bytes
        // Actually, we encrypt the raw_key with a fresh DEK
        let wrapped = self.provider.encrypt_block(&raw_key, &raw_key, &iv)
            .map_err(|e| StorageError::Backend(format!("Key wrap: {}", e)))?;

        // 3. Store wrapped key
        tokio::fs::create_dir_all(&self.keys_dir).await?;
        let wrapped_path = self.keys_dir.join(format!("{}.wrapped", volume_id));
        let encoded = dek.to_base64(); // Stores ciphertext + iv + master_key_id
        tokio::fs::write(&wrapped_path, &encoded).await?;

        // 4. Write raw key to temp file
        let temp_key_path = self.keys_dir.join(format!("{}.raw.tmp", volume_id));
        tokio::fs::write(&temp_key_path, &raw_key).await?;

        // 5. Load key into ZFS
        let dataset = self.zfs.full_path(&format!("volumes/{}", volume_id));
        self.zfs.cli().load_key(&dataset, temp_key_path.to_str().unwrap()).await?;

        // 6. Secure-delete temp key
        // Overwrite with zeros before deleting
        tokio::fs::write(&temp_key_path, vec![0u8; 32]).await?;
        tokio::fs::remove_file(&temp_key_path).await?;

        Ok(EncryptionStatus::ZfsNative)
    }

    /// Decrypt (load key) for an existing encrypted volume
    pub async fn unlock_volume(&self, volume_id: Uuid) -> Result<(), StorageError> {
        let wrapped_path = self.keys_dir.join(format!("{}.wrapped", volume_id));
        let encoded = tokio::fs::read_to_string(&wrapped_path).await
            .map_err(|_| StorageError::NotFound(format!("wrapped key for {}", volume_id)))?;

        let dek = DataKey::from_base64(&encoded)
            .map_err(|e| StorageError::Backend(format!("DEK parse: {}", e)))?;

        let raw_key = self.provider.decrypt_dek(&dek.ciphertext, &dek.iv).await
            .map_err(|e| StorageError::Backend(format!("Key unwrap: {}", e)))?;

        // Write to temp file, load, delete
        let temp_key_path = self.keys_dir.join(format!("{}.raw.tmp", volume_id));
        tokio::fs::write(&temp_key_path, &raw_key).await?;

        let dataset = self.zfs.full_path(&format!("volumes/{}", volume_id));
        let result = self.zfs.cli().load_key(&dataset, temp_key_path.to_str().unwrap()).await;

        tokio::fs::write(&temp_key_path, vec![0u8; 32]).await?;
        tokio::fs::remove_file(&temp_key_path).await?;

        result
    }

    /// Lock (unload key) for a volume
    pub async fn lock_volume(&self, volume_id: Uuid) -> Result<(), StorageError> {
        let dataset = self.zfs.full_path(&format!("volumes/{}", volume_id));
        self.zfs.cli().unload_key(&dataset).await
    }

    /// Check encryption status of a volume
    pub async fn get_encryption_status(&self, volume_id: Uuid) -> Result<EncryptionStatus, StorageError> {
        let dataset = self.zfs.full_path(&format!("volumes/{}", volume_id));
        if self.zfs.cli().is_encrypted(&dataset).await? {
            Ok(EncryptionStatus::ZfsNative)
        } else {
            Ok(EncryptionStatus::Unencrypted)
        }
    }
}
```

**C4. Make `generate_iv()` public on `EncryptionProvider`**

File: `crates/shellwego-storage/src/encryption.rs`

Change line 164:
```rust
fn generate_iv(&self) -> Vec<u8> {
```
to:
```rust
pub fn generate_iv(&self) -> Vec<u8> {
```

**C5. Add `cli()` accessor to `ZfsManager`**

File: `crates/shellwego-storage/src/zfs/mod.rs`

```rust
/// Access the underlying CLI (for direct operations)
pub fn cli(&self) -> &ZfsCli {
    &self.cli
}
```

**C6. Create LUKS2 module (stub, feature-gated)**

File: `crates/shellwego-storage/src/luks.rs`

```rust
//! LUKS2 disk encryption support (optional)
//!
//! Requires `cryptsetup` on the host and the `luks2` feature flag.
//! ZFS native encryption is preferred; LUKS2 is for zvols that
//! need cross-platform compatibility or specific cipher modes.

use crate::StorageError;
use std::path::PathBuf;
use tokio::process::Command;
use tracing::{debug, info};

pub struct Luks2Manager {
    cryptsetup_path: String,
}

impl Luks2Manager {
    pub fn new() -> Result<Self, StorageError> {
        // Check cryptsetup exists
        Ok(Self {
            cryptsetup_path: "cryptsetup".to_string(),
        })
    }

    /// Format a block device as LUKS2
    pub async fn luks_format(
        &self,
        device: &str,
        keyfile: &str,
        cipher: Option<&str>,  // e.g. "aes-xts-plain64"
    ) -> Result<(), StorageError> {
        let mut cmd = Command::new(&self.cryptsetup_path);
        cmd.args(["luksFormat", "--type", "luks2", "--batch-mode"]);

        if let Some(c) = cipher {
            cmd.args(["--cipher", c]);
        }

        cmd.args(["--key-file", keyfile, device]);

        let output = cmd.output().await?;
        if !output.status.success() {
            return Err(StorageError::ZfsCommand(format!(
                "luksFormat: {}", String::from_utf8_lossy(&output.stderr)
            )));
        }
        info!("Formatted {} as LUKS2", device);
        Ok(())
    }

    /// Open a LUKS2 device
    pub async fn luks_open(
        &self,
        device: &str,
        mapper_name: &str,
        keyfile: &str,
    ) -> Result<(), StorageError> {
        let output = Command::new(&self.cryptsetup_path)
            .args(["open", "--type", "luks2", "--key-file", keyfile, device, mapper_name])
            .output().await?;

        if !output.status.success() {
            return Err(StorageError::ZfsCommand(format!(
                "luksOpen: {}", String::from_utf8_lossy(&output.stderr)
            )));
        }
        debug!("Opened LUKS2 device {} as {}", device, mapper_name);
        Ok(())
    }

    /// Close a LUKS2 device
    pub async fn luks_close(&self, mapper_name: &str) -> Result<(), StorageError> {
        let output = Command::new(&self.cryptsetup_path)
            .args(["close", mapper_name])
            .output().await?;

        if !output.status.success() {
            return Err(StorageError::ZfsCommand(format!(
                "luksClose: {}", String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }
}
```

**C7. Add `luks2` feature flag**

File: `crates/shellwego-storage/Cargo.toml`

```toml
[features]
default = []
luks2 = []

[dependencies]
# ... existing deps ...

# Optional: for secure memory zeroing in LUKS module
# zeroize = { version = "1.7", optional = true }
```

### Phase D: Wire Agent Reconciler to ZfsManager

**D1. Add `ZfsManager` to `Reconciler`**

File: `crates/shellwego-agent/src/reconciler.rs`

Add field:
```rust
use shellwego_storage::{ZfsManager, VolumeInfo};

#[derive(Clone)]
pub struct Reconciler {
    vmm: VmmManager,
    network: std::sync::Arc<CniNetwork>,
    state_client: StateClient,
    zfs: Option<ZfsManager>,  // Optional: falls back to directory-based volumes
}
```

Update `Reconciler::new()`:
```rust
pub fn new(
    vmm: VmmManager,
    network: std::sync::Arc<CniNetwork>,
    state_client: StateClient,
    zfs_pool: Option<&str>,
) -> Self {
    let zfs = zfs_pool.and_then(|pool| {
        tokio::runtime::Handle::current().block_on(async {
            ZfsManager::new(pool).await.ok()
        })
    });

    if zfs.is_some() {
        info!("ZFS storage backend initialized");
    } else {
        tracing::warn!("ZFS unavailable, falling back to directory-based volumes");
    }

    Self { vmm, network, state_client, zfs }
}
```

**D2. Replace `reconcile_volumes()`**

File: `crates/shellwego-agent/src/reconciler.rs`

Replace lines 258-269:
```rust
async fn reconcile_volumes(&self, apps: &[DesiredApp]) -> anyhow::Result<()> {
    for app in apps {
        for vol in &app.volumes {
            if let Some(ref zfs) = self.zfs {
                let volume_dataset = zfs.full_path(&format!("volumes/{}", vol.volume_id));

                if !zfs.cli().dataset_exists(&volume_dataset).await? {
                    info!("Provisioning ZFS volume for {}", vol.volume_id);
                    // Use a default 10GB if size not specified, otherwise derive from vol
                    zfs.create_volume(vol.volume_id, vol.size_gb.unwrap_or(10)).await?;
                }

                // Get mountpoint for drive config
                let info = zfs.cli().get_info(&volume_dataset).await?;
                if let Some(mountpoint) = info.mountpoint {
                    // Store mountpoint for drive attachment
                    vol.device = mountpoint.to_string_lossy().to_string();
                }
            } else {
                // Fallback: directory-based volumes (dev mode)
                let host_path = std::path::Path::new(&vol.device);
                if !host_path.exists() {
                    info!("Creating volume directory for {}", vol.volume_id);
                    tokio::fs::create_dir_all(host_path).await?;
                }
            }
        }
    }
    Ok(())
}
```

**D3. Update daemon to initialize `ZfsManager`**

File: `crates/shellwego-agent/src/daemon.rs`

Where `Reconciler::new()` is called, pass the ZFS pool from config:
```rust
let reconciler = Reconciler::new(
    vmm_manager,
    network,
    state_client,
    config.zfs_pool.as_deref(),  // New config field
)?;
```

**D4. Add `zfs_pool` to agent config**

File: `crates/shellwego-agent/src/daemon.rs` (or wherever agent config is read)

```rust
pub struct AgentConfig {
    // ... existing fields ...
    pub zfs_pool: Option<String>,  // ZFS pool name, e.g. "tank"
}
```

Read from env var `SHELLWEGO_ZFS_POOL`.

### Phase E: Replace Agent Snapshot Duplicate ZFS Code

**E1. Refactor `snapshot.rs` to use `ZfsCli`**

File: `crates/shellwego-agent/src/snapshot.rs`

Replace the entire `ZfsSnapshotManager` struct (lines 39-185) with:
```rust
use shellwego_storage::zfs::ZfsCli;

#[derive(Clone)]
struct ZfsSnapshotManager {
    pool: String,
    base_dataset: String,
    cli: ZfsCli,
}

impl ZfsSnapshotManager {
    async fn new(pool: &str) -> anyhow::Result<Self> {
        let cli = ZfsCli::new();
        let base_dataset = format!("{}/shellwego/snapshots", pool);
        Ok(Self {
            pool: pool.to_string(),
            base_dataset,
            cli,
        })
    }

    async fn is_zfs_available(&self) -> bool {
        self.cli.check_prereqs().await.is_ok()
    }

    async fn dataset_exists(&self, dataset: &str) -> anyhow::Result<bool> {
        self.cli.dataset_exists(dataset).await.map_err(Into::into)
    }

    async fn create_disk_snapshot(
        &self,
        app_id: Uuid,
        snapshot_name: &str,
    ) -> anyhow::Result<Option<String>> {
        if !self.is_zfs_available().await { return Ok(None); }

        let app_dataset = format!("{}/shellwego/apps/{}", self.pool, app_id);
        if !self.dataset_exists(&app_dataset).await? { return Ok(None); }

        let snapshot_full = format!("{}@{}", app_dataset, snapshot_name);
        self.cli.create_snapshot(&app_dataset, snapshot_name).await
            .map(|()| Some(snapshot_full))
            .or_else(|e| {
                tracing::warn!("Failed to create ZFS snapshot: {}", e);
                Ok(None)
            })
    }

    async fn restore_disk_snapshot(
        &self,
        snapshot_path: &str,
        new_app_id: Uuid,
    ) -> anyhow::Result<Option<String>> {
        if !self.is_zfs_available().await { return Ok(None); }

        let parts: Vec<&str> = snapshot_path.split('@').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid snapshot path: {}", snapshot_path);
        }

        let target_dataset = format!("{}/shellwego/apps/{}", self.pool, new_app_id);
        self.cli.clone_snapshot(snapshot_path, &target_dataset).await
            .map(|()| Some(target_dataset))
            .map_err(|e| anyhow::anyhow!("Failed to clone ZFS snapshot: {}", e))
    }

    async fn delete_disk_snapshot(&self, snapshot_path: &str) -> anyhow::Result<()> {
        if !self.is_zfs_available().await { return Ok(()); }
        self.cli.destroy_dataset(snapshot_path, false).await
            .map_err(|e| anyhow::anyhow!("Failed to delete ZFS snapshot: {}", e))
    }

    async fn get_snapshot_size(&self, snapshot_path: &str) -> anyhow::Result<u64> {
        if !self.is_zfs_available().await { return Ok(0); }

        let output = tokio::process::Command::new("zfs")
            .args(["list", "-H", "-p", "-o", "used", snapshot_path])
            .output().await?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().parse().unwrap_or(0))
        } else {
            Ok(0)
        }
    }
}
```

This eliminates ~100 lines of duplicate `tokio::process::Command::new("zfs")` calls.

### Phase F: Compression Ratio Tracking

**F1. Fix S3 backend `compression_ratio`**

File: `crates/shellwego-storage/src/s3.rs`

In `info()` method (line 250), use content-encoding metadata:
```rust
async fn info(&self, name: &str) -> Result<VolumeInfo, StorageError> {
    let key = self.object_key(name);
    let resp = self.client.head_object().bucket(&self.bucket).key(&key).send().await
        .map_err(|e| StorageError::Backend(format!("S3 head: {}", e)))?;

    // S3 objects don't have ZFS-style compression ratios.
    // Use content-encoding as a hint, but report 1.0 for object storage.
    let compression_ratio = match resp.content_encoding() {
        Some(enc) if enc.contains("gzip") || enc.contains("br") => {
            // Estimate: compressed size / original size is unknown for S3 head
            1.0
        }
        _ => 1.0,
    };

    let last_modified = resp.last_modified()
        .map(|t| chrono::DateTime::from_timestamp(t.secs(), 0).unwrap_or_else(chrono::Utc::now))
        .unwrap_or_else(chrono::Utc::now);

    Ok(VolumeInfo {
        name: name.to_string(),
        mountpoint: None,
        used_bytes: resp.content_length() as u64,
        available_bytes: 0,
        referenced_bytes: resp.content_length() as u64,
        compression_ratio,
        created: last_modified,  // Fix: was Utc::now()
        properties: std::collections::HashMap::new(),
    })
}
```

Apply the same fix to `list()` and `create()`.

**F2. Add compression ratio to volume schema entity**

File: `crates/shellwego-schema/src/entities/volume.rs`

Add field to `Model` struct:
```rust
#[serde(default)]
pub compression_ratio: Option<f64>,
```

**F3. Add periodic compression ratio sync to `VolumeProvisioner`**

File: `crates/shellwego-storage/src/provisioner.rs`

Add method:
```rust
/// Get current compression ratio for a volume from ZFS
pub async fn get_compression_ratio(&self, volume_id: Uuid) -> Result<f64, StorageError> {
    let dataset = self.zfs.full_path(&format!("volumes/{}", volume_id));
    self.zfs.cli().get_compression_ratio(&dataset).await
}
```

### Phase G: Wire Control Plane to Storage

**G1. Add `shellwego-storage` dependency**

File: `crates/shellwego-control-plane/Cargo.toml`

Add under `[dependencies]`:
```toml
shellwego-storage = { path = "../shellwego-storage" }
```

Note: The control plane itself should NOT directly provision volumes on disk. The control plane is the scheduler/decision-maker. Volume provisioning happens on the agent node where the volume will be used. The control plane should:
1. Create the volume record in DB with `VolumeStatus::Creating`
2. Send a provisioning command to the agent (via gRPC/QUIC message bus)
3. The agent provisions via `VolumeProvisioner` / `ZfsManager`
4. The agent reports back with `VolumeStatus::Attached` or `VolumeStatus::Error`

**G2. Update volume handlers in control plane**

File: `crates/shellwego-control-plane/src/api/handlers.rs`

For `create_volume`:
```rust
pub async fn create_volume(/* ... */) -> Result<...> {
    // 1. Validate request
    // 2. Create DB record with VolumeStatus::Creating
    // 3. Emit event to agent message bus (or schedule via agent communication)
    // 4. Return the volume record with status=Creating
    //    The actual ZFS provisioning happens asynchronously on the agent
}
```

For `snapshot_volume`:
```rust
pub async fn snapshot_volume(/* ... */) -> Result<...> {
    // 1. Validate volume exists and is in Attached status
    // 2. Send snapshot command to the agent hosting this volume
    // 3. Return success or error from agent response
}
```

The control plane handlers remain API-facing orchestrators. They dispatch work to agents.

**G3. Wire `BackupService` to real storage**

File: `crates/shellwego-control-plane/src/services/backup.rs`

Replace the simulated `execute_backup()` method:
```rust
async fn execute_backup(&self, backup: &mut BackupMetadata) -> Result<(), BackupError> {
    backup.status = BackupStatus::InProgress { progress_percent: 0 };
    self.update_backup(backup.clone()).await;

    // Real implementation:
    // 1. If ResourceType::Volume, call zfs snapshot on the agent
    // 2. Send the snapshot stream to S3 backend
    // 3. Update backup with real size_bytes from ZFS

    // Phase 1: For now, keep simulated but add hooks for real integration
    // TODO(Plan 10): Replace with actual ZfsManager::snapshot_volume() call
    // dispatched to the appropriate agent node.

    for progress in [25, 50, 75, 100] {
        tokio::time::sleep(Duration::from_millis(50)).await;
        backup.status = BackupStatus::InProgress { progress_percent: progress };
        self.update_backup(backup.clone()).await;
    }

    backup.size_bytes = 0; // Will be populated from actual ZFS snapshot
    backup.completed_at = Some(Utc::now());
    backup.status = BackupStatus::Completed;
    self.update_backup(backup.clone()).await;
    Ok(())
}
```

### Phase H: Wire CLI Volume Commands

**H1. Implement CLI volume API calls**

File: `crates/shellwego-cli/src/commands/volumes.rs`

Replace the stub implementations with actual API client calls:
```rust
pub async fn handle(args: VolumeArgs, config: &CliConfig, format: OutputFormat) -> anyhow::Result<()> {
    let client = crate::client(config)?;

    match args.command {
        VolumeCommands::List => {
            let resp = client.get("/v1/volumes").send().await?;
            let volumes: serde_json::Value = resp.json().await?;
            print_response(&volumes, format);
        }
        VolumeCommands::Create { name, size_gb } => {
            let body = serde_json::json!({
                "name": name,
                "size_gb": size_gb,
                "volume_type": "persistent",
                "filesystem": "ext4",
                "encrypted": false
            });
            let resp = client.post("/v1/volumes").json(&body).send().await?;
            let volume: serde_json::Value = resp.json().await?;
            print_response(&volume, format);
        }
        VolumeCommands::Get { id } => {
            let resp = client.get(&format!("/v1/volumes/{}", id)).send().await?;
            let volume: serde_json::Value = resp.json().await?;
            print_response(&volume, format);
        }
        VolumeCommands::Delete { id } => {
            client.delete(&format!("/v1/volumes/{}", id)).send().await?;
            println!("Volume {} deleted", id);
        }
        VolumeCommands::Attach { id, app_id } => {
            let body = serde_json::json!({ "app_id": app_id });
            let resp = client.post(&format!("/v1/volumes/{}/attach", id)).json(&body).send().await?;
            let result: serde_json::Value = resp.json().await?;
            print_response(&result, format);
        }
        VolumeCommands::Detach { id } => {
            let resp = client.post(&format!("/v1/volumes/{}/detach", id)).send().await?;
            let result: serde_json::Value = resp.json().await?;
            print_response(&result, format);
        }
        VolumeCommands::Snapshot { id, name } => {
            let body = serde_json::json!({ "name": name });
            let resp = client.post(&format!("/v1/volumes/{}/snapshots", id)).json(&body).send().await?;
            let snapshot: serde_json::Value = resp.json().await?;
            print_response(&snapshot, format);
        }
    }
    Ok(())
}

fn print_response(value: &serde_json::Value, format: OutputFormat) {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(value).unwrap()),
        OutputFormat::Text => println!("{}", value),
    }
}
```

**H2. Add `OutputFormat` helper (if not already present)**

File: `crates/shellwego-cli/src/commands/volumes.rs`

Ensure `OutputFormat` is imported from `crate::OutputFormat`.

### Phase I: Storage Metrics

**I1. Create metrics module**

File: `crates/shellwego-storage/src/metrics.rs`

```rust
//! Storage metrics collection
//!
//! Polls ZFS pool and volume metrics periodically and exposes them
//! for the observability pipeline (Prometheus, tracing spans).

use crate::zfs::{PoolMetrics, ZfsManager};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct StorageMetrics {
    pool_metrics: Arc<RwLock<Option<PoolMetrics>>>,
    volume_compression: Arc<RwLock<std::collections::HashMap<String, f64>>>,
}

impl StorageMetrics {
    pub fn new() -> Self {
        Self {
            pool_metrics: Arc::new(RwLock::new(None)),
            volume_compression: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Start background metrics collection
    pub async fn start_collection(&self, zfs: Arc<ZfsManager>, interval: Duration) {
        let pool_ref = self.pool_metrics.clone();
        let compression_ref = self.volume_compression.clone();
        let zfs_clone = zfs.clone();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;

                // Collect pool metrics
                match zfs_clone.get_pool_metrics().await {
                    Ok(metrics) => {
                        info!(
                            "Pool {}: {}GB / {}GB used, {}% fragmented, dedup={:.2}x",
                            metrics.name,
                            metrics.allocated_bytes / (1024 * 1024 * 1024),
                            metrics.size_bytes / (1024 * 1024 * 1024),
                            metrics.fragmentation_percent,
                            metrics.dedup_ratio,
                        );
                        *pool_ref.write().await = Some(metrics);
                    }
                    Err(e) => debug!("Failed to collect pool metrics: {}", e),
                }

                // Collect volume compression ratios
                match zfs_clone.list_volumes().await {
                    Ok(volumes) => {
                        let mut comp = compression_ref.write().await;
                        comp.clear();
                        for vol in &volumes {
                            if vol.compression_ratio > 1.0 {
                                comp.insert(vol.name.clone(), vol.compression_ratio);
                                debug!("Volume {} compression ratio: {:.2}x", vol.name, vol.compression_ratio);
                            }
                        }
                    }
                    Err(e) => debug!("Failed to list volumes for metrics: {}", e),
                }
            }
        });
    }

    /// Get latest pool metrics
    pub async fn pool_metrics(&self) -> Option<PoolMetrics> {
        self.pool_metrics.read().await.clone()
    }

    /// Get compression ratios for all volumes with ratio > 1.0
    pub async fn compression_ratios(&self) -> std::collections::HashMap<String, f64> {
        self.volume_compression.read().await.clone()
    }
}
```

**I2. Add `list_volumes()` to `ZfsManager`**

File: `crates/shellwego-storage/src/zfs/mod.rs`

```rust
/// List all shellwego volumes
pub async fn list_volumes(&self) -> Result<Vec<VolumeInfo>, StorageError> {
    let volumes_dataset = format!("{}/volumes", self.base_dataset);
    self.cli.list_volumes(&volumes_dataset).await
}
```

**I3. Wire metrics into agent startup**

In the agent `daemon.rs` or `main.rs`, after `ZfsManager` initialization:
```rust
if let Some(ref zfs) = reconciler.zfs {
    let metrics = StorageMetrics::new();
    metrics.start_collection(Arc::new(zfs.clone()), Duration::from_secs(60)).await;
}
```

## 6. Dependencies on Other Plans

| Plan ID | Dependency | Notes |
|---|---|---|
| **Plan 01 (Security Hardening)** | RBAC — volume handlers need `check_permission("volumes:write")` etc. | This plan adds `shellwego-storage` dep to control-plane. If Plan 01 is already done, RBAC checks on volume handlers will be present. If not, add them. |
| **Plan 03 (QUIC Message Bus)** | Agent ↔ control plane communication for provisioning commands | Phase G assumes agent communication exists. Without it, control plane can only create DB records. |
| **Plan 04 (Agent Activation)** | Agent registration, health checks, config distribution | Phase D adds `SHELLWEGO_ZFS_POOL` to agent config. Must not conflict with agent config changes in Plan 04. |

This plan can proceed independently for Phases A-F (storage crate internal work). Phases G-H (control plane wiring, CLI) depend on Plan 03 for the message bus.

## 7. Acceptance Criteria

### Unit Tests
- [ ] `cargo test -p shellwego-storage` passes with 0 failures
- [ ] `VolumeProvisioner::provision()` test — creates dataset, returns `ProvisionedVolume`
- [ ] `VolumeProvisioner::provision()` encrypted test — returns `EncryptionStatus::ZfsNative`
- [ ] `VolumeProvisioner::destroy()` test — destroys volume and snapshots
- [ ] `ZfsCli::create_zvol()` test — mocked or feature-gated
- [ ] `ZfsCli::check_output()` error parsing — "dataset does not exist" → `NotFound`, "already exists" → `AlreadyExists`
- [ ] `EncryptionProvider` existing tests still pass (3 tests)
- [ ] `DataKey::from_base64` / `to_base64` roundtrip still passes
- [ ] `Luks2Manager` compiles behind `luks2` feature flag
- [ ] `StorageMetrics` construction and field access work

### Integration Verification
- [ ] Agent starts with `SHELLWEGO_ZFS_POOL=tank` → `ZfsManager` initializes, log confirms
- [ ] Agent starts without ZFS pool set → falls back to directory-based volumes, warning logged
- [ ] Agent reconciler provisions volume via `ZfsManager::create_volume()` when ZFS available
- [ ] Agent reconciler falls back to `tokio::fs::create_dir_all()` when ZFS unavailable
- [ ] `snapshot.rs` uses `ZfsCli` instead of inline `Command::new("zfs")` — verify by checking no raw `Command::new("zfs")` in snapshot.rs
- [ ] `cargo build --release` succeeds with 0 errors across all crates

### Compression Ratio
- [ ] ZFS `VolumeInfo.compression_ratio` is populated from `zfs list -o compressratio` (already works)
- [ ] S3 `VolumeInfo.created` uses `Last-Modified` instead of `Utc::now()`
- [ ] `VolumeInfo.compression_ratio` is serializable to JSON (for API responses)

### CLI
- [ ] `shellwego volumes list` calls `GET /v1/volumes` and prints response
- [ ] `shellwego volumes create myvol 10` calls `POST /v1/volumes` with body
- [ ] `shellwego volumes delete <uuid>` calls `DELETE /v1/volumes/<uuid>`

## 8. Estimated Complexity

**L** (Large)

Rationale:
- Phase A (VolumeProvisioner): ~200 lines new code. Core orchestration logic. Medium complexity.
- Phase B (ZFS CLI hardening): ~120 lines changed/added across 2 files. Mechanical but many call sites for timeout wrapping (~15). Medium complexity.
- Phase C (Encryption integration): ~250 lines new code across 3 files. Crypto + filesystem integration. High complexity (key management is security-sensitive).
- Phase D (Agent reconciler): ~80 lines changed. Medium complexity (conditional ZFS/directory fallback).
- Phase E (Snapshot refactor): ~100 lines replaced. Low complexity (straightforward substitution).
- Phase F (Compression ratio): ~40 lines changed across 3 files. Low complexity.
- Phase G (Control plane wiring): ~60 lines changed. Medium complexity (depends on message bus design).
- Phase H (CLI wiring): ~60 lines changed. Low complexity (mechanical API calls).
- Phase I (Metrics): ~80 lines new code. Low-medium complexity.

Total: ~990 lines of production code + ~150 lines of test code across 12 files modified, 3 new files.

## 9. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **ZFS not available in CI** — tests that call `zfs` will fail without a real pool | High | Medium — tests skipped, less coverage | All ZFS tests behind `#[cfg(feature = "zfs-integration")]`. Use mock ZFS for unit tests. CI runs without the flag. |
| **Key material leak** — raw encryption keys written to disk even briefly | Low | Critical — data at rest compromised | Use `zeroize` crate on temp key buffers. Ensure `O_NOCTTY` / `O_EXCL` on temp files. Delete with overwrite pattern (write zeros, sync, unlink). |
| **Timeout wrapping breaks CLI calls** — 15 call sites modified for `tokio::time::timeout` | Medium | Medium — some operations may timeout prematurely | Use generous default (30s). Make timeout configurable on `ZfsCli`. Pool operations and snapshot creation may need longer timeouts — expose per-method overrides. |
| **Control-plane `shellwego-storage` dep bloat** — adding storage crate to control plane pulls in `aws-sdk-s3`, `aes-gcm`, etc. | Certain | Low — larger binary, longer compile | Consider making `shellwego-storage` features modular: `s3`, `encryption`, `oci` as cargo features. Control plane only needs the types, not the backends. Alternatively, move shared types to `shellwego-schema` and keep storage implementations agent-only. |
| **Breaking change to `ZfsCli::new()`** — adding `timeout` field changes the struct | Low | Low — only internal crate usage | `ZfsCli::new()` keeps backward-compatible defaults. New `ZfsCli::with_timeout()` for customization. |
| **LUKS2 + ZFS conflict** — encrypting a zvol with both ZFS native encryption and LUKS2 is redundant and error-prone | Medium | Medium — performance degradation, key management complexity | Documentation must clearly state: use ZFS native encryption for datasets, LUKS2 only for zvols that need portability. `VolumeEncryptor::encrypt_volume()` should refuse to double-encrypt. |
| **Agent config collision with Plan 04** — both plans modify agent config | Medium | Low — merge conflict | Coordinate field names: `SHELLWEGO_ZFS_POOL` (this plan) vs Plan 04 fields. Keep config structs additive. |
| **S3 `compression_ratio` always 1.0** — S3 doesn't track compression natively | Certain | Low — inaccurate metrics for S3 volumes | Document that `compression_ratio` is ZFS-only. Consider renaming to `compression_ratio_zfs` or adding a note in the API response. |
