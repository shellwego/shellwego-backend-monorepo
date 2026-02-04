//! Webhook subscription and delivery entities

use crate::prelude::*;

/// Webhook subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id: uuid::Uuid,
    pub org_id: uuid::Uuid,
    pub url: String,
    pub secret: String, // For HMAC signature
    pub events: Vec<String>, // e.g., ["app.deployed", "app.crashed"]
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub last_delivered_at: Option<DateTime<Utc>>,
    pub failure_count: u32,
}

/// Webhook delivery attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: uuid::Uuid,
    pub webhook_id: uuid::Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub status_code: Option<u16>,
    pub response_body: Option<String>,
    pub delivered_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub success: bool,
}

/// Webhook event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebhookEventType {
    AppCreated,
    AppDeployed,
    AppCrashed,
    AppScaled,
    BuildCompleted,
    BuildFailed,
    // TODO: Add more event types
}