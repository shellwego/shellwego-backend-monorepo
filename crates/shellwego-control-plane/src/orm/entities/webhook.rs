//! Webhook entity using Sea-ORM
//!
//! Represents webhook endpoints for event notifications.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "webhooks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub events: Json, // TODO: Use Vec<WebhookEvent> with custom type
    pub secret: String, // HMAC secret for signature verification
    pub active: bool,
    pub organization_id: Uuid,
    pub created_at: DateTime,
    pub updated_at: DateTime,
    pub last_triggered_at: Option<DateTime>,
    pub last_success_at: Option<DateTime>,
    pub last_failure_at: Option<DateTime>,
    pub failure_count: u32,
    // TODO: Add headers field (custom HTTP headers)
    // TODO: Add retry_policy field
    // TODO: Add timeout_secs field
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // TODO: Define relation to Organization
    // TODO: Define relation to WebhookDelivery (has many)
}

impl ActiveModelBehavior for ActiveModel {
    // TODO: Implement before_save hook for URL validation
    // TODO: Implement after_save hook for webhook registration event
    // TODO: Implement before_update hook for secret rotation
}

// TODO: Implement conversion methods between ORM Model and core entity Webhook
// impl From<Model> for shellwego_core::entities::webhook::Webhook { ... }
// impl From<shellwego_core::entities::webhook::Webhook> for ActiveModel { ... }

// TODO: Implement custom query methods
// impl Model {
//     pub async fn find_by_org(db: &DatabaseConnection, org_id: Uuid) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_active(db: &DatabaseConnection, org_id: Uuid) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_by_event(db: &DatabaseConnection, event: WebhookEvent) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn update_triggered(db: &DatabaseConnection, webhook_id: Uuid, success: bool) -> Result<Self, DbErr> { ... }
// }
