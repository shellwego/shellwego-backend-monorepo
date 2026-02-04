//! Health check endpoint (no auth required)

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde_json::json;
use std::sync::Arc;

use crate::state::AppState;

pub async fn health_check(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // TODO: Check DB connectivity
    // TODO: Check QUIC connectivity
    // TODO: Check disk space
    
    Ok(Json(json!({
        "status": "healthy",
        "version": env!("CARGO_PKG_VERSION"),
        // "database": "ok",
        // "quic": "ok",
    })))
}