//! User entity using Sea-ORM
//!
//! Represents users who can access organizations.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub role: String, // TODO: Use UserRole enum with custom type
    pub organization_id: Uuid,
    pub created_at: DateTime,
    pub updated_at: DateTime,
    pub last_login_at: Option<DateTime>,
    // TODO: Add avatar_url field
    // TODO: Add mfa_enabled field
    // TODO: Add mfa_secret field (encrypted)
    // TODO: Add api_key_hash field
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // TODO: Define relation to Organization
    // TODO: Define relation to App (created_by)
    // TODO: Define relation to SecretVersion (created_by)
    // TODO: Define relation to AuditLog (has many)
}

impl ActiveModelBehavior for ActiveModel {
    // TODO: Implement before_save hook for password hashing
    // TODO: Implement after_save hook for user registration event
    // TODO: Implement before_update hook for email validation
}

// TODO: Implement conversion methods between ORM Model and core entity User
// impl From<Model> for shellwego_core::entities::user::User { ... }
// impl From<shellwego_core::entities::user::User> for ActiveModel { ... }

// TODO: Implement custom query methods
// impl Model {
//     pub async fn find_by_email(db: &DatabaseConnection, email: &str) -> Result<Option<Self>, DbErr> { ... }
//     pub async fn find_by_org(db: &DatabaseConnection, org_id: Uuid) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn update_last_login(db: &DatabaseConnection, user_id: Uuid) -> Result<Self, DbErr> { ... }
// }
