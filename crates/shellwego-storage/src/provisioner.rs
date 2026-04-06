//! Volume provisioning orchestrator
//!
//! High-level API for creating, destroying, snapshotting, and cloning
//! storage volumes. Integrates ZfsManager + EncryptionProvider.

use crate::encryption::{EncryptionProvider, EncryptionStatus};
use crate::zfs::ZfsManager;
use crate::{SnapshotInfo, StorageError, VolumeInfo};
use std::sync::Arc;
use tracing::{info, warn};
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
    pub compression: Option<String>,
    pub volume_type: ProvisionVolumeType,
}

#[derive(Debug, Clone, Copy)]
pub enum ProvisionVolumeType {
    Dataset,
    Zvol,
}

#[derive(Debug, Clone)]
pub struct ProvisionedVolume {
    pub info: VolumeInfo,
    pub encryption_status: EncryptionStatus,
    pub dataset_path: String,
    pub mountpoint: Option<std::path::PathBuf>,
}

impl VolumeProvisioner {
    pub fn new(zfs: Arc<ZfsManager>, encryption: Option<Arc<EncryptionProvider>>) -> Self {
        Self { zfs, encryption }
    }

    /// Provision a new volume
    pub async fn provision(&self, req: ProvisionVolumeRequest) -> Result<ProvisionedVolume, StorageError> {
        let vol_name = format!("volumes/{}", req.volume_id);
        let dataset_path = self.zfs.full_path(&vol_name);

        match req.volume_type {
            ProvisionVolumeType::Zvol => {
                // Create block device volume
                self.zfs.create_zvol(req.volume_id, req.size_gb).await?;
            }
            ProvisionVolumeType::Dataset => {
                // Create filesystem dataset
                if !self.zfs.cli().dataset_exists(&dataset_path).await? {
                    self.zfs.create_volume(req.volume_id, req.size_gb).await?;
                }
            }
        }

        // Set compression if specified
        if let Some(ref comp) = req.compression {
            if comp != "off" {
                self.zfs.cli().set_property(&dataset_path, "compression", comp).await?;
            }
        }

        // Get volume info
        let info = self.zfs.cli().get_info(&dataset_path).await?;

        // Determine encryption status
        let encryption_status = if req.encrypted {
            if self.encryption.is_some() {
                EncryptionStatus::ZfsNative
            } else {
                warn!("Volume {} requested encryption but no EncryptionProvider configured", req.volume_id);
                EncryptionStatus::Unencrypted
            }
        } else {
            EncryptionStatus::Unencrypted
        };

        let mountpoint = info.mountpoint.clone();

        info!(
            "Provisioned volume {} ({}GB, type={:?}, encrypted={})",
            req.volume_id, req.size_gb, req.volume_type, req.encrypted
        );

        Ok(ProvisionedVolume {
            info,
            encryption_status,
            dataset_path,
            mountpoint,
        })
    }

    /// Destroy a volume and its snapshots
    pub async fn destroy(&self, volume_id: Uuid) -> Result<(), StorageError> {
        let vol_name = format!("volumes/{}", volume_id);
        let full_path = self.zfs.full_path(&vol_name);

        if self.zfs.cli().dataset_exists(&full_path).await? {
            self.zfs.cli().destroy_dataset(&full_path, true).await?;
            info!("Destroyed volume {}", volume_id);
        } else {
            warn!("Volume {} does not exist, skipping destroy", volume_id);
        }

        Ok(())
    }

    /// Create a snapshot of a volume
    pub async fn snapshot(&self, volume_id: Uuid, snap_name: &str) -> Result<SnapshotInfo, StorageError> {
        self.zfs.snapshot_volume(volume_id, snap_name).await
    }

    /// Clone a snapshot to a new volume
    pub async fn clone(
        &self,
        volume_id: Uuid,
        snap_name: &str,
        target_volume_id: Uuid,
    ) -> Result<ProvisionedVolume, StorageError> {
        let vol_name = format!("volumes/{}", volume_id);
        let full_name = self.zfs.full_path(&vol_name);
        let snapshot = format!("{}@{}", full_name, snap_name);

        let target_name = format!("volumes/{}", target_volume_id);
        let target_path = self.zfs.full_path(&target_name);

        self.zfs.cli().clone_snapshot(&snapshot, &target_path).await?;

        let info = self.zfs.cli().get_info(&target_path).await?;
        let mountpoint = info.mountpoint.clone();

        Ok(ProvisionedVolume {
            info,
            encryption_status: EncryptionStatus::Unencrypted,
            dataset_path: target_path,
            mountpoint,
        })
    }

    /// Get current status of a provisioned volume
    pub async fn get_status(&self, volume_id: Uuid) -> Result<ProvisionedVolume, StorageError> {
        let vol_name = format!("volumes/{}", volume_id);
        let dataset_path = self.zfs.full_path(&vol_name);

        let info = self.zfs.cli().get_info(&dataset_path).await?;
        let mountpoint = info.mountpoint.clone();

        let encryption_status = if let Some(ref _encryption) = self.encryption {
            // Check if volume has ZFS native encryption
            if self.zfs.cli().is_encrypted(&dataset_path).await? {
                EncryptionStatus::ZfsNative
            } else {
                EncryptionStatus::ApplicationLevel
            }
        } else {
            EncryptionStatus::Unencrypted
        };

        Ok(ProvisionedVolume {
            info,
            encryption_status,
            dataset_path,
            mountpoint,
        })
    }

    /// Get current compression ratio for a volume from ZFS
    pub async fn get_compression_ratio(&self, volume_id: Uuid) -> Result<f64, StorageError> {
        let vol_name = format!("volumes/{}", volume_id);
        let dataset = self.zfs.full_path(&vol_name);
        self.zfs.cli().get_compression_ratio(&dataset).await
    }
}
