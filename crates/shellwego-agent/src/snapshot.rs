use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;
use serde::{Serialize, Deserialize};

use crate::vmm::{VmmManager, MicrovmConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub id: String,
    pub app_id: Uuid,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub size_bytes: u64,
    pub memory_path: String,
    pub disk_snapshot: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotMetadata {
    pub id: String,
    pub app_id: Uuid,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub memory_path: String,
    pub snapshot_path: String,
    pub size_bytes: u64,
    pub vm_config: Option<MicrovmConfig>,
    pub disk_snapshot: Option<String>,
}

#[derive(Clone)]
pub struct SnapshotManager {
    snapshot_dir: PathBuf,
    metadata: Arc<RwLock<HashMap<String, SnapshotMetadata>>>,
}

impl SnapshotManager {
    pub async fn new(data_dir: &Path) -> anyhow::Result<Self> {
        let snapshot_dir = data_dir.join("snapshots");
        tokio::fs::create_dir_all(snapshot_dir.join("memory")).await?;
        tokio::fs::create_dir_all(snapshot_dir.join("metadata")).await?;

        Ok(Self {
            snapshot_dir,
            metadata: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn create_snapshot(
        &self,
        vmm_manager: &VmmManager,
        app_id: Uuid,
        snapshot_name: &str,
    ) -> anyhow::Result<SnapshotInfo> {
        let snapshot_id = format!("{}-{}", snapshot_name, Uuid::new_v4());
        info!("Creating snapshot {} for app {}", snapshot_id, app_id);

        let base_path = self.snapshot_dir.join("memory").join(&snapshot_id);
        let mem_path = base_path.with_extension("mem");
        let snap_path = base_path.with_extension("snap");

        // 1. Pause VM to ensure consistency
        vmm_manager.pause(app_id).await?;

        // 2. Take memory snapshot
        vmm_manager.snapshot_vm_state(app_id, mem_path.clone(), snap_path.clone()).await?;

        // 3. TODO: Take ZFS disk snapshot here (requires ZFS integration from storage crate)
        
        // 4. Resume VM
        vmm_manager.resume(app_id).await?;

        let info = SnapshotInfo {
            id: snapshot_id,
            app_id,
            name: snapshot_name.to_string(),
            created_at: chrono::Utc::now(),
            size_bytes: 0,
            memory_path: mem_path.to_string_lossy().to_string(),
            disk_snapshot: None,
        };

        Ok(info)
    }

    pub async fn restore_snapshot(
        &self,
        vmm_manager: &VmmManager,
        snapshot_id: &str,
        new_app_id: Uuid,
    ) -> anyhow::Result<()> {
        info!("Restoring snapshot {} to new app {}", snapshot_id, new_app_id);
        
        let meta = self.metadata.read().await;
        let snapshot_info = meta.get(snapshot_id)
            .ok_or_else(|| anyhow::anyhow!("Snapshot metadata not found for {}", snapshot_id))?;
            
        let mem_path = PathBuf::from(&snapshot_info.memory_path);
        let snap_path = PathBuf::from(&snapshot_info.snapshot_path);
        
        if !mem_path.exists() || !snap_path.exists() {
            anyhow::bail!("Snapshot files missing on disk");
        }
        
        // TODO: If we have ZFS snapshots, clone the dataset here before starting VM
        if let Some(disk_snap) = &snapshot_info.disk_snapshot {
            info!("Should restore ZFS snapshot: {}", disk_snap);
            // shellwego_storage::zfs::clone_snapshot(disk_snap, &new_app_id.to_string()).await?;
        }

        vmm_manager.restore_from_snapshot(new_app_id, mem_path, snap_path).await?;
        
        Ok(())
    }

    pub async fn list_snapshots(&self, app_id: Option<Uuid>) -> anyhow::Result<Vec<SnapshotInfo>> {
        let meta = self.metadata.read().await;
        Ok(meta.values()
            .filter(|m| app_id.map_or(true, |id| m.app_id == id))
            .map(|m| SnapshotInfo {
                id: m.id.clone(),
                app_id: m.app_id,
                name: m.name.clone(),
                created_at: m.created_at,
                size_bytes: m.size_bytes,
                memory_path: m.memory_path.clone(),
                disk_snapshot: m.disk_snapshot.clone(),
            })
            .collect())
    }

    pub async fn delete_snapshot(&self, snapshot_id: &str) -> anyhow::Result<()> {
        let mut meta = self.metadata.write().await;
        if let Some(m) = meta.remove(snapshot_id) {
            let _ = tokio::fs::remove_file(m.memory_path).await;
            let _ = tokio::fs::remove_file(m.snapshot_path).await;
            // TODO: Cleanup ZFS snapshot if exists
        }
        Ok(())
    }

    pub async fn get_snapshot(&self, snapshot_id: &str) -> anyhow::Result<Option<SnapshotInfo>> {
        let meta = self.metadata.read().await;
        Ok(meta.get(snapshot_id).map(|m| SnapshotInfo {
            id: m.id.clone(),
            app_id: m.app_id,
            name: m.name.clone(),
            created_at: m.created_at,
            size_bytes: m.size_bytes,
            memory_path: m.memory_path.clone(),
            disk_snapshot: m.disk_snapshot.clone(),
        }))
    }
}
