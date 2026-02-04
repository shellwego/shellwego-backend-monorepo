//! HTTP API layer
//!
//! Route definitions, middleware stack, and handler dispatch.
//! All business logic lives in `services/`, this is just the HTTP glue.

use std::sync::Arc;

use axum::{
    routing::{get, post, patch, delete},
    Router,
};

use tower_http::{
    cors::CorsLayer,
    trace::TraceLayer,
    compression::CompressionLayer,
};

use crate::state::AppState;

mod docs;
pub mod handlers;

use handlers::{
    apps, auth, domains, nodes, volumes, databases, secrets, health,
};

/// Create the complete API router with all routes and middleware
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // API routes
        .nest("/v1", api_routes())
        // Health check (no auth)
        .route("/health", get(health::health_check))
        // OpenAPI docs
        .merge(docs::swagger_ui())
        // ReDoc alternative
        .merge(docs::redoc_ui())
        // OpenAPI JSON endpoint
        .route("/api-docs/openapi.json", get(docs::openapi_json))
        // Middleware stack
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// API v1 routes
fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Apps
        .route("/apps", get(apps::list_apps).post(apps::create_app))
        .route(
            "/apps/:app_id",
            get(apps::get_app)
                .patch(apps::update_app)
                .delete(apps::delete_app),
        )
        .route("/apps/:app_id/actions/start", post(apps::start_app))
        .route("/apps/:app_id/actions/stop", post(apps::stop_app))
        .route("/apps/:app_id/actions/restart", post(apps::restart_app))
        .route("/apps/:app_id/scale", post(apps::scale_app))
        .route("/apps/:app_id/deploy", post(apps::deploy_app))
        .route("/apps/:app_id/logs", get(apps::get_logs))
        .route("/apps/:app_id/metrics", get(apps::get_metrics))
        .route("/apps/:app_id/exec", post(apps::exec_command))
        .route("/apps/:app_id/deployments", get(apps::list_deployments))
        
        // Nodes
        .route("/nodes", get(nodes::list_nodes).post(nodes::register_node))
        .route(
            "/nodes/:node_id",
            get(nodes::get_node)
                .patch(nodes::update_node)
                .delete(nodes::delete_node),
        )
        .route("/nodes/:node_id/actions/drain", post(nodes::drain_node))
        
        // Volumes
        .route("/volumes", get(volumes::list_volumes).post(volumes::create_volume))
        .route(
            "/volumes/:volume_id",
            get(volumes::get_volume)
                .delete(volumes::delete_volume),
        )
        .route("/volumes/:volume_id/attach", post(volumes::attach_volume))
        .route("/volumes/:volume_id/detach", post(volumes::detach_volume))
        .route("/volumes/:volume_id/snapshots", post(volumes::create_snapshot))
        .route("/volumes/:volume_id/restore", post(volumes::restore_snapshot))
        
        // Domains
        .route("/domains", get(domains::list_domains).post(domains::create_domain))
        .route(
            "/domains/:domain_id",
            get(domains::get_domain)
                .delete(domains::delete_domain),
        )
        .route("/domains/:domain_id/certificate", post(domains::upload_certificate))
        .route("/domains/:domain_id/actions/validate", post(domains::validate_dns))
        
        // Databases
        .route("/databases", get(databases::list_databases).post(databases::create_database))
        .route(
            "/databases/:db_id",
            get(databases::get_database)
                .delete(databases::delete_database),
        )
        .route("/databases/:db_id/connection", get(databases::get_connection_string))
        .route("/databases/:db_id/backups", post(databases::create_backup))
        .route("/databases/:db_id/restore", post(databases::restore_backup))
        
        // Secrets
        .route("/secrets", get(secrets::list_secrets).post(secrets::create_secret))
        .route(
            "/secrets/:secret_id",
            get(secrets::get_secret)
                .delete(secrets::delete_secret),
        )
        .route("/secrets/:secret_id/versions", post(secrets::rotate_secret))
        
        // Auth
        .route("/auth/token", post(auth::create_token))
        .route("/auth/token/:token_id", delete(auth::revoke_token))
        .route("/user", get(auth::get_current_user))
        .route("/user/tokens", get(auth::list_tokens).post(auth::generate_api_token))
        .route("/user/tokens/:token_id", delete(auth::revoke_api_token))
}
