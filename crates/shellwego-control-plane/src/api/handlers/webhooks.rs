//! Webhook management for external integrations

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::state::AppState;

/// Webhook subscription
pub async fn create_webhook(
    State(state): State<Arc<AppState>>,
    // TODO: Json body with url, events, secret
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate URL
    // TODO: Generate signing secret
    // TODO: Store subscription
    // TODO: Return 201 with webhook ID
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn list_webhooks(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, StatusCode> {
    // TODO: List all webhooks for current org
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn delete_webhook(
    State(state): State<Arc<AppState>>,
    Path(webhook_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Remove subscription
    // TODO: Cancel pending deliveries
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn test_webhook(
    State(state): State<Arc<AppState>>,
    Path(webhook_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Send test event to webhook URL
    // TODO: Return delivery result
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Webhook delivery logs
pub async fn get_webhook_deliveries(
    State(state): State<Arc<AppState>>,
    Path(webhook_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Return delivery history with status codes
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn redeliver_webhook(
    State(state): State<Arc<AppState>>,
    Path((webhook_id, delivery_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Retry specific delivery
    Err(StatusCode::NOT_IMPLEMENTED)
}