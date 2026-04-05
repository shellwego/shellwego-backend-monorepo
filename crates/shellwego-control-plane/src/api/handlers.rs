//! HTTP request handlers
//!
//! Types imported from shellwego-schema - single source of truth.
//! This module contains only handler logic and API-specific DTOs.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::state::AppState;
use crate::auth::{AuthError, CurrentUser};
use super::ErrorResponse;

// Import types from schema - single source of truth
use shellwego_schema::entities::ResourceRequest;
use shellwego_schema::api::ScaleRequest;
use shellwego_schema::api::responses::HealthResponse;
use shellwego_schema::api::pagination::PaginatedResponse;

// ==================== Health ====================

pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse::healthy(env!("CARGO_PKG_VERSION")))
}

pub async fn readiness_check(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check database connection
    if let Err(e) = state.db.health_check().await {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("SERVICE_UNAVAILABLE", &format!("Database unavailable: {}", e))),
        ));
    }

    Ok(Json(HealthResponse::healthy(env!("CARGO_PKG_VERSION"))))
}

// ==================== Apps ====================

/// App API response type (simplified view)
#[derive(Debug, Serialize, Deserialize)]
pub struct App {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub image: String,
    pub replicas: u32,
    pub region: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create app request
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAppRequest {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub replicas: u32,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub resources: Option<ResourceRequest>,
}

#[derive(Debug, Deserialize)]
pub struct ListAppsQuery {
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
    #[serde(default)]
    pub status: Option<String>,
}

fn default_per_page() -> u32 { 20 }

pub async fn list_apps(
    State(_state): State<Arc<AppState>>,
    Query(_params): Query<ListAppsQuery>,
) -> Json<PaginatedResponse<App>> {
    Json(PaginatedResponse::empty())
}

pub async fn create_app(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAppRequest>,
) -> Result<(StatusCode, Json<App>), (StatusCode, Json<ErrorResponse>)> {
    let app = App {
        id: Uuid::new_v4(),
        name: req.name,
        status: "creating".to_string(),
        image: req.image,
        replicas: req.replicas.max(1),
        region: state.config.default_region.clone(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    Ok((StatusCode::CREATED, Json(app)))
}

pub async fn get_app(
    State(_state): State<Arc<AppState>>,
    Path(_app_id): Path<Uuid>,
) -> Result<Json<App>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("App")),
    ))
}

pub async fn delete_app(
    State(_state): State<Arc<AppState>>,
    Path(_app_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("App")),
    ))
}

pub async fn deploy_app(
    State(_state): State<Arc<AppState>>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(serde_json::json!({
        "deployment_id": Uuid::new_v4(),
        "app_id": app_id,
        "status": "pending"
    })))
}

pub async fn scale_app(
    State(_state): State<Arc<AppState>>,
    Path(_app_id): Path<Uuid>,
    Json(_body): Json<ScaleRequest>,
) -> Result<Json<App>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("App")),
    ))
}

pub async fn restart_app(
    State(_state): State<Arc<AppState>>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(serde_json::json!({
        "status": "restarting",
        "app_id": app_id
    })))
}

pub async fn stop_app(
    State(_state): State<Arc<AppState>>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(serde_json::json!({
        "status": "stopped",
        "app_id": app_id
    })))
}

pub async fn start_app(
    State(_state): State<Arc<AppState>>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(serde_json::json!({
        "status": "starting",
        "app_id": app_id
    })))
}

pub async fn get_logs(
    State(_state): State<Arc<AppState>>,
    Path(_app_id): Path<Uuid>,
    Query(_params): Query<LogQuery>,
) -> Result<Json<Vec<LogEntry>>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(Vec::new()))
}

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    pub follow: Option<bool>,
    pub tail: Option<u32>,
    pub since: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub source: String,
}

// ==================== Nodes ====================

/// Node API response type
#[derive(Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub hostname: String,
    pub status: String,
    pub region: String,
    pub capacity: NodeCapacity,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeCapacity {
    pub cpu_cores: f64,
    pub memory_gb: u64,
    pub disk_gb: u64,
}

