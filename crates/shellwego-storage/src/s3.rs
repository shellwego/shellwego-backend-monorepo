//! S3-compatible remote storage backend
//!
//! S3 is object storage designed for backups and archival, NOT for mounting as
//! block devices. For Firecracker rootfs, use ZfsManager instead - it provides
//! the CoW snapshots and virtio-blk support needed for 85ms cold starts.
//!
//! S3Backend usage:
//!   - Offsite backups of ZFS snapshots
//!   - Long-term blob storage
//!   - Cross-region replication
//!
//! DO NOT use mount()/unmount() - S3 over FUSE produces abysmal performance
//! and violates the cold-start promise.

use crate::{StorageBackend, StorageError, VolumeInfo, SnapshotInfo};

/// S3 storage backend
pub struct S3Backend {
    // TODO: Add s3_client (rusoto or aws-sdk), bucket, prefix
}

#[async_trait::async_trait]
impl StorageBackend for S3Backend {
    async fn create(&self, name: &str, size: u64) -> Result<VolumeInfo, StorageError> {
        // TODO: Create placeholder object in S3
        unimplemented!("S3Backend::create - S3 doesn't support block devices")
    }

    async fn destroy(&self, name: &str, force: bool) -> Result<(), StorageError> {
        // TODO: Delete all objects with prefix
        unimplemented!("S3Backend::destroy")
    }

    async fn snapshot(&self, source: &str, snap_name: &str) -> Result<SnapshotInfo, StorageError> {
        // TODO: S3 copy operation for snapshot (blob-level, not block-level)
        unimplemented!("S3Backend::snapshot")
    }

    async fn clone(&self, snap: &str, target: &str) -> Result<VolumeInfo, StorageError> {
        // S3 "clone" is just a copy - no CoW like ZFS
        // This is slow and expensive. Use ZFS for instant cloning.
        unimplemented!("S3Backend::clone - Use ZfsManager.clone() for instant cloning")
    }

    async fn rollback(&self, snap: &str, force: bool) -> Result<(), StorageError> {
        unimplemented!("S3Backend::rollback")
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<VolumeInfo>, StorageError> {
        unimplemented!("S3Backend::list")
    }

    async fn info(&self, name: &str) -> Result<VolumeInfo, StorageError> {
        unimplemented!("S3Backend::info")
    }

    async fn mount(&self, _name: &str, _mountpoint: &std::path::PathBuf) -> Result<(), StorageError> {
        // ERROR: S3 over FUSE is NOT suitable for Firecracker rootfs
        // S3 is object storage, not block storage. Mounting it for virtio-blk
        // will produce 500ms+ latencies, breaking the 85ms cold-start promise.
        // Use ZfsManager for block device rootfs. Use S3 only for backups.
        Err(StorageError::PermissionDenied(
            "S3 cannot be mounted as a block device. Use ZfsManager for rootfs.".to_string()
        ))
    }

    async fn unmount(&self, _name: &str) -> Result<(), StorageError> {
        // See mount() - S3 should never be mounted for block device access
        Err(StorageError::PermissionDenied(
            "S3 is not a block device. Use ZfsManager for block storage.".to_string()
        ))
    }

    async fn set_property(&self, name: &str, key: &str, value: &str) -> Result<(), StorageError> {
        // TODO: Set S3 object tags or metadata
        unimplemented!("S3Backend::set_property")
    }

    async fn get_property(&self, name: &str, key: &str) -> Result<String, StorageError> {
        // TODO: Get S3 object tags or metadata
        unimplemented!("S3Backend::get_property")
    }
}

/// S3 backend configuration
#[derive(Debug, Clone)]
pub struct S3Config {
    // TODO: Add endpoint, region, bucket, access_key, secret_key, prefix
}