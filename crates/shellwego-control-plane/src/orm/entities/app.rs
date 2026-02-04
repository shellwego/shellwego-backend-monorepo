//! App entity using Sea-ORM
//!
//! Represents deployable applications running in Firecracker microVMs.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

// TODO: Add #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "apps")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String, // TODO: Use AppStatus enum with custom type
    pub image: String,
    pub command: Option<Json>, // TODO: Use Vec<String> with custom type
    pub resources: Json, // TODO: Use ResourceSpec with custom type
    pub env: Json, // TODO: Use Vec<EnvVar> with custom type
    pub domains: Json, // TODO: Use Vec<DomainConfig> with custom type
    pub volumes: Json, // TODO: Use Vec<VolumeMount> with custom type
    pub health_check: Option<Json>, // TODO: Use HealthCheck with custom type
    pub source: Json, // TODO: Use SourceSpec with custom type
    pub organization_id: Uuid,
    pub created_by: Uuid,
    pub created_at: DateTime,
    pub updated_at: DateTime,
    // TODO: Add replica_count field
    // TODO: Add networking_policy field
    // TODO: Add tags field
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // TODO: Define relation to Organization
    // TODO: Define relation to User (created_by)
    // TODO: Define relation to AppInstance (has many)
    // TODO: Define relation to Deployment (has many)
    // TODO: Define relation to Volume (many-to-many)
    // TODO: Define relation to Domain (many-to-many)
}

impl ActiveModelBehavior for ActiveModel {
    // TODO: Implement before_save hook for slug generation
    // TODO: Implement after_save hook for event publishing
    // TODO: Implement before_update hook for version tracking
}

// TODO: Implement conversion methods between ORM Model and core entity App
// impl From<Model> for shellwego_core::entities::app::App { ... }
// impl From<shellwego_core::entities::app::App> for ActiveModel { ... }

// TODO: Implement custom query methods
// impl Model {
//     pub async fn find_by_org(db: &DatabaseConnection, org_id: Uuid) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_by_status(db: &DatabaseConnection, status: AppStatus) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_with_instances(db: &DatabaseConnection, app_id: Uuid) -> Result<(Self, Vec<app_instance::Model>), DbErr> { ... }
// }