/// Register node request
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterNodeRequest {
    pub hostname: String,
    pub region: String,
    pub capacity: NodeCapacity,
}

#[derive(Debug, Deserialize)]
pub struct ListNodesQuery {
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

pub async fn list_nodes(
    State(state): State<Arc<AppState>>,
    Query(_params): Query<ListNodesQuery>,
) -> Json<PaginatedResponse<Node>> {
    let agents = state.list_agents();
    let nodes: Vec<Node> = agents.into_iter().map(|a| Node {
        id: a.node_id,
        hostname: a.hostname,
        status: "ready".to_string(),
        region: a.region,
        capacity: NodeCapacity {
            cpu_cores: 8.0,
            memory_gb: 32,
            disk_gb: 100,
        },
        created_at: a.connected_at,
    }).collect();

    Json(PaginatedResponse::new(nodes, None, false))
}

pub async fn register_node(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterNodeRequest>,
) -> Result<(StatusCode, Json<Node>), (StatusCode, Json<ErrorResponse>)> {
    let node_id = Uuid::new_v4();

    state.register_agent(node_id, req.hostname.clone(), req.region.clone());

    let node = Node {
        id: node_id,
        hostname: req.hostname,
        status: "ready".to_string(),
        region: req.region,
        capacity: req.capacity,
        created_at: Utc::now(),
    };

    Ok((StatusCode::CREATED, Json(node)))
}

pub async fn get_node(
    State(_state): State<Arc<AppState>>,
    Path(_node_id): Path<Uuid>,
) -> Result<Json<Node>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Node")),
    ))
}

pub async fn deregister_node(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state.deregister_agent(&node_id);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn drain_node(
    State(_state): State<Arc<AppState>>,
    Path(node_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(serde_json::json!({
        "status": "draining",
        "node_id": node_id
    })))
}

// ==================== Volumes ====================

/// Volume API response type
#[derive(Debug, Serialize, Deserialize)]
pub struct Volume {
    pub id: Uuid,
    pub name: String,
    pub size_gb: u32,
    pub status: String,
    pub attached_to: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Create volume request
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateVolumeRequest {
    pub name: String,
    pub size_gb: u32,
    #[serde(default)]
    pub encrypted: bool,
}

pub async fn list_volumes(
    State(_state): State<Arc<AppState>>,
) -> Json<PaginatedResponse<Volume>> {
    Json(PaginatedResponse::empty())
}

pub async fn create_volume(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CreateVolumeRequest>,
) -> Result<(StatusCode, Json<Volume>), (StatusCode, Json<ErrorResponse>)> {
    let volume = Volume {
        id: Uuid::new_v4(),
        name: req.name,
        size_gb: req.size_gb,
        status: "creating".to_string(),
        attached_to: None,
        created_at: Utc::now(),
    };

    Ok((StatusCode::CREATED, Json(volume)))
}

pub async fn get_volume(
    State(_state): State<Arc<AppState>>,
    Path(_volume_id): Path<Uuid>,
) -> Result<Json<Volume>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Volume")),
    ))
}

pub async fn delete_volume(
    State(_state): State<Arc<AppState>>,
    Path(_volume_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Volume")),
    ))
}

pub async fn attach_volume(
    State(_state): State<Arc<AppState>>,
    Path(_volume_id): Path<Uuid>,
    Json(_body): Json<AttachVolumeRequest>,
) -> Result<Json<Volume>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Volume")),
    ))
}

#[derive(Debug, Deserialize)]
pub struct AttachVolumeRequest {
    pub app_id: Uuid,
}

pub async fn detach_volume(
    State(_state): State<Arc<AppState>>,
    Path(_volume_id): Path<Uuid>,
) -> Result<Json<Volume>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Volume")),
    ))
}

pub async fn snapshot_volume(
    State(_state): State<Arc<AppState>>,
    Path(volume_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(serde_json::json!({
        "snapshot_id": Uuid::new_v4(),
        "volume_id": volume_id,
        "status": "creating"
    })))
}

// ==================== Domains ====================

/// Domain API response type
#[derive(Debug, Serialize, Deserialize)]
pub struct Domain {
    pub id: Uuid,
    pub hostname: String,
    pub status: String,
    pub tls_enabled: bool,
    pub created_at: DateTime<Utc>,
}

