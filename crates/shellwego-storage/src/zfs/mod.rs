//! ZFS implementation of StorageBackend
//! 
//! Wraps `zfs` and `zpool` CLI commands. In production, this could
//! be replaced with libzfs_core FFI for lower overhead.

use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info, warn, error};

use crate::{StorageBackend, StorageError, VolumeInfo, SnapshotInfo};

pub mod cli;

pub use cli::ZfsCli;

/// ZFS storage manager
#[derive(Clone)]
pub struct ZfsManager {
    pool: String,
    base_dataset: String,
    cli: ZfsCli,
    // TODO: Add cache of dataset properties
}

impl ZfsManager {
    /// Create manager for a ZFS pool
    pub async fn new(pool: &str) -> Result<Self, StorageError> {
        let cli = ZfsCli::new();
        
        // Verify pool exists and is healthy
        cli.check_pool(pool).await?;
        
        let base_dataset = format!("{}/shellwego", pool);
        
        // Ensure base dataset exists
        if !cli.dataset_exists(&base_dataset).await? {
            info!("Creating base dataset: {}", base_dataset);
            cli.create_dataset(&base_dataset, None).await?;
            
            // Set default properties
            cli.set_property(&base_dataset, "compression", "zstd-3").await?;
            cli.set_property(&base_dataset, "atime", "off").await?;
            cli.set_property(&base_dataset, "xattr", "sa").await?;
        }
        
        Ok(Self {
            pool: pool.to_string(),
            base_dataset,
            cli,
        })
    }

    /// Get full dataset path for a volume/app
    fn full_path(&self, name: &str) -> String {
        format!("{}/{}", self.base_dataset, name)
    }

    /// Initialize app storage: creates dataset hierarchy
    pub async fn init_app_storage(&self, app_id: uuid::Uuid) -> Result<AppStorage, StorageError> {
        let app_dataset = self.full_path(&format!("apps/{}", app_id));
        
        // Create hierarchy
        self.cli.create_dataset(&app_dataset, None).await?;
        
        // Sub-datasets for different purposes
        let rootfs = format!("{}/rootfs", app_dataset);
        let data = format!("{}/data", app_dataset);
        let snapshots = format!("{}/.snapshots", app_dataset);
        
        self.cli.create_dataset(&rootfs, Some(&format!("{}/rootfs", app_dataset))).await?;
        self.cli.create_dataset(&data, Some(&format!("{}/data", app_dataset))).await?;
        self.cli.create_dataset(&snapshots, None).await?;
        
        // Rootfs is read-only base image, data is persistent
        self.cli.set_property(&rootfs, "readonly", "on").await?;
        
        Ok(AppStorage {
            app_id,
            rootfs,
            data,
            snapshots,
        })
    }

