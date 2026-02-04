//! S3-compatible remote storage backend

use crate::{StorageBackend, StorageError, VolumeInfo, SnapshotInfo};

/// S3 storage backend
pub struct S3Backend {
    // TODO: Add s3_client (rusoto or aws-sdk), bucket, prefix
}

#[async_trait::async_trait]
impl StorageBackend for S3Backend {
    async fn create(&self, name: &str, size: u64) -> Result<VolumeInfo, StorageError> {
        // TODO: Create placeholder in S3 (actual data on first write)
        unimplemented!("S3Backend::create")
    }

    async fn destroy(&self, name: &str, force: bool) -> Result<(), StorageError> {
        // TODO: Delete all objects with prefix
        unimplemented!("S3Backend::destroy")
    }

    async fn snapshot(&self, source: &str, snap_name: &str) -> Result<SnapshotInfo, StorageError> {
        // TODO: S3 copy operation for snapshot
        unimplemented!("S3Backend::snapshot")
    }

    async fn clone(&self, snap: &str, target: &str) -> Result<VolumeInfo, StorageError> {
        // TODO: S3 copy from snapshot to new prefix
        unimplemented!("S3Backend::clone")
    }

    async fn rollback(&self, snap: &str, force: bool) -> Result<(), StorageError> {
        // TODO: S3 copy snapshot back to main prefix
        unimplemented!("S3Backend::rollback")
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<VolumeInfo>, StorageError> {
        // TODO: List objects with delimiter
        unimplemented!("S3Backend::list")
    }

    async fn info(&self, name: &str) -> Result<VolumeInfo, StorageError> {
        // TODO: Head object or list to calculate size
        unimplemented!("S3Backend::info")
    }

    async fn mount(&self, name: &str, mountpoint: &std::path::PathBuf) -> Result<(), StorageError> {
        // TODO: Use s3fs-fuse or mountpoint-s3
        unimplemented!("S3Backend::mount")
    }

    async fn unmount(&self, name: &str) -> Result<(), StorageError> {
        // TODO: Fusermount -u
        unimplemented!("S3Backend::unmount")
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