/// Create domain request
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDomainRequest {
    pub hostname: String,
    #[serde(default)]
    pub tls_enabled: bool,
}

pub async fn list_domains(
    State(_state): State<Arc<AppState>>,
) -> Json<PaginatedResponse<Domain>> {
    Json(PaginatedResponse::empty())
}

pub async fn create_domain(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CreateDomainRequest>,
) -> Result<(StatusCode, Json<Domain>), (StatusCode, Json<ErrorResponse>)> {
    let domain = Domain {
        id: Uuid::new_v4(),
        hostname: req.hostname,
        status: "pending".to_string(),
        tls_enabled: req.tls_enabled,
        created_at: Utc::now(),
    };

    Ok((StatusCode::CREATED, Json(domain)))
}

pub async fn get_domain(
    State(_state): State<Arc<AppState>>,
    Path(_domain_id): Path<Uuid>,
) -> Result<Json<Domain>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Domain")),
    ))
}

pub async fn delete_domain(
    State(_state): State<Arc<AppState>>,
    Path(_domain_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Domain")),
    ))
}

pub async fn verify_domain(
    State(_state): State<Arc<AppState>>,
    Path(domain_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(serde_json::json!({
        "status": "verified",
        "domain_id": domain_id
    })))
}

// ==================== Databases ====================

/// Database API response type
#[derive(Debug, Serialize, Deserialize)]
pub struct Database {
    pub id: Uuid,
    pub name: String,
    pub engine: String,
    pub status: String,
    pub connection_string: String,
    pub created_at: DateTime<Utc>,
}

/// Create database request
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDatabaseRequest {
    pub name: String,
    pub engine: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub size_gb: Option<u32>,
}

pub async fn list_databases(
    State(_state): State<Arc<AppState>>,
) -> Json<PaginatedResponse<Database>> {
    Json(PaginatedResponse::empty())
}

pub async fn create_database(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CreateDatabaseRequest>,
) -> Result<(StatusCode, Json<Database>), (StatusCode, Json<ErrorResponse>)> {
    let db = Database {
        id: Uuid::new_v4(),
        name: req.name,
        engine: req.engine,
        status: "creating".to_string(),
        connection_string: String::new(),
        created_at: Utc::now(),
    };

    Ok((StatusCode::CREATED, Json(db)))
}

pub async fn get_database(
    State(_state): State<Arc<AppState>>,
    Path(_db_id): Path<Uuid>,
) -> Result<Json<Database>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Database")),
    ))
}

pub async fn delete_database(
    State(_state): State<Arc<AppState>>,
    Path(_db_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Database")),
    ))
}

pub async fn backup_database(
    State(_state): State<Arc<AppState>>,
    Path(db_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(serde_json::json!({
        "backup_id": Uuid::new_v4(),
        "database_id": db_id,
        "status": "creating"
    })))
}

pub async fn restore_database(
    State(_state): State<Arc<AppState>>,
    Path(db_id): Path<Uuid>,
    Json(body): Json<RestoreDatabaseRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(serde_json::json!({
        "status": "restoring",
        "database_id": db_id,
        "backup_id": body.backup_id
    })))
}

#[derive(Debug, Deserialize)]
pub struct RestoreDatabaseRequest {
    pub backup_id: Uuid,
}

// ==================== Secrets ====================

/// Secret API response type (metadata only, no values)
#[derive(Debug, Serialize, Deserialize)]
pub struct Secret {
    pub id: Uuid,
    pub name: String,
    pub scope: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create secret request
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSecretRequest {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub scope: String,
}

pub async fn list_secrets(
    State(_state): State<Arc<AppState>>,
) -> Json<PaginatedResponse<Secret>> {
    Json(PaginatedResponse::empty())
}

pub async fn create_secret(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSecretRequest>,
) -> Result<(StatusCode, Json<Secret>), (StatusCode, Json<ErrorResponse>)> {
    // Encrypt the secret value using KMS
    let _encrypted = state.kms_client.encrypt(&req.name, &req.value).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("INTERNAL_ERROR", &e.to_string())),
        ))?;

    let secret = Secret {
        id: Uuid::new_v4(),
        name: req.name,
        scope: req.scope,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    Ok((StatusCode::CREATED, Json(secret)))
}

