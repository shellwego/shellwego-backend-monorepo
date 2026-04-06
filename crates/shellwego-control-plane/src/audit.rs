//! Audit logging service
//!
//! Provides structured audit logging for all secret access and sensitive operations.
//! Audit events are persisted to the database for compliance and security review.

use crate::orm::Database;
use crate::auth::CurrentUser;
use chrono::Utc;
use serde_json;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

/// Audit service for logging security-relevant events
pub struct AuditService {
    db: Arc<Database>,
}

impl AuditService {
    /// Create a new audit service
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Log an audit event to the database
    ///
    /// This method is designed to be non-failing for the caller:
    /// errors are logged as warnings but do not propagate.
    pub async fn log(
        &self,
        actor: &CurrentUser,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        changes: Option<serde_json::Value>,
    ) {
        let entry = serde_json::json!({
            "id": Uuid::new_v4().to_string(),
            "timestamp": Utc::now().to_rfc3339(),
            "org_id": actor.organization_id.map(|id| id.to_string()).unwrap_or_default(),
            "actor_id": actor.user_id.to_string(),
            "actor_type": "User",
            "actor_name": actor.username,
            "action": action,
            "resource_type": resource_type,
            "resource_id": resource_id,
            "changes": changes,
            "metadata": {
                "ip_address": null,
                "user_agent": null,
                "request_id": null,
            },
        });

        if let Err(e) = self.db.insert("audit_logs", &entry).await {
            warn!("Failed to write audit log: {}", e);
        }

        info!(
            "AUDIT: user={} action={} resource={}:{}",
            actor.username, action, resource_type, resource_id
        );
    }
}
