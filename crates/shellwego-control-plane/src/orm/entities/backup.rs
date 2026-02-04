//! Backup entity using Sea-ORM
//!
//! Represents backups for databases and volumes.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "backups")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub resource_type: String, // TODO: Use ResourceType enum (Database, Volume) with custom type
    pub resource_id: Uuid,
    pub name: String,
    pub status: String, // TODO: Use BackupStatus enum with custom type
    pub size_bytes: u64,
    pub storage_location: String, // e.g., s3://bucket/path
    pub checksum: String,
    pub created_at: DateTime,
    pub completed_at: Option<DateTime>,
    pub expires_at: Option<DateTime>,
    // TODO: Add wal_segment_start field (for Postgres)
    // TODO: Add wal_segment_end field (for Postgres)
    // TODO: Add retention_days field
    // TODO: Add encryption_key_id field
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // TODO: Define relation to Database (polymorphic via resource_id)
    // TODO: Define relation to Volume (polymorphic via resource_id)
}

impl ActiveModelBehavior for ActiveModel {
    // TODO: Implement before_save hook for backup initiation
    // TODO: Implement after_save hook for backup started event
    // TODO: Implement before_update hook for completion validation
}

// TODO: Implement conversion methods between ORM Model and core entity Backup
// impl From<Model> for shellwego_core::entities::backup::Backup { ... }
// impl From<shellwego_core::entities::backup::Backup> for ActiveModel { ... }

// TODO: Implement custom query methods
// impl Model {
//     pub async fn find_by_resource(db: &DatabaseConnection, resource_type: ResourceType, resource_id: Uuid) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_expired(db: &DatabaseConnection) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_latest(db: &DatabaseConnection, resource_type: ResourceType, resource_id: Uuid) -> Result<Option<Self>, DbErr> { ... }
//     pub async fn update_status(db: &DatabaseConnection, backup_id: Uuid, status: BackupStatus) -> Result<Self, DbErr> { ... }
// }
