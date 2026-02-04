//! Organization entity using Sea-ORM
//!
//! Represents organizations that own resources.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organizations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub plan: String, // TODO: Use Plan enum with custom type
    pub settings: Json, // TODO: Use OrganizationSettings with custom type
    pub billing_email: String,
    pub created_at: DateTime,
    pub updated_at: DateTime,
    // TODO: Add stripe_customer_id field
    // TODO: Add trial_ends_at field
    // TODO: Add max_apps field
    // TODO: Add max_nodes field
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // TODO: Define relation to User (has many)
    // TODO: Define relation to App (has many)
    // TODO: Define relation to Node (has many)
    // TODO: Define relation to Database (has many)
    // TODO: Define relation to Domain (has many)
    // TODO: Define relation to Secret (has many)
    // TODO: Define relation to Volume (has many)
}

impl ActiveModelBehavior for ActiveModel {
    // TODO: Implement before_save hook for slug generation
    // TODO: Implement after_save hook for organization creation event
}

// TODO: Implement conversion methods between ORM Model and core entity Organization
// impl From<Model> for shellwego_core::entities::organization::Organization { ... }
// impl From<shellwego_core::entities::organization::Organization> for ActiveModel { ... }

// TODO: Implement custom query methods
// impl Model {
//     pub async fn find_by_slug(db: &DatabaseConnection, slug: &str) -> Result<Option<Self>, DbErr> { ... }
//     pub async fn find_by_plan(db: &DatabaseConnection, plan: Plan) -> Result<Vec<Self>, DbErr> { ... }
// }
