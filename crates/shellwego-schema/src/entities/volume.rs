//! Persistent Volume entity definitions.
//!
//! ZFS-backed storage for application data.

use crate::prelude::*;
#[cfg(feature = "orm")]
use sea_orm::entity::prelude::*;
#[cfg(feature = "orm")]
use sea_query::IdenStatic;

pub type VolumeId = Uuid;

/// Volume operational status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_orm::EnumIter, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum VolumeStatus {
    #[cfg_attr(feature = "orm", sea_orm(string_value = "creating"))]
    Creating,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "detached"))]
    Detached,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "attaching"))]
    Attaching,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "attached"))]
    Attached,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "snapshotting"))]
    Snapshotting,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "deleting"))]
    Deleting,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "error"))]
    Error,
}

/// Volume type (performance characteristics)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_orm::EnumIter, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum VolumeType {
    #[cfg_attr(feature = "orm", sea_orm(string_value = "persistent"))]
    Persistent,  // Default, survives app deletion
    #[cfg_attr(feature = "orm", sea_orm(string_value = "ephemeral"))]
    Ephemeral,   // Deleted with app
    #[cfg_attr(feature = "orm", sea_orm(string_value = "shared"))]
    Shared,      // NFS-style, multi-attach
}

/// Filesystem type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_orm::EnumIter, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "lowercase")]
pub enum FilesystemType {
    #[cfg_attr(feature = "orm", sea_orm(string_value = "ext4"))]
    Ext4,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "xfs"))]
    Xfs,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "zfs"))]
    Zfs,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "btrfs"))]
    Btrfs,
}

/// Volume snapshot metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromJsonQueryResult))]
pub struct Snapshot {
    pub id: Uuid,
    pub name: String,
    pub created_at: chrono::DateTime<Utc>,
    pub size_bytes: u64,
    pub parent_volume_id: VolumeId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromJsonQueryResult))]
#[serde(transparent)]
pub struct Snapshots(pub Vec<Snapshot>);

/// Backup policy configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromJsonQueryResult))]
pub struct BackupPolicy {
    pub enabled: bool,
    pub frequency: String, // "daily", "hourly", cron expression
    pub retention_days: u32,
    pub destination: String, // s3://bucket/path
}

/// Volume entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(DeriveEntityModel))]
#[cfg_attr(feature = "orm", sea_orm(table_name = "volumes"))]
pub struct Model {
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
    pub encryption_key_id: Option<String>,
    #[serde(default)]
    pub attached_to: Option<Uuid>, // App ID
    #[serde(default)]
    pub mount_path: Option<String>,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub snapshots: Snapshots,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary", nullable))]
    pub backup_policy: Option<BackupPolicy>,
    pub organization_id: Uuid,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[cfg(feature = "orm")]
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[cfg(feature = "orm")]
impl ActiveModelBehavior for ActiveModel {}

/// Create volume request
#[derive(Debug, Clone, Serialize, Deserialize, Validate, PartialEq)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
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
