//! HTTP API layer
//!
//! Route definitions, middleware stack, and handler dispatch.

use std::sync::Arc;
use axum::{
    routing::{get, post, put, patch, delete},
    Router,
    Json,
    http::StatusCode,
    extract::Path,
};
use tower_http::{
    cors::CorsLayer,
    trace::TraceLayer,
    compression::CompressionLayer,
    request_id::RequestIdLayer,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

mod handlers;
mod middleware;
mod response;

pub use handlers::*;
pub use response::{ApiResponse, ErrorResponse, ListResponse};

/// Create the complete API router with all routes and middleware
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Health checks (no auth required)
        .route("/health", get(handlers::health_check))
        .route("/healthz", get(handlers::health_check))
        .route("/readyz", get(handlers::readiness_check))
        
        // API v1 routes
        .nest("/v1", v1_routes())
        
        // API v1 routes (backward compat)
        .nest("/api/v1", v1_routes())
        
        // Middleware stack
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .layer(RequestIdLayer::new())
        .with_state(state)
}

/// API v1 routes
fn v1_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Apps
        .route("/apps", get(handlers::list_apps).post(handlers::create_app))
        .route("/apps/{app_id}", get(handlers::get_app).delete(handlers::delete_app))
        .route("/apps/{app_id}/deploy", post(handlers::deploy_app))
        .route("/apps/{app_id}/scale", post(handlers::scale_app))
        .route("/apps/{app_id}/logs", get(handlers::get_logs))
        .route("/apps/{app_id}/restart", post(handlers::restart_app))
        .route("/apps/{app_id}/stop", post(handlers::stop_app))
        .route("/apps/{app_id}/start", post(handlers::start_app))
        
        // Nodes
        .route("/nodes", get(handlers::list_nodes).post(handlers::register_node))
        .route("/nodes/{node_id}", get(handlers::get_node).delete(handlers::deregister_node))
        .route("/nodes/{node_id}/drain", post(handlers::drain_node))
        
        // Volumes
        .route("/volumes", get(handlers::list_volumes).post(handlers::create_volume))
        .route("/volumes/{volume_id}", get(handlers::get_volume).delete(handlers::delete_volume))
        .route("/volumes/{volume_id}/attach", post(handlers::attach_volume))
        .route("/volumes/{volume_id}/detach", post(handlers::detach_volume))
        .route("/volumes/{volume_id}/snapshot", post(handlers::snapshot_volume))
        
        // Domains
        .route("/domains", get(handlers::list_domains).post(handlers::create_domain))
        .route("/domains/{domain_id}", get(handlers::get_domain).delete(handlers::delete_domain))
        .route("/domains/{domain_id}/verify", post(handlers::verify_domain))
        
        // Databases
        .route("/databases", get(handlers::list_databases).post(handlers::create_database))
        .route("/databases/{db_id}", get(handlers::get_database).delete(handlers::delete_database))
        .route("/databases/{db_id}/backup", post(handlers::backup_database))
        .route("/databases/{db_id}/restore", post(handlers::restore_database))
        
        // Secrets
        .route("/secrets", get(handlers::list_secrets).post(handlers::create_secret))
        .route("/secrets/{secret_id}", get(handlers::get_secret).delete(handlers::delete_secret))
        .route("/secrets/{secret_id}/rotate", post(handlers::rotate_secret))
        
        // Builds
        .route("/builds", get(handlers::list_builds))
        .route("/builds/{build_id}", get(handlers::get_build))
        .route("/builds/{build_id}/logs", get(handlers::get_build_logs))
        .route("/builds/{build_id}/cancel", post(handlers::cancel_build))
        
        // Git webhooks
        .route("/webhooks/github", post(handlers::github_webhook))
        .route("/webhooks/gitlab", post(handlers::gitlab_webhook))
        
        // Auth
        .route("/auth/token", post(handlers::create_token))
        .route("/auth/refresh", post(handlers::refresh_token))
        .route("/auth/logout", post(handlers::logout))
        
        // Organizations
        .route("/organizations", get(handlers::list_organizations).post(handlers::create_organization))
        .route("/organizations/{org_id}", get(handlers::get_organization))
        
        // Metrics
        .route("/metrics", get(handlers::get_metrics))
}
