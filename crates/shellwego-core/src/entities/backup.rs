//! Backup and disaster recovery entities

use crate::prelude::*;

/// Backup record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct Backup {
    pub id: uuid::Uuid,
    pub resource_type: ResourceType,
    pub resource_id: uuid::Uuid,
    pub status: BackupStatus,
    pub size_bytes: u64,
    pub storage_location: String,
    pub started_at: chrono::DateTime<Utc>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub expires_at: chrono::DateTime<Utc>,
    pub metadata: BackupMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum ResourceType {
    App,
    Database,
    Volume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum BackupStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct BackupMetadata {
    pub encryption_key_id: Option<String>,
    pub checksum: String,
    pub compression_format: CompressionFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum CompressionFormat {
    None,
    Gzip,
    Zstd,
}

/// Restore job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct RestoreJob {
    pub id: uuid::Uuid,
    pub backup_id: uuid::Uuid,
    pub target_resource_id: uuid::Uuid,
    pub status: RestoreStatus,
    pub started_at: chrono::DateTime<Utc>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum RestoreStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}