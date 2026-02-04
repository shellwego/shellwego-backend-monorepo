//! Audit log entities

use crate::prelude::*;

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: uuid::Uuid,
    pub timestamp: DateTime<Utc>,
    pub org_id: Option<uuid::Uuid>,
    pub actor_id: uuid::Uuid,
    pub actor_type: ActorType,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub changes: Option<serde_json::Value>, // Before/after
    pub metadata: AuditMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorType {
    User,
    ApiKey,
    System,
    Webhook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditMetadata {
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
}