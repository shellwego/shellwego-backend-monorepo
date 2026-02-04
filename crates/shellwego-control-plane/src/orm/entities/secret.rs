//! Secret entity using Sea-ORM
//!
//! Represents encrypted secrets for credentials and sensitive configuration.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "secrets")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub name: String,
    pub scope: String, // TODO: Use SecretScope enum with custom type
    pub app_id: Option<Uuid>,
    pub current_version: u32,
    pub versions: Json, // TODO: Use Vec<SecretVersion> with custom type
    pub last_used_at: Option<DateTime>,
    pub expires_at: Option<DateTime>,
    pub organization_id: Uuid,
    pub created_at: DateTime,
    pub updated_at: DateTime,
    // TODO: Add encrypted_value field (stored separately in KMS)
    // TODO: Add kms_key_id field
    // TODO: Add rotation_policy field
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // TODO: Define relation to Organization
    // TODO: Define relation to App (optional, via app_id)
    // TODO: Define relation to SecretVersion (has many)
}

impl ActiveModelBehavior for ActiveModel {
    // TODO: Implement before_save hook for value encryption
    // TODO: Implement after_save hook for KMS storage
    // TODO: Implement before_update hook for version increment
}

// TODO: Implement conversion methods between ORM Model and core entity Secret
// impl From<Model> for shellwego_core::entities::secret::Secret { ... }
// impl From<shellwego_core::entities::secret::Secret> for ActiveModel { ... }

// TODO: Implement custom query methods
// impl Model {
//     pub async fn find_by_name(db: &DatabaseConnection, org_id: Uuid, name: &str) -> Result<Option<Self>, DbErr> { ... }
//     pub async fn find_by_app(db: &DatabaseConnection, app_id: Uuid) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_expired(db: &DatabaseConnection) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn update_last_used(db: &DatabaseConnection, secret_id: Uuid) -> Result<Self, DbErr> { ... }
// }
