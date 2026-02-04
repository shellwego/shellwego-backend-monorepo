//! Volume entity using Sea-ORM
//!
//! Represents persistent storage volumes for applications.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "volumes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub name: String,
    pub status: String, // TODO: Use VolumeStatus enum with custom type
    pub size_gb: u64,
    pub used_gb: u64,
    pub volume_type: String, // TODO: Use VolumeType enum with custom type
    pub filesystem: String, // TODO: Use FilesystemType enum with custom type
    pub encrypted: bool,
    pub encryption_key_id: Option<String>,
    pub attached_to: Option<Uuid>, // App ID
    pub mount_path: Option<String>,
    pub snapshots: Json, // TODO: Use Vec<Snapshot> with custom type
    pub backup_policy: Option<Json>, // TODO: Use BackupPolicy with custom type
    pub organization_id: Uuid,
    pub created_at: DateTime,
    pub updated_at: DateTime,
    // TODO: Add zfs_dataset field
    // TODO: Add node_id field (for attached volumes)
    // TODO: Add iops_limit field
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // TODO: Define relation to Organization
    // TODO: Define relation to App (optional, via attached_to)
    // TODO: Define relation to Snapshot (has many)
    // TODO: Define relation to Backup (has many)
}

impl ActiveModelBehavior for ActiveModel {
    // TODO: Implement before_save hook for ZFS dataset creation
    // TODO: Implement after_save hook for volume provisioning event
    // TODO: Implement before_update hook for attachment validation
}

// TODO: Implement conversion methods between ORM Model and core entity Volume
// impl From<Model> for shellwego_core::entities::volume::Volume { ... }
// impl From<shellwego_core::entities::volume::Volume> for ActiveModel { ... }

// TODO: Implement custom query methods
// impl Model {
//     pub async fn find_by_status(db: &DatabaseConnection, status: VolumeStatus) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_by_app(db: &DatabaseConnection, app_id: Uuid) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_detached(db: &DatabaseConnection) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn update_usage(db: &DatabaseConnection, volume_id: Uuid, used_gb: u64) -> Result<Self, DbErr> { ... }
// }
