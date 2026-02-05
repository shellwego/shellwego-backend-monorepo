use std::path::PathBuf;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::time::Duration;
use tracing::{debug, info};
use crate::{StorageError, VolumeInfo, SnapshotInfo, OciConfig, OciClient};

pub mod cli;
pub use cli::ZfsCli;

#[derive(Clone)]
pub struct ZfsManager {
    pool: String,
    #[allow(dead_code)]
    base_dataset: String,
    cli: ZfsCli,
    cache: Arc<RwLock<PropertyCache>>,
}

struct PropertyCache {
    entries: HashMap<String, (VolumeInfo, std::time::Instant)>,
    ttl: Duration,
}

impl ZfsManager {
    pub async fn new(pool: &str) -> Result<Self, StorageError> {
        let cli = ZfsCli::new();
        Ok(Self {
            pool: pool.to_string(),
            base_dataset: format!("{}/shellwego", pool),
            cli,
            cache: Arc::new(RwLock::new(PropertyCache {
                entries: HashMap::new(),
                ttl: Duration::from_secs(30),
            })),
        })
    }

    pub async fn init_app_storage(&self, app_id: uuid::Uuid) -> Result<AppStorage, StorageError> {
        let rootfs = format!("{}/apps/{}/rootfs", self.pool, app_id);
        let data = format!("{}/apps/{}/data", self.pool, app_id);
        Ok(AppStorage { app_id, rootfs, data, snapshots: String::new() })
    }

    pub async fn prepare_rootfs(&self, app_id: uuid::Uuid, image_ref: &str) -> Result<PathBuf, StorageError> {
        let target_dataset = format!("{}/apps/{}/rootfs", self.pool, app_id);
        self.pull_image_to_dataset(image_ref, &target_dataset).await?;
        let info = self.cli.get_info(&target_dataset).await?;
        Ok(info.mountpoint.unwrap_or_default())
    }

    pub async fn create_volume(&self, volume_id: uuid::Uuid, size_gb: u64) -> Result<VolumeInfo, StorageError> {
        let name = format!("{}/volumes/{}", self.pool, volume_id);
        self.cli.create_dataset(&name, None).await?;
        self.cli.set_property(&name, "quota", &format!("{}G", size_gb)).await?;
        self.cli.get_info(&name).await
    }

    pub async fn snapshot_volume(&self, volume_id: uuid::Uuid, snap_name: &str) -> Result<SnapshotInfo, StorageError> {
        let name = format!("{}/volumes/{}", self.pool, volume_id);
        self.cli.create_snapshot(&name, snap_name).await?;
        self.cli.get_snapshot_info(&format!("{}@{}", name, snap_name)).await
    }

    pub async fn rollback_volume(&self, volume_id: uuid::Uuid, snap_name: &str) -> Result<(), StorageError> {
        let name = format!("{}/volumes/{}", self.pool, volume_id);
        self.cli.rollback(&format!("{}@{}", name, snap_name), true).await
    }

    pub async fn cleanup_app(&self, app_id: uuid::Uuid) -> Result<(), StorageError> {
        let name = format!("{}/apps/{}", self.pool, app_id);
        self.cli.destroy_dataset(&name, true).await
    }

    pub async fn get_pool_metrics(&self) -> Result<PoolMetrics, StorageError> {
        self.cli.get_pool_info(&self.pool).await
    }

    async fn pull_image_to_dataset(&self, image_ref: &str, target_dataset: &str) -> Result<(), StorageError> {
        let oci_config = OciConfig {
            registry: "docker.io".to_string(),
            username: None, password: None, insecure: false, platform: None,
        };
        let oci_client = OciClient::new(oci_config).await.map_err(|e| StorageError::Backend(e.to_string()))?;
        let info = self.cli.get_info(target_dataset).await.ok();
        let mountpoint = info.and_then(|i| i.mountpoint).unwrap_or_else(|| PathBuf::from("/tmp"));
        oci_client.pull_image(image_ref, target_dataset, mountpoint).await.map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AppStorage {
    pub app_id: uuid::Uuid,
    pub rootfs: String,
    pub data: String,
    pub snapshots: String,
}

#[derive(Debug, Clone)]
pub struct PoolMetrics {
    pub name: String,
    pub size_bytes: u64,
    pub allocated_bytes: u64,
    pub free_bytes: u64,
    pub fragmentation_percent: f64,
    pub dedup_ratio: f64,
}