pub async fn get_secret(
    State(_state): State<Arc<AppState>>,
    Path(_secret_id): Path<Uuid>,
) -> Result<Json<Secret>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Secret")),
    ))
}

pub async fn delete_secret(
    State(_state): State<Arc<AppState>>,
    Path(_secret_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Secret")),
    ))
}

pub async fn rotate_secret(
    State(_state): State<Arc<AppState>>,
    Path(_secret_id): Path<Uuid>,
    Json(_body): Json<RotateSecretRequest>,
) -> Result<Json<Secret>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Secret")),
    ))
}

#[derive(Debug, Deserialize)]
pub struct RotateSecretRequest {
    pub value: String,
}

// ==================== Builds ====================

pub async fn list_builds(
    State(state): State<Arc<AppState>>,
) -> Json<PaginatedResponse<serde_json::Value>> {
    let stats = futures::executor::block_on(state.build_queue.get_stats());
    Json(PaginatedResponse::new(vec![
        serde_json::json!({
            "pending": stats.pending_count,
            "running": stats.running_count,
            "completed": stats.completed_count
        })
    ], None, false))
}

pub async fn get_build(
    State(_state): State<Arc<AppState>>,
    Path(_build_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Build")),
    ))
}

pub async fn get_build_logs(
    State(_state): State<Arc<AppState>>,
    Path(_build_id): Path<Uuid>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(Vec::new()))
}

pub async fn cancel_build(
    State(_state): State<Arc<AppState>>,
    Path(build_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(serde_json::json!({
        "status": "cancelled",
        "build_id": build_id
    })))
}

// ==================== Webhooks ====================

pub async fn github_webhook(
    State(_state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    _body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let event_type = headers.get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    Ok(Json(serde_json::json!({
        "status": "received",
        "event_type": event_type
    })))
}

pub async fn gitlab_webhook(
    State(_state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    _body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let event_type = headers.get("X-Gitlab-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    Ok(Json(serde_json::json!({
        "status": "received",
        "event_type": event_type
    })))
}

// ==================== Auth ====================

/// Token response
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub token_type: String,
}

/// Register request
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

/// Login request (reuses the CreateTokenRequest shape)
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// User info response
#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub organization_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// POST /v1/auth/register — register a new user
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<TokenResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Input validation
    if req.username.len() < 3 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::validation("Username must be at least 3 characters")),
        ));
    }
    if req.password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::validation("Password must be at least 8 characters")),
        ));
    }
    if !req.email.contains('@') || !req.email.contains('.') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::validation("Invalid email address")),
        ));
    }

    let result = state
        .auth_service
        .register(&req.username, &req.email, &req.password)
        .await
        .map_err(|e| auth_error_to_response(&e))?;

    Ok((
        StatusCode::CREATED,
        Json(TokenResponse {
            token: result.access_token,
            refresh_token: result.refresh_token,
            expires_in: result.expires_in,
            token_type: "Bearer".to_string(),
        }),
    ))
}

/// POST /v1/auth/token — login (kept as create_token for route compatibility)
pub async fn create_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<TokenResponse>, (StatusCode, Json<ErrorResponse>)> {
    let result = state
        .auth_service
        .login(&req.username, &req.password)
        .await
        .map_err(|e| auth_error_to_response(&e))?;

    Ok(Json(TokenResponse {
        token: result.access_token,
        refresh_token: result.refresh_token,
        expires_in: result.expires_in,
        token_type: "Bearer".to_string(),
    }))
}

