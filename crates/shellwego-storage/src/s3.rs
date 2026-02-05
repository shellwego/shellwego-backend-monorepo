use std::path::PathBuf;
use async_trait::async_trait;
use aws_sdk_s3::{Client, Config, types};
use aws_types::region::Region;
use aws_credential_types::Credentials;
use crate::{StorageBackend, StorageError, VolumeInfo, SnapshotInfo};

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
            None, None, "shellwego",
        );
        let mut builder = Config::builder()
            .region(region)
            .credentials_provider(credentials);
        if let Some(endpoint) = config.endpoint {
            builder = builder.endpoint_url(endpoint);
        }
        let sdk_config = builder.build();
        let client = Client::from_conf(sdk_config);
        Ok(S3Backend {
            client,
            bucket: config.bucket,
            prefix: config.prefix.unwrap_or_default(),
        })
    }
    fn object_key(&self, name: &str) -> String {
        format!("{}{}", self.prefix, name)
    }
}

#[async_trait]
impl StorageBackend for S3Backend {
    async fn create(&self, name: &str, _size: u64) -> Result<VolumeInfo, StorageError> {
        let key = self.object_key(name);
        self.client.put_object()
            .bucket(&self.bucket).key(&key)
            .body("Volume placeholder".as_bytes().to_vec().into())
            .send().await
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
        self.client.delete_object().bucket(&self.bucket).key(&key).send().await
            .map_err(|e| StorageError::Backend(format!("S3 delete: {}", e)))?;
        Ok(())
    }
    async fn snapshot(&self, source: &str, snap_name: &str) -> Result<SnapshotInfo, StorageError> {
        let source_key = self.object_key(source);
        let snap_key = format!("{}@{}", source_key, snap_name);
        self.client.copy_object()
            .bucket(&self.bucket).key(&snap_key)
            .copy_source(format!("{}/{}", self.bucket, source_key))
            .send().await
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
        self.client.copy_object()
            .bucket(&self.bucket).key(&target_key)
            .copy_source(format!("{}/{}", self.bucket, snap_key))
            .send().await
            .map_err(|e| StorageError::Backend(format!("S3 clone: {}", e)))?;
        self.create(target, 0).await
    }
    async fn rollback(&self, _snap: &str, _force: bool) -> Result<(), StorageError> {
        Err(StorageError::Unsupported("S3 rollback not supported".into()))
    }
    async fn list(&self, prefix: Option<&str>) -> Result<Vec<VolumeInfo>, StorageError> {
        let prefix = prefix.unwrap_or(&self.prefix);
        let resp = self.client.list_objects_v2().bucket(&self.bucket).prefix(prefix).send().await
            .map_err(|e| StorageError::Backend(format!("S3 list: {}", e)))?;
        let mut volumes = Vec::new();
        for obj in resp.contents() {
            if let Some(key) = obj.key() {
                if !key.contains('@') && key != prefix {
                    volumes.push(VolumeInfo {
                        name: key.to_string(),
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
        Ok(volumes)
    }
    async fn info(&self, name: &str) -> Result<VolumeInfo, StorageError> {
        let key = self.object_key(name);
        let resp = self.client.head_object().bucket(&self.bucket).key(&key).send().await
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
        Err(StorageError::PermissionDenied("S3 mount not supported".into()))
    }
    async fn unmount(&self, _name: &str) -> Result<(), StorageError> {
        Err(StorageError::PermissionDenied("S3 unmount not supported".into()))
    }
    async fn set_property(&self, name: &str, key: &str, value: &str) -> Result<(), StorageError> {
        let s3_key = self.object_key(name);
        self.client.put_object_tagging()
            .bucket(&self.bucket).key(&s3_key)
            .tagging(types::Tagging::builder()
                .tag_set(types::Tag::builder().key(key).value(value).build().unwrap())
                .build().unwrap())
            .send().await
            .map_err(|e| StorageError::Backend(format!("S3 set tag: {}", e)))?;
        Ok(())
    }
    async fn get_property(&self, name: &str, key: &str) -> Result<String, StorageError> {
        let s3_key = self.object_key(name);
        let resp = self.client.get_object_tagging().bucket(&self.bucket).key(&s3_key).send().await
            .map_err(|e| StorageError::Backend(format!("S3 get tag: {}", e)))?;
        for tag in resp.tag_set() {
            if tag.key() == key {
                return Ok(tag.value().to_string());
            }
        }
        Err(StorageError::NotFound(key.to_string()))
    }
}
