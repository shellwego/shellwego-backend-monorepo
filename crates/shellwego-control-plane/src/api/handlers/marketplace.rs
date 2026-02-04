//! One-click app marketplace

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::state::AppState;

/// List marketplace apps
pub async fn list_apps(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Return curated list of one-click apps
    // TODO: Include logos, descriptions, categories
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Get app details
pub async fn get_app(
    State(state): State<Arc<AppState>>,
    Path(app_slug): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Return app manifest with config options
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Install app from marketplace
pub async fn install_app(
    State(state): State<Arc<AppState>>,
    Path(app_slug): Path<String>,
    // TODO: Json body with config values
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate config against schema
    // TODO: Create app with pre-configured settings
    // TODO: Deploy dependencies if needed
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Submit app to marketplace (vendor)
pub async fn submit_app(
    State(state): State<Arc<AppState>>,
    // TODO: Multipart form with manifest, logo, screenshots
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate manifest schema
    // TODO: Virus scan docker image
    // TODO: Queue for review
    Err(StatusCode::NOT_IMPLEMENTED)
}