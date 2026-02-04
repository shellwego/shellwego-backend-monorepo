//! Database entity using Sea-ORM
//!
//! Represents managed databases (Postgres, MySQL, Redis, etc.).

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "databases")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub name: String,
    pub engine: String, // TODO: Use DatabaseEngine enum with custom type
    pub version: String,
    pub status: String, // TODO: Use DatabaseStatus enum with custom type
    pub endpoint: Json, // TODO: Use DatabaseEndpoint with custom type
    pub resources: Json, // TODO: Use DatabaseResources with custom type
    pub usage: Json, // TODO: Use DatabaseUsage with custom type
    pub ha: Json, // TODO: Use HighAvailability with custom type
    pub backup_config: Json, // TODO: Use DatabaseBackupConfig with custom type
    pub organization_id: Uuid,
    pub created_at: DateTime,
    pub updated_at: DateTime,
    // TODO: Add connection_pool_size field
    // TODO: Add maintenance_window field
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // TODO: Define relation to Organization
    // TODO: Define relation to DatabaseBackup (has many)
    // TODO: Define relation to App (many-to-many via app_databases)
}

impl ActiveModelBehavior for ActiveModel {
    // TODO: Implement before_save hook for endpoint encryption
    // TODO: Implement after_save hook for database provisioning event
    // TODO: Implement before_update hook for status transition validation
}

// TODO: Implement conversion methods between ORM Model and core entity Database
// impl From<Model> for shellwego_core::entities::database::Database { ... }
// impl From<shellwego_core::entities::database::Database> for ActiveModel { ... }

// TODO: Implement custom query methods
// impl Model {
//     pub async fn find_by_engine(db: &DatabaseConnection, engine: DatabaseEngine) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_by_status(db: &DatabaseConnection, status: DatabaseStatus) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn update_usage(db: &DatabaseConnection, db_id: Uuid, usage: DatabaseUsage) -> Result<Self, DbErr> { ... }
// }
