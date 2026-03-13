//! Snapshot Management for ShellWeGo Agent
//!
//! Provides ZFS-backed snapshot functionality for Firecracker microVMs.
//! Supports creating, restoring, and managing VM snapshots with both
//! memory state and disk state persistence.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::vmm::{MicrovmConfig, VmmManager};

// Re-export types from schema
pub use shellwego_schema::{AgentSnapshotInfo, AgentSnapshotType};

/// Internal metadata for snapshot tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotMetadata {
    pub id: String,
    pub app_id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub memory_path: String,
    pub snapshot_path: String,
    pub size_bytes: u64,
    pub vm_config: Option<MicrovmConfig>,
    pub disk_snapshot: Option<String>,
    pub snapshot_type: AgentSnapshotType,
    pub includes_memory: bool,
    pub zfs_dataset: Option<String>,
}

/// ZFS integration for disk snapshots
#[derive(Clone)]
struct ZfsSnapshotManager {
    pool: String,
    base_dataset: String,
}

impl ZfsSnapshotManager {
    async fn new(pool: &str) -> anyhow::Result<Self> {
        let base_dataset = format!("{}/shellwego/snapshots", pool);
        Ok(Self {
            pool: pool.to_string(),
            base_dataset,
        })
    }

    async fn create_disk_snapshot(
        &self,
        app_id: Uuid,
        snapshot_name: &str,
    ) -> anyhow::Result<Option<String>> {
        // Check if ZFS is available
        if !self.is_zfs_available().await {
            debug!("ZFS not available, skipping disk snapshot");
            return Ok(None);
        }

        let app_dataset = format!("{}/shellwego/apps/{}", self.pool, app_id);

        // Check if the app dataset exists
        if !self.dataset_exists(&app_dataset).await? {
            debug!(
                "App dataset {} does not exist, skipping disk snapshot",
                app_dataset
            );
            return Ok(None);
        }

        let snapshot_full = format!("{}@{}", app_dataset, snapshot_name);

        // Create ZFS snapshot
        let output = tokio::process::Command::new("zfs")
            .args(["snapshot", &snapshot_full])
            .output()
            .await?;

        if output.status.success() {
            info!("Created ZFS disk snapshot: {}", snapshot_full);
            Ok(Some(snapshot_full))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to create ZFS snapshot: {}", stderr);
            Ok(None)
        }
    }

    async fn restore_disk_snapshot(
        &self,
        snapshot_path: &str,
        new_app_id: Uuid,
    ) -> anyhow::Result<Option<String>> {
        if !self.is_zfs_available().await {
            return Ok(None);
        }

        // Parse snapshot path to get dataset and snapshot name
        let parts: Vec<&str> = snapshot_path.split('@').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid snapshot path format: {}", snapshot_path);
        }

        let source_dataset = parts[0];
        let snap_name = parts[1];

        // Create new dataset for the cloned app
        let target_dataset = format!("{}/shellwego/apps/{}", self.pool, new_app_id);

        // Clone the snapshot
        let output = tokio::process::Command::new("zfs")
            .args(["clone", snapshot_path, &target_dataset])
            .output()
            .await?;

        if output.status.success() {
            info!(
                "Cloned ZFS snapshot {} to {}",
                snapshot_path, target_dataset
            );
            Ok(Some(target_dataset))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to clone ZFS snapshot: {}", stderr)
        }
    }

    async fn delete_disk_snapshot(&self, snapshot_path: &str) -> anyhow::Result<()> {
        if !self.is_zfs_available().await {
            return Ok(());
        }

        let output = tokio::process::Command::new("zfs")
            .args(["destroy", snapshot_path])
            .output()
            .await?;

        if output.status.success() {
            info!("Deleted ZFS snapshot: {}", snapshot_path);
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to delete ZFS snapshot: {}", stderr);
        }

        Ok(())
    }

    async fn is_zfs_available(&self) -> bool {
        tokio::process::Command::new("which")
            .arg("zfs")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn dataset_exists(&self, dataset: &str) -> anyhow::Result<bool> {
        let output = tokio::process::Command::new("zfs")
            .args(["list", "-H", "-o", "name", dataset])
            .output()
            .await?;

        Ok(output.status.success())
    }

    async fn get_snapshot_size(&self, snapshot_path: &str) -> anyhow::Result<u64> {
        let output = tokio::process::Command::new("zfs")
            .args(["list", "-H", "-p", "-o", "used", snapshot_path])
            .output()
            .await?;

        if output.status.success() {
            let size_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(size_str.parse().unwrap_or(0))
        } else {
            Ok(0)
        }
    }
}

