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

use std::path::PathBuf;
use async_trait::async_trait;
use aws_sdk_s3::{Client, Config, Error as SdkError, types::{Tagging, Tag}};
use aws_types::region::Region;
use aws_credential_types::Credentials;
use thiserror::Error;
use crate::{StorageBackend, StorageError, VolumeInfo, SnapshotInfo};

#[derive(Debug, Error)]
pub enum S3Error {
    #[error("S3 operation failed: {0}")]
    Operation(String),
    #[error("Object not found: {0}")]
    NotFound(String),
    #[error("Invalid configuration: {0}")]
    Config(String),
}

impl From<S3Error> for StorageError {
    fn from(e: S3Error) -> Self {
        StorageError::Backend(format!("S3: {}", e))
    }
}

impl From<SdkError> for StorageError {
    fn from(e: SdkError) -> Self {
        StorageError::Backend(format!("AWS SDK: {}", e))
    }
}

/// S3 storage backend
#[derive(Clone)]
pub struct S3Backend {
    client: Client,
    bucket: String,
    prefix: String,
}

#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: Option<String>,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub prefix: Option<String>,
}

impl S3Backend {
    pub async fn new(config: S3Config) -> Result<Self, StorageError> {
        let region = Region::new(config.region.clone());
        
        let credentials = Credentials::new(
            config.access_key.clone(),
            config.secret_key.clone(),
            None,
            None,
            "shellwego",
        );

        let sdk_config = Config::builder()
            .region(region)
            .credentials_provider(credentials)
            .endpoint_url(config.endpoint.clone().unwrap_or_default())
            .build();

        let client = Client::from_conf(sdk_config);

        let bucket = config.bucket.clone();
        let prefix = config.prefix.unwrap_or_default();

        Ok(S3Backend { client, bucket, prefix })
    }

    fn object_key(&self, name: &str) -> String {
        format!("{}{}", self.prefix, name)
    }

    #[allow(dead_code)]
    fn parse_snapshot_key(&self, key: &str) -> Option<(String, String)> {
        let without_prefix = key.strip_prefix(&self.prefix)?;
        let parts: Vec<&str> = without_prefix.split('@').collect();
        if parts.len() == 2 {
            Some((parts[0].to_string(), parts[1].to_string()))
        } else {
            None
        }
    }
}

