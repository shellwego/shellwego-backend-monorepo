//! Persistent Volume entity definitions.
//!
//! ZFS-backed storage for application data.

use crate::prelude::*;

#[cfg(feature = "orm")]
use sea_orm::entity::prelude::*;

pub type VolumeId = Uuid;

/// Volume operational status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum VolumeStatus {
    Creating,
    Detached,
    Attaching,
    Attached,
    Snapshotting,
    Deleting,
    Error,
}

/// Volume type (performance characteristics)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum VolumeType {
    Persistent,
    Ephemeral,
    Shared,
}

/// Filesystem type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "lowercase")]
pub enum FilesystemType {
    Ext4,
    Xfs,
    Zfs,
    Btrfs,
}

/// Volume snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct Snapshot {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub parent_volume_id: VolumeId,
}

/// Backup policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct BackupPolicy {
    pub enabled: bool,
    pub frequency: String,
    pub retention_days: u32,
    pub destination: String,
}

/// Volume entity
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(DeriveEntityModel))]
#[cfg_attr(feature = "orm", sea_orm(table_name = "volumes"))]
pub struct Volume {
    #[cfg_attr(feature = "orm", sea_orm(primary_key, auto_increment = false))]
    pub id: VolumeId,
    pub name: String,
    pub status: VolumeStatus,
    pub size_gb: u64,
    pub used_gb: u64,
    pub volume_type: VolumeType,
    pub filesystem: FilesystemType,
    pub encrypted: bool,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(nullable))]
    pub encryption_key_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(nullable))]
    pub attached_to: Option<Uuid>,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(nullable))]
    pub mount_path: Option<String>,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub snapshots: Vec<Snapshot>,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary", nullable))]
    pub backup_policy: Option<BackupPolicy>,
    pub organization_id: Uuid,
    #[cfg_attr(feature = "orm", sea_orm(default_value = "sea_orm::prelude::DateTimeWithchrono::Utc::now()"))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "orm", sea_orm(default_value = "sea_orm::prelude::DateTimeWithchrono::Utc::now()"))]
    pub updated_at: DateTime<Utc>,
}

#[cfg(feature = "orm")]
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[cfg(feature = "orm")]
impl ActiveModelBehavior for ActiveModel {}

/// Volume type (performance characteristics)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum VolumeType {
    Persistent,  // Default, survives app deletion
    Ephemeral,   // Deleted with app
    Shared,      // NFS-style, multi-attach
}

/// Filesystem type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum FilesystemType {
    Ext4,
    Xfs,
    Zfs,
    Btrfs,
}

/// Volume snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct Snapshot {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub parent_volume_id: VolumeId,
}

/// Backup policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct BackupPolicy {
    pub enabled: bool,
    pub frequency: String, // "daily", "hourly", cron expression
    pub retention_days: u32,
    pub destination: String, // s3://bucket/path
}

/// Volume entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct Volume {
    pub id: VolumeId,
    pub name: String,
    pub status: VolumeStatus,
    pub size_gb: u64,
    pub used_gb: u64,
    pub volume_type: VolumeType,
    pub filesystem: FilesystemType,
    pub encrypted: bool,
    #[serde(default)]
    pub encryption_key_id: Option<String>,
    #[serde(default)]
    pub attached_to: Option<Uuid>, // App ID
    #[serde(default)]
    pub mount_path: Option<String>,
    pub snapshots: Vec<Snapshot>,
    #[serde(default)]
    pub backup_policy: Option<BackupPolicy>,
    pub organization_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create volume request
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct CreateVolumeRequest {
    pub name: String,
    pub size_gb: u64,
    #[serde(default = "default_volume_type")]
    pub volume_type: VolumeType,
    #[serde(default = "default_filesystem")]
    pub filesystem: FilesystemType,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default)]
    pub snapshot_id: Option<Uuid>,
}

fn default_volume_type() -> VolumeType { VolumeType::Persistent }
fn default_filesystem() -> FilesystemType { FilesystemType::Ext4 }