/// Manages VM snapshots with ZFS backend support
#[derive(Clone)]
pub struct SnapshotManager {
    /// Directory for storing snapshot files
    snapshot_dir: PathBuf,
    /// In-memory metadata cache
    metadata: Arc<RwLock<HashMap<String, SnapshotMetadata>>>,
    /// ZFS integration
    zfs: Option<ZfsSnapshotManager>,
    /// Metadata file path
    metadata_path: PathBuf,
}

impl SnapshotManager {
    /// Create a new snapshot manager
    ///
    /// # Arguments
    /// * `data_dir` - Base directory for snapshot storage
    ///
    /// # Returns
    /// A new SnapshotManager instance
    pub async fn new(data_dir: &Path) -> anyhow::Result<Self> {
        let snapshot_dir = data_dir.join("snapshots");
        let metadata_path = snapshot_dir.join("metadata").join("snapshots.json");

        tokio::fs::create_dir_all(snapshot_dir.join("memory")).await?;
        tokio::fs::create_dir_all(snapshot_dir.join("metadata")).await?;

        // Try to initialize ZFS manager
        let zfs = ZfsSnapshotManager::new("shellwego").await.ok();

        // Load existing metadata
        let metadata = Self::load_metadata(&metadata_path).await?;

        let manager = Self {
            snapshot_dir,
            metadata: Arc::new(RwLock::new(metadata)),
            zfs,
            metadata_path,
        };

        Ok(manager)
    }

    /// Create a snapshot manager with explicit ZFS pool
    pub async fn with_zfs_pool(data_dir: &Path, zfs_pool: &str) -> anyhow::Result<Self> {
        let snapshot_dir = data_dir.join("snapshots");
        let metadata_path = snapshot_dir.join("metadata").join("snapshots.json");

        tokio::fs::create_dir_all(snapshot_dir.join("memory")).await?;
        tokio::fs::create_dir_all(snapshot_dir.join("metadata")).await?;

        let zfs = ZfsSnapshotManager::new(zfs_pool).await.ok();
        let metadata = Self::load_metadata(&metadata_path).await?;

        Ok(Self {
            snapshot_dir,
            metadata: Arc::new(RwLock::new(metadata)),
            zfs,
            metadata_path,
        })
    }

    /// Load metadata from disk
    async fn load_metadata(path: &Path) -> anyhow::Result<HashMap<String, SnapshotMetadata>> {
        if !path.exists() {
            return Ok(HashMap::new());
        }

        let content = tokio::fs::read_to_string(path).await?;
        if content.trim().is_empty() {
            return Ok(HashMap::new());
        }

        serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse snapshot metadata: {}", e))
    }