    /// Prepare container rootfs from image
    pub async fn prepare_rootfs(
        &self,
        app_id: uuid::Uuid,
        image_ref: &str,
    ) -> Result<PathBuf, StorageError> {
        let cache_dataset = self.full_path("cache/images");
        
        // Ensure image cache exists
        if !self.cli.dataset_exists(&cache_dataset).await? {
            self.cli.create_dataset(&cache_dataset, None).await?;
            self.cli.set_property(&cache_dataset, "compression", "zstd-3").await?;
        }
        
        // Sanitize image ref for dataset name
        let image_name = image_ref.replace([':', '/'], "_");
        let image_dataset = format!("{}/{}", cache_dataset, image_name);
        
        // Check if already cached
        if self.cli.dataset_exists(&image_dataset).await? {
            debug!("Using cached image: {}", image_dataset);
        } else {
            info!("Pulling and caching image: {}", image_ref);
            
            // TODO: Pull container image and extract to dataset
            // This requires integration with container runtime (skopeo, umoci, etc)
            self.pull_image_to_dataset(image_ref, &image_dataset).await?;
        }
        
        // Clone to app rootfs (writable overlay)
        let app_storage = self.init_app_storage(app_id).await?;
        let app_rootfs = format!("{}/rootfs", self.full_path(&format!("apps/{}", app_id)));
        
        // Destroy if exists (fresh deploy)
        if self.cli.dataset_exists(&app_rootfs).await? {
            self.cli.destroy_dataset(&app_rootfs, true).await?;
        }
        
        // Clone from cached image
        let snapshot = format!("{}@base", image_dataset);
        self.cli.clone_snapshot(&snapshot, &app_rootfs).await?;
        
        // Make writable (promote to independent dataset)
        self.cli.set_property(&app_rootfs, "readonly", "off").await?;
        self.cli.promote(&app_rootfs).await?;
        
        // Get mountpoint
        let info = self.cli.get_info(&app_rootfs).await?;
        
        Ok(info.mountpoint.ok_or_else(|| {
            StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No mountpoint for rootfs"
            ))
        })?)
    }

    /// Create persistent volume for app
    pub async fn create_volume(
        &self,
        volume_id: uuid::Uuid,
        size_gb: u64,
    ) -> Result<VolumeInfo, StorageError> {
        let vol_name = format!("volumes/{}", volume_id);
        let full_name = self.full_path(&vol_name);
        
        info!("Creating volume {} ({}GB)", volume_id, size_gb);
        
        // Create ZFS volume (block device) or dataset (filesystem)?
        // For Firecracker, we want raw block devices or mounted directories.
        // Use dataset with quota for filesystem, zvol for block.
        
        // Default to dataset for now (simpler)
        self.cli.create_dataset(&full_name, None).await?;
        self.cli.set_property(&full_name, "quota", &format!("{}G", size_gb)).await?;
        self.cli.set_property(&full_name, "reservation", &format!("{}G", size_gb / 10)).await?; // 10% reserved
        
        self.cli.get_info(&full_name).await
    }

    /// Snapshot volume before dangerous operation
    pub async fn snapshot_volume(
        &self,
        volume_id: uuid::Uuid,
        snap_name: &str,
    ) -> Result<SnapshotInfo, StorageError> {
        let vol_name = format!("volumes/{}", volume_id);
        let full_name = self.full_path(&vol_name);
        
        let snap = format!("{}@{}", full_name, snap_name);
        self.cli.create_snapshot(&full_name, snap_name).await?;
        
        self.cli.get_snapshot_info(&snap).await
    }

    /// Rollback volume to snapshot
    pub async fn rollback_volume(
        &self,
        volume_id: uuid::Uuid,
        snap_name: &str,
    ) -> Result<(), StorageError> {
        let vol_name = format!("volumes/{}", volume_id);
        let full_name = self.full_path(&vol_name);
        let snap = format!("{}@{}", full_name, snap_name);
        
        // Must unmount first
        if let Ok(info) = self.cli.get_info(&full_name).await {
            if info.mountpoint.is_some() {
                self.cli.unmount(&full_name, false).await?;
            }
        }
        
        self.cli.rollback(&snap, true).await
    }

    /// Clean up app storage after deletion
    pub async fn cleanup_app(&self, app_id: uuid::Uuid) -> Result<(), StorageError> {
        let app_dataset = self.full_path(&format!("apps/{}", app_id));
        
        if self.cli.dataset_exists(&app_dataset).await? {
            info!("Destroying app dataset: {}", app_dataset);
            self.cli.destroy_dataset(&app_dataset, true).await?;
        }
        
        Ok(())
    }

    /// Get storage metrics for node
    pub async fn get_pool_metrics(&self) -> Result<PoolMetrics, StorageError> {
        self.cli.get_pool_info(&self.pool).await
    }

    async fn pull_image_to_dataset(
        &self,
        image_ref: &str,
        target_dataset: &str,
    ) -> Result<(), StorageError> {
        // TODO: Integrate with container runtime
        // 1. skopeo copy docker://$image oci:/tmp/...
        // 2. umoci unpack --image ...
        // 3. rsync to mounted dataset
        // 4. snapshot @base
        
        // Placeholder: create empty dataset
        self.cli.create_dataset(target_dataset, None).await?;
        self.cli.snapshot(target_dataset, "base").await?;
        
        Ok(())
    }
}

/// App-specific storage paths
#[derive(Debug, Clone)]
pub struct AppStorage {
    pub app_id: uuid::Uuid,
    pub rootfs: String,      // Dataset name
    pub data: String,        // Persistent data dataset
    pub snapshots: String,   // Snapshot staging area
}

/// Pool utilization metrics
#[derive(Debug, Clone)]
pub struct PoolMetrics {
    pub name: String,
    pub size_bytes: u64,
    pub allocated_bytes: u64,
    pub free_bytes: u64,
    pub fragmentation_percent: f64,
    pub dedup_ratio: f64,
}