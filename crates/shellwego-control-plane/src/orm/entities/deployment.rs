//! Deployment entity using Sea-ORM
//!
//! Represents application deployments with version history.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "deployments")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub app_id: Uuid,
    pub version: u32,
    pub spec: Json, // TODO: Use DeploymentSpec with custom type
    pub state: String, // TODO: Use DeploymentState enum with custom type
    pub message: Option<String>,
    pub previous_deployment_id: Option<Uuid>,
    pub created_by: Uuid,
    pub created_at: DateTime,
    pub updated_at: DateTime,
    pub completed_at: Option<DateTime>,
    // TODO: Add rollback_to_version field
    // TODO: Add rollback_reason field
    // TODO: Add rollout_strategy field
    // TODO: Add rollout_percentage field
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // TODO: Define relation to App
    // TODO: Define relation to User (created_by)
    // TODO: Define relation to Deployment (self-referential, previous_deployment_id)
    // TODO: Define relation to AppInstance (has many)
}

impl ActiveModelBehavior for ActiveModel {
    // TODO: Implement before_save hook for version increment
    // TODO: Implement after_save hook for deployment started event
    // TODO: Implement before_update hook for state transition validation
}

// TODO: Implement conversion methods between ORM Model and core entity Deployment
// impl From<Model> for shellwego_core::entities::deployment::Deployment { ... }
// impl From<shellwego_core::entities::deployment::Deployment> for ActiveModel { ... }

// TODO: Implement custom query methods
// impl Model {
//     pub async fn find_by_app(db: &DatabaseConnection, app_id: Uuid) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_latest(db: &DatabaseConnection, app_id: Uuid) -> Result<Option<Self>, DbErr> { ... }
//     pub async fn find_rollback_chain(db: &DatabaseConnection, deployment_id: Uuid) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn update_state(db: &DatabaseConnection, deployment_id: Uuid, state: DeploymentState, message: Option<String>) -> Result<Self, DbErr> { ... }
// }