    /// Save metadata to disk
    async fn save_metadata(&self) -> anyhow::Result<()> {
        let meta = self.metadata.read().await;
        let content = serde_json::to_string_pretty(&*meta)?;

        // Ensure parent directory exists
        if let Some(parent) = self.metadata_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&self.metadata_path, content).await?;
        Ok(())
    }

    /// Create a new snapshot
    ///
    /// This operation:
    /// 1. Pauses the VM for consistency
    /// 2. Creates memory snapshot via Firecracker API
    /// 3. Creates ZFS disk snapshot if available
    /// 4. Resumes the VM
    ///
    /// # Arguments
    /// * `vmm_manager` - VMM manager for VM operations
    /// * `app_id` - Application ID to snapshot
    /// * `snapshot_name` - Human-readable name for the snapshot
    ///
    /// # Returns
    /// AgentSnapshotInfo on success
    pub async fn create_snapshot(
        &self,
        vmm_manager: &VmmManager,
        app_id: Uuid,
        snapshot_name: &str,
    ) -> anyhow::Result<AgentSnapshotInfo> {
        let snapshot_id = format!("{}-{}", snapshot_name, Uuid::new_v4());
        info!("Creating snapshot {} for app {}", snapshot_id, app_id);

        let base_path = self.snapshot_dir.join("memory").join(&snapshot_id);
        let mem_path = base_path.with_extension("mem");
        let snap_path = base_path.with_extension("snap");

        // Ensure parent directory exists
        if let Some(parent) = mem_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 1. Pause VM to ensure consistency
        debug!("Pausing VM for snapshot");
        vmm_manager.pause(app_id).await.map_err(|e| {
            error!("Failed to pause VM for snapshot: {}", e);
            e
        })?;

        // Track if we need to resume on error
        let mut should_resume = true;

        // 2. Take memory snapshot
        let memory_result = vmm_manager
            .snapshot_vm_state(app_id, mem_path.clone(), snap_path.clone())
            .await;
        if let Err(e) = memory_result {
            error!("Failed to create memory snapshot: {}", e);
            // Try to resume VM on error
            if let Err(resume_err) = vmm_manager.resume(app_id).await {
                error!("Failed to resume VM after snapshot failure: {}", resume_err);
            }
            should_resume = false;
            return Err(e);
        }

        // 3. Create ZFS disk snapshot if available
        let disk_snapshot = if let Some(ref zfs) = self.zfs {
            match zfs.create_disk_snapshot(app_id, &snapshot_id).await {
                Ok(Some(path)) => {
                    info!("Created ZFS disk snapshot: {}", path);
                    Some(path)
                }
                Ok(None) => {
                    debug!("No ZFS disk snapshot created (ZFS unavailable or no dataset)");
                    None
                }
                Err(e) => {
                    warn!("Failed to create ZFS disk snapshot: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // 4. Resume VM
        if should_resume {
            debug!("Resuming VM after snapshot");
            vmm_manager.resume(app_id).await.map_err(|e| {
                error!("Failed to resume VM after snapshot: {}", e);
                e
            })?;
        }

        // Calculate total size
        let mut size_bytes = 0u64;
        if mem_path.exists() {
            if let Ok(metadata) = tokio::fs::metadata(&mem_path).await {
                size_bytes += metadata.len();
            }
        }
        if snap_path.exists() {
            if let Ok(metadata) = tokio::fs::metadata(&snap_path).await {
                size_bytes += metadata.len();
            }
        }
        if let Some(ref zfs) = self.zfs {
            if let Some(ref disk_snap) = disk_snapshot {
                if let Ok(zfs_size) = zfs.get_snapshot_size(disk_snap).await {
                    size_bytes += zfs_size;
                }
            }
        }

        let now = Utc::now();

        // Store metadata
        let snapshot_metadata = SnapshotMetadata {
            id: snapshot_id.clone(),
            app_id,
            name: snapshot_name.to_string(),
            created_at: now,
            memory_path: mem_path.to_string_lossy().to_string(),
            snapshot_path: snap_path.to_string_lossy().to_string(),
            size_bytes,
            vm_config: None,
            disk_snapshot: disk_snapshot.clone(),
            snapshot_type: AgentSnapshotType::Full,
            includes_memory: true,
            zfs_dataset: disk_snapshot.clone(),
        };

        {
            let mut meta = self.metadata.write().await;
            meta.insert(snapshot_id.clone(), snapshot_metadata.clone());
        }

        // Persist metadata
        if let Err(e) = self.save_metadata().await {
            warn!("Failed to persist snapshot metadata: {}", e);
        }

        info!(
            "Created snapshot {} for app {} ({} bytes)",
            snapshot_id, app_id, size_bytes
        );

        Ok(AgentSnapshotInfo {
            id: snapshot_id,
            app_id,
            name: snapshot_name.to_string(),
            created_at: now,
            size_bytes,
            memory_path: mem_path.to_string_lossy().to_string(),
            disk_snapshot,
            snapshot_type: AgentSnapshotType::Full,
            includes_memory: true,
        })
    }

    /// Create a disk-only snapshot (no memory state)
    ///
    /// This is faster and smaller but requires a full boot on restore
    pub async fn create_disk_only_snapshot(
        &self,
        app_id: Uuid,
        snapshot_name: &str,
    ) -> anyhow::Result<AgentSnapshotInfo> {
        let snapshot_id = format!("{}-{}", snapshot_name, Uuid::new_v4());
        info!(
            "Creating disk-only snapshot {} for app {}",
            snapshot_id, app_id
        );

        let disk_snapshot = if let Some(ref zfs) = self.zfs {
            zfs.create_disk_snapshot(app_id, &snapshot_id).await?
        } else {
            None
        };

        let disk_snapshot = match disk_snapshot {
            Some(ds) => ds,
            None => anyhow::bail!("ZFS not available for disk-only snapshot"),
        };

        let now = Utc::now();
        let size_bytes = if let Some(ref zfs) = self.zfs {
            zfs.get_snapshot_size(&disk_snapshot).await.unwrap_or(0)
        } else {
            0
        };

        let metadata = SnapshotMetadata {
            id: snapshot_id.clone(),
            app_id,
            name: snapshot_name.to_string(),
            created_at: now,
            memory_path: String::new(),
            snapshot_path: String::new(),
            size_bytes,
            vm_config: None,
            disk_snapshot: Some(disk_snapshot.clone()),
            snapshot_type: AgentSnapshotType::DiskOnly,
            includes_memory: false,
            zfs_dataset: Some(disk_snapshot.clone()),
        };

        {
            let mut meta = self.metadata.write().await;
            meta.insert(snapshot_id.clone(), metadata);
        }

        self.save_metadata().await?;

        Ok(AgentSnapshotInfo {
            id: snapshot_id,
            app_id,
            name: snapshot_name.to_string(),
            created_at: now,
            size_bytes,
            memory_path: String::new(),
            disk_snapshot: Some(disk_snapshot),
            snapshot_type: AgentSnapshotType::DiskOnly,
            includes_memory: false,
        })
    }

    /// Restore a snapshot to a new app ID
    ///
    /// # Arguments
    /// * `vmm_manager` - VMM manager for VM operations
    /// * `snapshot_id` - ID of the snapshot to restore
    /// * `new_app_id` - New application ID for the restored VM
    pub async fn restore_snapshot(
        &self,
        vmm_manager: &VmmManager,
        snapshot_id: &str,
        new_app_id: Uuid,
    ) -> anyhow::Result<()> {
        info!(
            "Restoring snapshot {} to new app {}",
            snapshot_id, new_app_id
        );

        let meta = self.metadata.read().await;
        let snapshot_info = meta
            .get(snapshot_id)
            .ok_or_else(|| anyhow::anyhow!("Snapshot metadata not found for {}", snapshot_id))?
            .clone();
        drop(meta);

        // Handle disk snapshot restoration
        if let Some(disk_snap) = &snapshot_info.disk_snapshot {
            if let Some(ref zfs) = self.zfs {
                info!("Restoring ZFS snapshot: {}", disk_snap);
                match zfs.restore_disk_snapshot(disk_snap, new_app_id).await {
                    Ok(Some(new_dataset)) => {
                        info!("Created new dataset from snapshot: {}", new_dataset);
                    }
                    Ok(None) => {
                        debug!("No ZFS dataset cloned (ZFS unavailable)");
                    }
                    Err(e) => {
                        warn!("Failed to restore ZFS snapshot: {}", e);
                        // Continue with memory-only restore if available
                    }
                }
            }
        }

        // Restore memory state if available
        if snapshot_info.includes_memory {
            let mem_path = PathBuf::from(&snapshot_info.memory_path);
            let snap_path = PathBuf::from(&snapshot_info.snapshot_path);

            if !mem_path.exists() || !snap_path.exists() {
                anyhow::bail!(
                    "Snapshot files missing on disk: {:?}, {:?}",
                    mem_path,
                    snap_path
                );
            }

            vmm_manager
                .restore_from_snapshot(new_app_id, mem_path, snap_path)
                .await?;
        } else {
            // For disk-only snapshots, we need to start a fresh VM
            // The disk has been cloned, but we need configuration
            info!("Disk-only snapshot restored. VM needs to be started with appropriate configuration.");
        }

        info!(
            "Successfully restored snapshot {} to app {}",
            snapshot_id, new_app_id
        );
        Ok(())
    }

    /// List all snapshots, optionally filtered by app ID
    pub async fn list_snapshots(
        &self,
        app_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<AgentSnapshotInfo>> {
        let meta = self.metadata.read().await;
        Ok(meta
            .values()
            .filter(|m| app_id.map_or(true, |id| m.app_id == id))
            .map(|m| AgentSnapshotInfo {
                id: m.id.clone(),
                app_id: m.app_id,
                name: m.name.clone(),
                created_at: m.created_at,
                size_bytes: m.size_bytes,
                memory_path: m.memory_path.clone(),
                disk_snapshot: m.disk_snapshot.clone(),
                snapshot_type: m.snapshot_type,
                includes_memory: m.includes_memory,
            })
            .collect())
    }

    /// Delete a snapshot
    ///
    /// Removes both memory files and ZFS snapshots
    pub async fn delete_snapshot(&self, snapshot_id: &str) -> anyhow::Result<()> {
        info!("Deleting snapshot {}", snapshot_id);

        let mut meta = self.metadata.write().await;
        if let Some(m) = meta.remove(snapshot_id) {
            // Delete memory snapshot files
            if !m.memory_path.is_empty() {
                if let Err(e) = tokio::fs::remove_file(&m.memory_path).await {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        warn!("Failed to delete memory file {}: {}", m.memory_path, e);
                    }
                }
            }
            if !m.snapshot_path.is_empty() {
                if let Err(e) = tokio::fs::remove_file(&m.snapshot_path).await {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        warn!("Failed to delete snapshot file {}: {}", m.snapshot_path, e);
                    }
                }
            }

            // Delete ZFS snapshot
            if let Some(disk_snap) = &m.disk_snapshot {
                if let Some(ref zfs) = self.zfs {
                    if let Err(e) = zfs.delete_disk_snapshot(disk_snap).await {
                        warn!("Failed to delete ZFS snapshot {}: {}", disk_snap, e);
                    }
                }
            }

            info!("Deleted snapshot {}", snapshot_id);
        } else {
            warn!("Snapshot {} not found for deletion", snapshot_id);
        }
        drop(meta);

        self.save_metadata().await?;
        Ok(())
    }

    /// Get snapshot info by ID
    pub async fn get_snapshot(
        &self,
        snapshot_id: &str,
    ) -> anyhow::Result<Option<AgentSnapshotInfo>> {
        let meta = self.metadata.read().await;
        Ok(meta.get(snapshot_id).map(|m| AgentSnapshotInfo {
            id: m.id.clone(),
            app_id: m.app_id,
            name: m.name.clone(),
            created_at: m.created_at,
            size_bytes: m.size_bytes,
            memory_path: m.memory_path.clone(),
            disk_snapshot: m.disk_snapshot.clone(),
            snapshot_type: m.snapshot_type,
            includes_memory: m.includes_memory,
        }))
    }

    /// Get total storage used by snapshots
    pub async fn total_storage_used(&self) -> u64 {
        let meta = self.metadata.read().await;
        meta.values().map(|m| m.size_bytes).sum()
    }

    /// Garbage collect old snapshots
    ///
    /// # Arguments
    /// * `max_age_hours` - Maximum age of snapshots to keep
    /// * `keep_count` - Minimum number of snapshots to keep per app
    pub async fn gc_old_snapshots(
        &self,
        max_age_hours: u64,
        keep_count: usize,
    ) -> anyhow::Result<usize> {
        let meta = self.metadata.read().await;
        let cutoff = Utc::now() - chrono::Duration::hours(max_age_hours as i64);

        // Group snapshots by app
        let mut by_app: HashMap<Uuid, Vec<&SnapshotMetadata>> = HashMap::new();
        for m in meta.values() {
            by_app.entry(m.app_id).or_default().push(m);
        }

        let mut to_delete = Vec::new();

        for (app_id, snapshots) in by_app {
            // Sort by creation time (newest first)
            let mut sorted: Vec<_> = snapshots.into_iter().collect();
            sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            // Keep at least `keep_count` most recent
            for (i, snap) in sorted.iter().enumerate() {
                if i >= keep_count && snap.created_at < cutoff {
                    to_delete.push(snap.id.clone());
                }
            }
        }

        drop(meta);

        let deleted = to_delete.len();
        for id in to_delete {
            self.delete_snapshot(&id).await?;
        }

        info!("Garbage collected {} old snapshots", deleted);
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_snapshot_manager_creation() {
        let dir = tempdir().unwrap();
        let result = SnapshotManager::new(dir.path()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_snapshots_empty() {
        let dir = tempdir().unwrap();
        let manager = SnapshotManager::new(dir.path()).await.unwrap();
        let snapshots = manager.list_snapshots(None).await.unwrap();
        assert!(snapshots.is_empty());
    }

    #[tokio::test]
    async fn test_total_storage_empty() {
        let dir = tempdir().unwrap();
        let manager = SnapshotManager::new(dir.path()).await.unwrap();
        let total = manager.total_storage_used().await;
        assert_eq!(total, 0);
    }

    #[test]
    fn test_snapshot_info_serialization() {
        let info = AgentSnapshotInfo {
            id: "test-123".to_string(),
            app_id: Uuid::nil(),
            name: "test-snapshot".to_string(),
            created_at: Utc::now(),
            size_bytes: 1024,
            memory_path: "/tmp/mem.snap".to_string(),
            disk_snapshot: Some("pool/app@snap".to_string()),
            snapshot_type: AgentSnapshotType::Full,
            includes_memory: true,
        };

        let json = serde_json::to_string(&info).unwrap();
        let decoded: AgentSnapshotInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, info.id);
    }
}