/// POST /v1/auth/refresh — exchange refresh token for new access token
pub async fn refresh_token(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RefreshTokenRequest>,
) -> Result<Json<TokenResponse>, (StatusCode, Json<ErrorResponse>)> {
    let result = state
        .auth_service
        .refresh_token(&body.refresh_token)
        .await
        .map_err(|e| auth_error_to_response(&e))?;

    Ok(Json(TokenResponse {
        token: result.access_token,
        refresh_token: result.refresh_token,
        expires_in: result.expires_in,
        token_type: "Bearer".to_string(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

/// POST /v1/auth/logout — revoke the current token
pub async fn logout(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    if let Some(auth_header) = headers.get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                // Best-effort revocation
                let _ = state.auth_service.revoke_token(token).await;
            }
        }
    }

    Ok(Json(serde_json::json!({
        "status": "logged_out"
    })))
}

/// GET /v1/auth/me — get current user info (protected)
pub async fn get_me(
    State(state): State<Arc<AppState>>,
    current_user: axum::Extension<CurrentUser>,
) -> Result<Json<UserInfo>, (StatusCode, Json<ErrorResponse>)> {
    let user = state
        .auth_service
        .get_user(&current_user.user_id)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::not_found("User")),
            )
        })?;

    Ok(Json(UserInfo {
        id: user.id,
        username: user.username,
        email: user.email,
        role: user.role.to_string(),
        permissions: user.permissions,
        organization_id: user.organization_id,
        created_at: user.created_at,
    }))
}

/// Convert AuthError to HTTP response
fn auth_error_to_response(err: &AuthError) -> (StatusCode, Json<ErrorResponse>) {
    match err {
        AuthError::InvalidCredentials => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new(
                "INVALID_CREDENTIALS",
                "Invalid username or password",
            )),
        ),
        AuthError::UserAlreadyExists(username) => (
            StatusCode::CONFLICT,
            Json(ErrorResponse::new(
                "USER_EXISTS",
                &format!("User '{}' already exists", username),
            )),
        ),
        AuthError::UserNotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("USER_NOT_FOUND", "User not found")),
        ),
        AuthError::TokenExpired => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("TOKEN_EXPIRED", "Token has expired")),
        ),
        AuthError::InvalidToken(msg) => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("INVALID_TOKEN", msg)),
        ),
        AuthError::TokenRevoked => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("TOKEN_REVOKED", "Token has been revoked")),
        ),
        AuthError::InsufficientPermissions { required, have } => (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new(
                "FORBIDDEN",
                &format!(
                    "Insufficient permissions: required '{}', have '{}'",
                    required, have
                ),
            )),
        ),
        AuthError::InternalError(msg) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("INTERNAL_ERROR", msg)),
        ),
    }
}

// ==================== Organizations ====================

/// Organization API response type
#[derive(Debug, Serialize, Deserialize)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
}

/// Create organization request
#[derive(Debug, Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
}

pub async fn list_organizations(
    State(_state): State<Arc<AppState>>,
) -> Json<PaginatedResponse<Organization>> {
    Json(PaginatedResponse::empty())
}

pub async fn create_organization(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CreateOrganizationRequest>,
) -> Result<(StatusCode, Json<Organization>), (StatusCode, Json<ErrorResponse>)> {
    let name = req.name.clone();
    let org = Organization {
        id: Uuid::new_v4(),
        name,
        slug: req.name.to_lowercase().replace(' ', "-"),
        created_at: Utc::now(),
    };

    Ok((StatusCode::CREATED, Json(org)))
}

pub async fn get_organization(
    State(_state): State<Arc<AppState>>,
    Path(_org_id): Path<Uuid>,
) -> Result<Json<Organization>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Organization")),
    ))
}

// ==================== Metrics ====================

pub async fn get_metrics(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "agents": {
            "connected": state.agent_count()
        },
        "builds": {
            "stats": futures::executor::block_on(state.build_queue.get_stats())
        },
        "version": env!("CARGO_PKG_VERSION")
    }))
}

// ==================== Audit Logs ====================

/// Audit log response type
#[derive(Debug, Serialize, Deserialize)]
pub struct AuditLogResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub ip_address: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub details: Option<serde_json::Value>,
}

/// GET /v1/audit-logs — list audit logs (protected)
pub async fn list_audit_logs(
    State(_state): State<Arc<AppState>>,
) -> Json<PaginatedResponse<AuditLogResponse>> {
    // Audit logs will be persisted to DB in a future phase.
    // Currently returns empty — the structure is ready for Phase 4 integration.
    Json(PaginatedResponse::empty())
}