#[async_trait]
impl StorageBackend for S3Backend {
    async fn create(&self, name: &str, _size: u64) -> Result<VolumeInfo, StorageError> {
        let key = self.object_key(name);
        let body = format!("Volume placeholder: {}", name);
        
        self.client.put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(body.into_bytes().into())
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("S3 create: {}", e)))?;

        Ok(VolumeInfo {
            name: name.to_string(),
            mountpoint: None,
            used_bytes: 0,
            available_bytes: 0,
            referenced_bytes: 0,
            compression_ratio: 1.0,
            created: chrono::Utc::now(),
            properties: std::collections::HashMap::new(),
        })
    }

    async fn destroy(&self, name: &str, _force: bool) -> Result<(), StorageError> {
        let key = self.object_key(name);
        
        match self.client.delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                // S3 delete_object is idempotent and doesn't return 404 for missing objects normally.
                // If it does, it's a ServiceError.
                Err(StorageError::Backend(format!("S3 delete: {}", e)))
            }
        }
    }

    async fn snapshot(&self, source: &str, snap_name: &str) -> Result<SnapshotInfo, StorageError> {
        let source_key = self.object_key(source);
        let snap_key = format!("{}@{}", source_key, snap_name);
        
        let copy_source = format!("{}/{}", self.bucket, source_key);
        
        self.client.copy_object()
            .bucket(&self.bucket)
            .key(&snap_key)
            .copy_source(copy_source)
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("S3 snapshot: {}", e)))?;

        Ok(SnapshotInfo {
            name: snap_name.to_string(),
            dataset: source.to_string(),
            created: chrono::Utc::now(),
            used_bytes: 0,
            referenced_bytes: 0,
        })
    }

    async fn clone(&self, snap: &str, target: &str) -> Result<VolumeInfo, StorageError> {
        let snap_key = self.object_key(snap);
        let target_key = self.object_key(target);
        
        let copy_source = format!("{}/{}", self.bucket, snap_key);
        
        self.client.copy_object()
            .bucket(&self.bucket)
            .key(&target_key)
            .copy_source(copy_source)
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("S3 clone: {}", e)))?;

        self.create(target, 0).await
    }

    async fn rollback(&self, _snap: &str, _force: bool) -> Result<(), StorageError> {
        Err(StorageError::Unsupported(
            "S3 does not support rollback. Restore from backup.".to_string()
        ))
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<VolumeInfo>, StorageError> {
        let mut prefix = prefix.unwrap_or("");
        if prefix.is_empty() {
            prefix = &self.prefix;
        }

        let resp = self.client.list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix)
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("S3 list: {}", e)))?;

        let mut volumes = Vec::new();
        
        for obj in resp.contents() {
            if let Some(key) = obj.key() {
                if !key.contains('@') && key != prefix {
                    let name = key.strip_prefix(&self.prefix).unwrap_or(key);
                    let name = name.trim_end_matches('/');
                    
                    volumes.push(VolumeInfo {
                        name: name.to_string(),
                        mountpoint: None,
                        used_bytes: obj.size().unwrap_or(0) as u64,
                        available_bytes: 0,
                        referenced_bytes: obj.size().unwrap_or(0) as u64,
                        compression_ratio: 1.0,
                        created: chrono::Utc::now(),
                        properties: std::collections::HashMap::new(),
                    });
                }
            }
        }
        
        // resp.contents() returns a slice directly

        Ok(volumes)
    }

    async fn info(&self, name: &str) -> Result<VolumeInfo, StorageError> {
        let key = self.object_key(name);
        
        let resp = self.client.head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("S3 head: {}", e)))?;

        Ok(VolumeInfo {
            name: name.to_string(),
            mountpoint: None,
            used_bytes: resp.content_length().unwrap_or(0) as u64,
            available_bytes: 0,
            referenced_bytes: resp.content_length().unwrap_or(0) as u64,
            compression_ratio: 1.0,
            created: chrono::Utc::now(),
            properties: std::collections::HashMap::new(),
        })
    }

    async fn mount(&self, _name: &str, _mountpoint: &PathBuf) -> Result<(), StorageError> {
        Err(StorageError::PermissionDenied(
            "S3 cannot be mounted as a block device. Use ZfsManager for rootfs.".to_string()
        ))
    }

    async fn unmount(&self, _name: &str) -> Result<(), StorageError> {
        Err(StorageError::PermissionDenied(
            "S3 is not a block device. Use ZfsManager for block storage.".to_string()
        ))
    }

    async fn set_property(&self, name: &str, key: &str, value: &str) -> Result<(), StorageError> {
        let s3_key = self.object_key(name);
        
        self.client.put_object_tagging()
            .bucket(&self.bucket)
            .key(&s3_key)
            .tagging(Tagging::builder()
                .tag_set(
                    Tag::builder()
                        .key(key)
                        .value(value)
                        .build()
                        .map_err(|e| StorageError::Backend(format!("Failed to build tag: {}", e)))?
                )
                .build()
                .map_err(|e| StorageError::Backend(format!("Failed to build tagging: {}", e)))?)
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("S3 set tag: {}", e)))?;

        Ok(())
    }

    async fn get_property(&self, name: &str, key: &str) -> Result<String, StorageError> {
        let s3_key = self.object_key(name);
        
        let resp = self.client.get_object_tagging()
            .bucket(&self.bucket)
            .key(&s3_key)
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("S3 get tag: {}", e)))?;

        for tag in resp.tag_set() {
            if tag.key() == key {
                return Ok(tag.value().to_string());
            }
        }

        Err(StorageError::NotFound(format!("Tag {} not found", key)))
    }
}
