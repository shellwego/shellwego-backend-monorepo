//! HTTP request handlers
//!
//! Types imported from shellwego-schema - single source of truth.
//! This module contains only handler logic and API-specific DTOs.
//!
//! Phase 4: All CRUD handlers now use the real Database for persistence.

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use tracing::{error, warn};

use crate::state::AppState;
use crate::auth::{AuthError, CurrentUser};
use crate::orm::DatabaseError;
use super::ErrorResponse;

// Import types from schema - single source of truth
use shellwego_schema::entities::ResourceRequest;
use shellwego_schema::api::ScaleRequest;
use shellwego_schema::api::responses::HealthResponse;
use shellwego_schema::api::pagination::PaginatedResponse;

// ==================== Helpers ====================

/// Convert a DatabaseError into an appropriate HTTP error tuple
fn db_error(code: &str, err: &DatabaseError) -> (StatusCode, Json<ErrorResponse>) {
    error!("Database error [{}]: {}", code, err);
    match err {
        DatabaseError::NotFound => (StatusCode::NOT_FOUND, Json(ErrorResponse::not_found("Resource"))),
        DatabaseError::ConnectionError(msg) => (StatusCode::SERVICE_UNAVAILABLE, Json(ErrorResponse::new("DB_UNAVAILABLE", msg))),
        DatabaseError::QueryError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new("DB_ERROR", msg))),
        DatabaseError::MigrationError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new("DB_MIGRATION_ERROR", msg))),
        DatabaseError::NotConnected => (StatusCode::SERVICE_UNAVAILABLE, Json(ErrorResponse::new("DB_UNAVAILABLE", "Database not connected"))),
        DatabaseError::DuplicateKey => (StatusCode::CONFLICT, Json(ErrorResponse::new("DUPLICATE", "Duplicate key"))),
        DatabaseError::Timeout => (StatusCode::GATEWAY_TIMEOUT, Json(ErrorResponse::new("TIMEOUT", "Database timeout"))),
    }
}

fn internal_db_err(err: DatabaseError) -> (StatusCode, Json<ErrorResponse>) {
    db_error("DB_ERROR", &err)
}

fn not_found_err(resource: &str) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::NOT_FOUND, Json(ErrorResponse::not_found(resource)))
}

fn validation_err(msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::BAD_REQUEST, Json(ErrorResponse::validation(msg)))
}

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
    #[serde(default)]
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub replicas: u32,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub created_at: DateTime<Utc>,
    #[serde(default)]
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
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Query(params): Query<ListAppsQuery>,
) -> Result<Json<PaginatedResponse<App>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "apps:read").map_err(|(code, err)| (code, Json(err)))?;
    let per_page = params.per_page.clamp(1, 100);
    let page = params.page.max(1);
    let offset = (page - 1) * per_page;

    let total = state.db.count("apps").await.map_err(internal_db_err)?;

    let conditions: HashMap<String, String> = params.status
        .map(|s| [("status".to_string(), s)].into_iter().collect())
        .unwrap_or_default();

    let items: Vec<App> = state.db.query("apps", conditions, Some(per_page), Some(offset))
        .await
        .map_err(internal_db_err)?;

    let has_more = (offset as u64 + items.len() as u64) < total;
    Ok(Json(PaginatedResponse::new(items, None, has_more).with_total_count(total)))
}

pub async fn create_app(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<CreateAppRequest>,
) -> Result<(StatusCode, Json<App>), (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "apps:write").map_err(|(code, err)| (code, Json(err)))?;
    // Validation
    if req.name.len() < 3 || req.name.len() > 63 {
        return Err(validation_err("App name must be 3-63 characters"));
    }
    if !req.name.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return Err(validation_err("App name must be alphanumeric with hyphens"));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();
    let slug = req.name.to_lowercase().replace(' ', "-");

    let app_entity = serde_json::json!({
        "id": id,
        "name": req.name,
        "slug": slug,
        "status": "creating",
        "image": req.image,
        "command": null,
        "resources": req.resources,
        "env": req.env,
        "domains": [],
        "volumes": [],
        "health_check": null,
        "source": null,
        "organization_id": null,
        "created_by": null,
        "created_at": now.to_rfc3339(),
        "updated_at": now.to_rfc3339(),
    });

    state.db.insert("apps", &app_entity).await.map_err(internal_db_err)?;

    let app: Option<App> = state.db.find_by_id("apps", &id).await.map_err(internal_db_err)?;
    let app = app.ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new("INTERNAL_ERROR", "Failed to retrieve created app")))
    })?;

    Ok((StatusCode::CREATED, Json(app)))
}

pub async fn get_app(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<App>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "apps:read").map_err(|(code, err)| (code, Json(err)))?;
    let app: Option<App> = state.db.find_by_id("apps", &app_id).await.map_err(internal_db_err)?;
    app.map(Json).ok_or_else(|| not_found_err("App"))
}

pub async fn delete_app(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(app_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "apps:delete").map_err(|(code, err)| (code, Json(err)))?;
    state.db.delete("apps", &app_id).await.map_err(|e| {
        match e {
            DatabaseError::NotFound => not_found_err("App"),
            _ => internal_db_err(e),
        }
    })?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn deploy_app(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "apps:write").map_err(|(code, err)| (code, Json(err)))?;
    // Verify app exists first
    let _app: Option<App> = state.db.find_by_id("apps", &app_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("App"))?;

    let deployment_id = Uuid::new_v4();

    // Create a deployment record
    let deployment_entity = serde_json::json!({
        "id": deployment_id,
        "app_id": app_id,
        "build_id": null,
        "status": "pending",
        "strategy": "rolling",
        "started_at": Utc::now().to_rfc3339(),
        "finished_at": null,
        "previous_deployment": null,
    });

    state.db.insert("deployments", &deployment_entity).await.map_err(internal_db_err)?;

    Ok(Json(serde_json::json!({
        "deployment_id": deployment_id,
        "app_id": app_id,
        "status": "pending"
    })))
}

pub async fn scale_app(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<ScaleRequest>,
) -> Result<Json<App>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "apps:write").map_err(|(code, err)| (code, Json(err)))?;
    // Verify app exists
    let existing: Option<App> = state.db.find_by_id("apps", &app_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("App"))?;

    let existing = existing.unwrap();

    // Build update entity with the new replicas stored in resources
    let resources = serde_json::json!({
        "replicas": body.replicas,
    });

    let update_entity = serde_json::json!({
        "name": existing.name,
        "slug": "",
        "status": existing.status,
        "image": existing.image,
        "command": null,
        "resources": resources,
        "env": {},
        "domains": [],
        "volumes": [],
        "health_check": null,
        "source": null,
    });

    state.db.update("apps", &app_id, &update_entity).await.map_err(internal_db_err)?;

    // Fetch the updated app
    let app: Option<App> = state.db.find_by_id("apps", &app_id).await.map_err(internal_db_err)?;
    app.map(Json).ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new("INTERNAL_ERROR", "Failed to retrieve updated app")))
    })
}

pub async fn restart_app(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "apps:write").map_err(|(code, err)| (code, Json(err)))?;
    // Verify app exists
    let _app: Option<App> = state.db.find_by_id("apps", &app_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("App"))?;

    Ok(Json(serde_json::json!({
        "status": "restarting",
        "app_id": app_id
    })))
}

pub async fn stop_app(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "apps:write").map_err(|(code, err)| (code, Json(err)))?;
    // Verify app exists and update status
    let _app: Option<App> = state.db.find_by_id("apps", &app_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("App"))?;

    let update_entity = serde_json::json!({
        "name": "",
        "slug": "",
        "status": "stopped",
        "image": "",
        "command": null,
        "resources": null,
        "env": {},
        "domains": [],
        "volumes": [],
        "health_check": null,
        "source": null,
    });

    state.db.update("apps", &app_id, &update_entity).await.map_err(internal_db_err)?;

    Ok(Json(serde_json::json!({
        "status": "stopped",
        "app_id": app_id
    })))
}

pub async fn start_app(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "apps:write").map_err(|(code, err)| (code, Json(err)))?;
    let _app: Option<App> = state.db.find_by_id("apps", &app_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("App"))?;

    let update_entity = serde_json::json!({
        "name": "",
        "slug": "",
        "status": "running",
        "image": "",
        "command": null,
        "resources": null,
        "env": {},
        "domains": [],
        "volumes": [],
        "health_check": null,
        "source": null,
    });

    state.db.update("apps", &app_id, &update_entity).await.map_err(internal_db_err)?;

    Ok(Json(serde_json::json!({
        "status": "starting",
        "app_id": app_id
    })))
}

pub async fn get_logs(
    State(_state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(_app_id): Path<Uuid>,
    Query(_params): Query<LogQuery>,
) -> Result<Json<Vec<LogEntry>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "apps:read").map_err(|(code, err)| (code, Json(err)))?;
    // Log streaming from live containers is not yet connected to DB;
    // returns empty for now — real implementation would stream from agent.
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
    #[serde(default)]
    pub id: Uuid,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub capacity: NodeCapacity,
    #[serde(default)]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct NodeCapacity {
    #[serde(default)]
    pub cpu_cores: f64,
    #[serde(default)]
    pub memory_gb: u64,
    #[serde(default)]
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
    Extension(current_user): Extension<CurrentUser>,
    Query(_params): Query<ListNodesQuery>,
) -> Result<Json<PaginatedResponse<Node>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "nodes:read").map_err(|(code, err)| (code, Json(err)))?;
    // Merge in-memory agents with DB-persisted nodes
    let mut nodes: Vec<Node> = Vec::new();

    // In-memory live agents
    let agents = state.list_agents();
    for a in agents {
        nodes.push(Node {
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
        });
    }

    // DB-persisted nodes
    let db_nodes: Vec<Node> = state.db.find_all("nodes").await.unwrap_or_default();
    for db_node in db_nodes {
        // Deduplicate: skip if we already have a live agent with this ID
        if !nodes.iter().any(|n| n.id == db_node.id) {
            nodes.push(db_node);
        }
    }

    let total = nodes.len() as u64;
    Ok(Json(PaginatedResponse::new(nodes, None, false).with_total_count(total)))
}

pub async fn register_node(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<RegisterNodeRequest>,
) -> Result<(StatusCode, Json<Node>), (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "nodes:write").map_err(|(code, err)| (code, Json(err)))?;
    let node_id = Uuid::new_v4();

    // Register in memory for live agent tracking
    state.register_agent(node_id, req.hostname.clone(), req.region.clone());

    // Persist to database
    let now = Utc::now();
    let node_entity = serde_json::json!({
        "id": node_id,
        "hostname": req.hostname,
        "status": "ready",
        "region": req.region,
        "zone": "",
        "capacity": serde_json::json!({
            "cpu_cores": req.capacity.cpu_cores,
            "memory_gb": req.capacity.memory_gb,
            "disk_gb": req.capacity.disk_gb,
        }),
        "capabilities": [],
        "network": {},
        "labels": {},
        "running_apps": 0,
        "microvm_capacity": 0,
        "microvm_used": 0,
        "kernel_version": "",
        "firecracker_version": "",
        "agent_version": env!("CARGO_PKG_VERSION"),
        "last_seen": now.to_rfc3339(),
        "created_at": now.to_rfc3339(),
        "organization_id": null,
    });

    state.db.insert("nodes", &node_entity).await.map_err(internal_db_err)?;

    let node = Node {
        id: node_id,
        hostname: req.hostname,
        status: "ready".to_string(),
        region: req.region,
        capacity: req.capacity,
        created_at: now,
    };

    Ok((StatusCode::CREATED, Json(node)))
}

pub async fn get_node(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(node_id): Path<Uuid>,
) -> Result<Json<Node>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "nodes:read").map_err(|(code, err)| (code, Json(err)))?;
    // Check in-memory agents first
    if let Some(conn) = state.agents.get(&node_id) {
        let agent = conn.value().clone();
        return Ok(Json(Node {
            id: agent.node_id,
            hostname: agent.hostname,
            status: "ready".to_string(),
            region: agent.region,
            capacity: NodeCapacity {
                cpu_cores: 8.0,
                memory_gb: 32,
                disk_gb: 100,
            },
            created_at: agent.connected_at,
        }));
    }

    // Fall back to DB
    let node: Option<Node> = state.db.find_by_id("nodes", &node_id).await.map_err(internal_db_err)?;
    node.map(Json).ok_or_else(|| not_found_err("Node"))
}

pub async fn deregister_node(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(node_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "nodes:delete").map_err(|(code, err)| (code, Json(err)))?;
    // Remove from in-memory
    state.deregister_agent(&node_id);

    // Also attempt to remove from DB (best-effort — might not exist in DB)
    match state.db.delete("nodes", &node_id).await {
        Ok(()) | Err(DatabaseError::NotFound) => {}
        Err(e) => {
            warn!("Failed to delete node {} from DB: {}", node_id, e);
            return Err(internal_db_err(e));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn drain_node(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(node_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "nodes:write").map_err(|(code, err)| (code, Json(err)))?;
    // Verify node exists (in-memory or DB)
    if !state.agents.contains_key(&node_id) {
        let _node: Option<Node> = state.db.find_by_id("nodes", &node_id).await.map_err(internal_db_err)?
            .ok_or_else(|| not_found_err("Node"))?;
    }

    // Update node status in DB if present
    let update_entity = serde_json::json!({
        "hostname": "",
        "status": "draining",
        "region": "",
        "zone": "",
        "capacity": null,
        "capabilities": [],
        "network": {},
        "labels": {},
        "running_apps": 0,
        "microvm_capacity": 0,
        "microvm_used": 0,
        "kernel_version": "",
        "firecracker_version": "",
        "agent_version": "",
        "last_seen": Utc::now().to_rfc3339(),
    });

    match state.db.update("nodes", &node_id, &update_entity).await {
        Ok(()) | Err(DatabaseError::NotFound) => {}
        Err(e) => warn!("Failed to update node {} status in DB: {}", node_id, e),
    }

    Ok(Json(serde_json::json!({
        "status": "draining",
        "node_id": node_id
    })))
}

// ==================== Volumes ====================

/// Volume API response type
#[derive(Debug, Serialize, Deserialize)]
pub struct Volume {
    #[serde(default)]
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size_gb: u32,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub attached_to: Option<Uuid>,
    #[serde(default)]
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
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<PaginatedResponse<Volume>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "volumes:read").map_err(|(code, err)| (code, Json(err)))?;
    let items: Vec<Volume> = state.db.find_all("volumes").await.map_err(internal_db_err)?;
    let total = items.len() as u64;
    Ok(Json(PaginatedResponse::new(items, None, false).with_total_count(total)))
}

pub async fn create_volume(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<CreateVolumeRequest>,
) -> Result<(StatusCode, Json<Volume>), (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "volumes:write").map_err(|(code, err)| (code, Json(err)))?;
    // Validation
    if req.size_gb < 1 || req.size_gb > 10240 {
        return Err(validation_err("Volume size must be between 1 and 10240 GB"));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    let volume_entity = serde_json::json!({
        "id": id,
        "name": req.name,
        "status": "creating",
        "size_gb": req.size_gb,
        "used_gb": 0,
        "volume_type": "persistent",
        "filesystem": "ext4",
        "encrypted": req.encrypted,
        "encryption_key_id": null,
        "attached_to": null,
        "mount_path": null,
        "snapshots": [],
        "backup_policy": null,
        "organization_id": null,
        "created_at": now.to_rfc3339(),
        "updated_at": now.to_rfc3339(),
    });

    state.db.insert("volumes", &volume_entity).await.map_err(internal_db_err)?;

    // TODO(Plan 10): Dispatch provisioning command to agent via QUIC message bus.
    // The actual ZFS provisioning happens asynchronously on the agent node.
    // 1. Find available agent with sufficient storage capacity
    // 2. Send ProvisionVolume command with volume_id, size_gb, encrypted
    // 3. Agent provisions via VolumeProvisioner / ZfsManager
    // 4. Agent reports back with VolumeStatus::Attached or VolumeStatus::Error
    // For now, the volume remains in "creating" status until agent confirms.

    let volume: Option<Volume> = state.db.find_by_id("volumes", &id).await.map_err(internal_db_err)?
    let volume = volume.ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new("INTERNAL_ERROR", "Failed to retrieve created volume")))
    })?;

    Ok((StatusCode::CREATED, Json(volume)))
}

pub async fn get_volume(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(volume_id): Path<Uuid>,
) -> Result<Json<Volume>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "volumes:read").map_err(|(code, err)| (code, Json(err)))?;
    let volume: Option<Volume> = state.db.find_by_id("volumes", &volume_id).await.map_err(internal_db_err)?;
    volume.map(Json).ok_or_else(|| not_found_err("Volume"))
}

pub async fn delete_volume(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(volume_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "volumes:delete").map_err(|(code, err)| (code, Json(err)))?;
    state.db.delete("volumes", &volume_id).await.map_err(|e| {
        match e {
            DatabaseError::NotFound => not_found_err("Volume"),
            _ => internal_db_err(e),
        }
    })?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn attach_volume(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(volume_id): Path<Uuid>,
    Json(body): Json<AttachVolumeRequest>,
) -> Result<Json<Volume>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "volumes:write").map_err(|(code, err)| (code, Json(err)))?;
    // Verify volume exists
    let _existing: Option<Volume> = state.db.find_by_id("volumes", &volume_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("Volume"))?;

    // Update volume to attach
    let update_entity = serde_json::json!({
        "name": "",
        "status": "attached",
        "size_gb": 0,
        "used_gb": 0,
        "volume_type": "persistent",
        "filesystem": "ext4",
        "encrypted": false,
        "encryption_key_id": null,
        "attached_to": body.app_id.to_string(),
        "mount_path": null,
        "snapshots": [],
        "backup_policy": null,
    });

    state.db.update("volumes", &volume_id, &update_entity).await.map_err(internal_db_err)?;

    let volume: Option<Volume> = state.db.find_by_id("volumes", &volume_id).await.map_err(internal_db_err)?;
    volume.map(Json).ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new("INTERNAL_ERROR", "Failed to retrieve updated volume")))
    })
}

#[derive(Debug, Deserialize)]
pub struct AttachVolumeRequest {
    pub app_id: Uuid,
}

pub async fn detach_volume(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(volume_id): Path<Uuid>,
) -> Result<Json<Volume>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "volumes:write").map_err(|(code, err)| (code, Json(err)))?;
    let _existing: Option<Volume> = state.db.find_by_id("volumes", &volume_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("Volume"))?;

    let update_entity = serde_json::json!({
        "name": "",
        "status": "available",
        "size_gb": 0,
        "used_gb": 0,
        "volume_type": "persistent",
        "filesystem": "ext4",
        "encrypted": false,
        "encryption_key_id": null,
        "attached_to": null,
        "mount_path": null,
        "snapshots": [],
        "backup_policy": null,
    });

    state.db.update("volumes", &volume_id, &update_entity).await.map_err(internal_db_err)?;

    let volume: Option<Volume> = state.db.find_by_id("volumes", &volume_id).await.map_err(internal_db_err)?;
    volume.map(Json).ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new("INTERNAL_ERROR", "Failed to retrieve updated volume")))
    })
}

pub async fn snapshot_volume(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(volume_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "volumes:write").map_err(|(code, err)| (code, Json(err)))?;
    // Verify volume exists and is in a state that allows snapshotting
    let existing: Option<Volume> = state.db.find_by_id("volumes", &volume_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("Volume"))?;

    // TODO(Plan 10): Send snapshot command to the agent hosting this volume.
    // 1. Look up which agent is hosting this volume (from attached_to or scheduling metadata)
    // 2. Send SnapshotVolume command via QUIC message bus
    // 3. Agent creates ZFS snapshot via ZfsManager::snapshot_volume()
    // 4. Agent reports back with snapshot ID and size
    // For now, return a placeholder snapshot ID.

    let snapshot_id = Uuid::new_v4();
    Ok(Json(serde_json::json!({
        "snapshot_id": snapshot_id,
        "volume_id": volume_id,
        "volume_name": existing.name,
        "status": "creating"
    })))
}

// ==================== Domains ====================

/// Domain API response type
#[derive(Debug, Serialize, Deserialize)]
pub struct Domain {
    #[serde(default)]
    pub id: Uuid,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub tls_enabled: bool,
    #[serde(default)]
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
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<PaginatedResponse<Domain>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "domains:read").map_err(|(code, err)| (code, Json(err)))?;
    let items: Vec<Domain> = state.db.find_all("domains").await.map_err(internal_db_err)?;
    let total = items.len() as u64;
    Ok(Json(PaginatedResponse::new(items, None, false).with_total_count(total)))
}

pub async fn create_domain(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<CreateDomainRequest>,
) -> Result<(StatusCode, Json<Domain>), (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "domains:write").map_err(|(code, err)| (code, Json(err)))?;
    // Validation
    if req.hostname.trim().is_empty() {
        return Err(validation_err("Hostname must not be empty"));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    let domain_entity = serde_json::json!({
        "id": id,
        "hostname": req.hostname,
        "status": "pending",
        "tls_status": req.tls_enabled.then_some("pending").unwrap_or("none"),
        "certificate": null,
        "validation": null,
        "routing": {},
        "features": {},
        "organization_id": null,
        "created_at": now.to_rfc3339(),
        "updated_at": now.to_rfc3339(),
    });

    state.db.insert("domains", &domain_entity).await.map_err(internal_db_err)?;

    let domain: Option<Domain> = state.db.find_by_id("domains", &id).await.map_err(internal_db_err)?;
    let domain = domain.ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new("INTERNAL_ERROR", "Failed to retrieve created domain")))
    })?;

    Ok((StatusCode::CREATED, Json(domain)))
}

pub async fn get_domain(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(domain_id): Path<Uuid>,
) -> Result<Json<Domain>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "domains:read").map_err(|(code, err)| (code, Json(err)))?;
    let domain: Option<Domain> = state.db.find_by_id("domains", &domain_id).await.map_err(internal_db_err)?;
    domain.map(Json).ok_or_else(|| not_found_err("Domain"))
}

pub async fn delete_domain(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(domain_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "domains:delete").map_err(|(code, err)| (code, Json(err)))?;
    state.db.delete("domains", &domain_id).await.map_err(|e| {
        match e {
            DatabaseError::NotFound => not_found_err("Domain"),
            _ => internal_db_err(e),
        }
    })?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn verify_domain(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(domain_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "domains:write").map_err(|(code, err)| (code, Json(err)))?;
    let _existing: Option<Domain> = state.db.find_by_id("domains", &domain_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("Domain"))?;

    let update_entity = serde_json::json!({
        "hostname": "",
        "status": "verified",
        "tls_status": "none",
        "certificate": null,
        "validation": {},
        "routing": {},
        "features": {},
    });

    state.db.update("domains", &domain_id, &update_entity).await.map_err(internal_db_err)?;

    Ok(Json(serde_json::json!({
        "status": "verified",
        "domain_id": domain_id
    })))
}

// ==================== Databases ====================

/// Database API response type
#[derive(Debug, Serialize, Deserialize)]
pub struct Database {
    #[serde(default)]
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub connection_string: String,
    #[serde(default)]
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

const VALID_ENGINES: &[&str] = &["postgres", "mysql", "redis", "mongodb"];

pub async fn list_databases(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<PaginatedResponse<Database>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "databases:read").map_err(|(code, err)| (code, Json(err)))?;
    let items: Vec<Database> = state.db.find_all("managed_databases").await.map_err(internal_db_err)?;
    let total = items.len() as u64;
    Ok(Json(PaginatedResponse::new(items, None, false).with_total_count(total)))
}

pub async fn create_database(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<CreateDatabaseRequest>,
) -> Result<(StatusCode, Json<Database>), (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "databases:write").map_err(|(code, err)| (code, Json(err)))?;
    // Validation
    if !VALID_ENGINES.contains(&req.engine.to_lowercase().as_str()) {
        return Err(validation_err(&format!(
            "Invalid engine. Must be one of: {}",
            VALID_ENGINES.join(", ")
        )));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    let db_entity = serde_json::json!({
        "id": id,
        "name": req.name,
        "engine": req.engine.to_lowercase(),
        "version": req.version.unwrap_or_else(|| "15".to_string()),
        "status": "provisioning",
        "endpoint": null,
        "resources": serde_json::json!({
            "size_gb": req.size_gb.unwrap_or(10),
        }),
        "ha": { "enabled": false },
        "backup_config": null,
        "organization_id": null,
        "created_at": now.to_rfc3339(),
        "updated_at": now.to_rfc3339(),
    });

    state.db.insert("managed_databases", &db_entity).await.map_err(internal_db_err)?;

    let database: Option<Database> = state.db.find_by_id("managed_databases", &id).await.map_err(internal_db_err)?;
    let database = database.ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new("INTERNAL_ERROR", "Failed to retrieve created database")))
    })?;

    Ok((StatusCode::CREATED, Json(database)))
}

pub async fn get_database(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(db_id): Path<Uuid>,
) -> Result<Json<Database>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "databases:read").map_err(|(code, err)| (code, Json(err)))?;
    let database: Option<Database> = state.db.find_by_id("managed_databases", &db_id).await.map_err(internal_db_err)?;
    database.map(Json).ok_or_else(|| not_found_err("Database"))
}

pub async fn delete_database(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(db_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "databases:delete").map_err(|(code, err)| (code, Json(err)))?;
    state.db.delete("managed_databases", &db_id).await.map_err(|e| {
        match e {
            DatabaseError::NotFound => not_found_err("Database"),
            _ => internal_db_err(e),
        }
    })?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn backup_database(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(db_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "databases:write").map_err(|(code, err)| (code, Json(err)))?;
    let _existing: Option<Database> = state.db.find_by_id("managed_databases", &db_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("Database"))?;

    let backup_id = Uuid::new_v4();
    Ok(Json(serde_json::json!({
        "backup_id": backup_id,
        "database_id": db_id,
        "status": "creating"
    })))
}

pub async fn restore_database(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(db_id): Path<Uuid>,
    Json(body): Json<RestoreDatabaseRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "databases:write").map_err(|(code, err)| (code, Json(err)))?;
    let _existing: Option<Database> = state.db.find_by_id("managed_databases", &db_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("Database"))?;

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
    #[serde(default)]
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub created_at: DateTime<Utc>,
    #[serde(default)]
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
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<PaginatedResponse<Secret>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "secrets:read").map_err(|(code, err)| (code, Json(err)))?;
    let items: Vec<Secret> = state.db.find_all("secrets").await.map_err(internal_db_err)?;
    let total = items.len() as u64;
    // Audit log
    state.audit.log(
        &current_user, "secret.list", "secret", "", None
    ).await;
    Ok(Json(PaginatedResponse::new(items, None, false).with_total_count(total)))
}

pub async fn create_secret(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<CreateSecretRequest>,
) -> Result<(StatusCode, Json<Secret>), (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "secrets:write").map_err(|(code, err)| (code, Json(err)))?;
    // Encrypt the secret value using KMS
    let encrypted = state.kms_client.encrypt(&req.name, &req.value).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("KMS_ERROR", &format!("Encryption failed: {}", e))),
        ))?;

    let id = Uuid::new_v4();
    let now = Utc::now();
    let scope = if req.scope.is_empty() { "organization".to_string() } else { req.scope };

    let secret_entity = serde_json::json!({
        "id": id,
        "name": req.name,
        "scope": scope,
        "app_id": null,
        "current_version": 1,
        "versions": [],
        "last_used_at": null,
        "expires_at": null,
        "encrypted_value": encrypted,
        "key_id": null,
        "nonce": null,
        "organization_id": null,
        "created_at": now.to_rfc3339(),
        "updated_at": now.to_rfc3339(),
    });

    state.db.insert("secrets", &secret_entity).await.map_err(internal_db_err)?;

    let secret: Option<Secret> = state.db.find_by_id("secrets", &id).await.map_err(internal_db_err)?;
    let secret = secret.ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new("INTERNAL_ERROR", "Failed to retrieve created secret")))
    })?;

    // Audit log
    state.audit.log(
        &current_user, "secret.create", "secret", &id.to_string(), None
    ).await;

    Ok((StatusCode::CREATED, Json(secret)))
}

pub async fn get_secret(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(secret_id): Path<Uuid>,
) -> Result<Json<Secret>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "secrets:read").map_err(|(code, err)| (code, Json(err)))?;
    let secret: Option<Secret> = state.db.find_by_id("secrets", &secret_id).await.map_err(internal_db_err)?;
    let secret = secret.ok_or_else(|| not_found_err("Secret"))?;
    // Audit log
    state.audit.log(
        &current_user, "secret.read", "secret", &secret_id.to_string(), None
    ).await;
    Ok(Json(secret))
}

pub async fn delete_secret(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(secret_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "secrets:delete").map_err(|(code, err)| (code, Json(err)))?;
    state.db.delete("secrets", &secret_id).await.map_err(|e| {
        match e {
            DatabaseError::NotFound => not_found_err("Secret"),
            _ => internal_db_err(e),
        }
    })?;
    // Audit log
    state.audit.log(
        &current_user, "secret.delete", "secret", &secret_id.to_string(), None
    ).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn rotate_secret(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(secret_id): Path<Uuid>,
    Json(body): Json<RotateSecretRequest>,
) -> Result<Json<Secret>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "secrets:write").map_err(|(code, err)| (code, Json(err)))?;
    // Verify secret exists
    let existing: Option<Secret> = state.db.find_by_id("secrets", &secret_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("Secret"))?;
    let existing = existing.unwrap();

    // Encrypt the new value
    let encrypted = state.kms_client.encrypt(&existing.name, &body.value).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("KMS_ERROR", &format!("Encryption failed: {}", e))),
        ))?;

    let now = Utc::now();
    let update_entity = serde_json::json!({
        "name": existing.name,
        "scope": existing.scope,
        "app_id": null,
        "current_version": 2,
        "versions": [],
        "last_used_at": null,
        "expires_at": null,
        "encrypted_value": encrypted,
        "key_id": null,
        "nonce": null,
    });

    state.db.update("secrets", &secret_id, &update_entity).await.map_err(internal_db_err)?;

    // Audit log
    state.audit.log(
        &current_user, "secret.rotate", "secret", &secret_id.to_string(),
        Some(serde_json::json!({"new_version": 2}))
    ).await;

    let secret: Option<Secret> = state.db.find_by_id("secrets", &secret_id).await.map_err(internal_db_err)?;
    secret.map(Json).ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new("INTERNAL_ERROR", "Failed to retrieve updated secret")))
    })
}

#[derive(Debug, Deserialize)]
pub struct RotateSecretRequest {
    pub value: String,
}

// ==================== Builds ====================

pub async fn list_builds(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<PaginatedResponse<serde_json::Value>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "builds:read").map_err(|(code, err)| (code, Json(err)))?;
    let items: Vec<serde_json::Value> = state.db.find_all("builds").await.map_err(internal_db_err)?;
    let total = items.len() as u64;
    Ok(Json(PaginatedResponse::new(items, None, false).with_total_count(total)))
}

pub async fn get_build(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(build_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "builds:read").map_err(|(code, err)| (code, Json(err)))?;
    let build: Option<serde_json::Value> = state.db.find_by_id("builds", &build_id).await.map_err(internal_db_err)?;
    build.map(Json).ok_or_else(|| not_found_err("Build"))
}

pub async fn get_build_logs(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(build_id): Path<Uuid>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "builds:read").map_err(|(code, err)| (code, Json(err)))?;
    // Verify build exists
    let _build: Option<serde_json::Value> = state.db.find_by_id("builds", &build_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("Build"))?;

    // Build logs are streamed from the build system; return empty for now
    Ok(Json(Vec::new()))
}

pub async fn cancel_build(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(build_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "builds:write").map_err(|(code, err)| (code, Json(err)))?;
    let _build: Option<serde_json::Value> = state.db.find_by_id("builds", &build_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("Build"))?;

    Ok(Json(serde_json::json!({
        "status": "cancelled",
        "build_id": build_id
    })))
}

// ==================== Webhooks ====================

pub async fn github_webhook(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let event_type = headers.get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let payload: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::json!({}));

    let build_id = Uuid::new_v4();
    let now = Utc::now();

    // Extract repo info from payload
    let repo_name = payload.pointer("/repository/full_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let commit_sha = payload.pointer("/after")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let build_entity = serde_json::json!({
        "id": build_id,
        "app_id": null,
        "status": "pending",
        "source": serde_json::json!({
            "provider": "github",
            "event": event_type,
            "repository": repo_name,
            "commit": commit_sha,
        }),
        "image_reference": null,
        "started_at": null,
        "finished_at": null,
        "logs_url": null,
        "triggered_by": format!("github-webhook:{}", event_type),
    });

    state.db.insert("builds", &build_entity).await.map_err(internal_db_err)?;

    Ok(Json(serde_json::json!({
        "status": "received",
        "event_type": event_type,
        "build_id": build_id,
        "repository": repo_name
    })))
}

pub async fn gitlab_webhook(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let event_type = headers.get("X-Gitlab-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let payload: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::json!({}));

    let build_id = Uuid::new_v4();
    let now = Utc::now();

    // Extract repo info from payload
    let repo_name = payload.pointer("/project/path_with_namespace")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let commit_sha = payload.pointer("/after")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let build_entity = serde_json::json!({
        "id": build_id,
        "app_id": null,
        "status": "pending",
        "source": serde_json::json!({
            "provider": "gitlab",
            "event": event_type,
            "repository": repo_name,
            "commit": commit_sha,
        }),
        "image_reference": null,
        "started_at": null,
        "finished_at": null,
        "logs_url": null,
        "triggered_by": format!("gitlab-webhook:{}", event_type),
    });

    state.db.insert("builds", &build_entity).await.map_err(internal_db_err)?;

    Ok(Json(serde_json::json!({
        "status": "received",
        "event_type": event_type,
        "build_id": build_id,
        "repository": repo_name
    })))
}

// ==================== WebSocket Logs Stub ====================

/// WebSocket log streaming endpoint (stub — not yet implemented)
pub async fn ws_logs(
    State(_state): State<Arc<AppState>>,
    Path(_app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Stub: in a future phase this will be upgraded to a WebSocket connection
    // using axum::extract::ws. For now returns a placeholder.
    Ok(Json(serde_json::json!({
        "message": "WebSocket log streaming not yet implemented. Use GET /apps/{app_id}/logs instead.",
        "status": "not_implemented"
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
    #[serde(default)]
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub created_at: DateTime<Utc>,
}

/// Create organization request
#[derive(Debug, Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
}

pub async fn list_organizations(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<PaginatedResponse<Organization>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "organizations:read").map_err(|(code, err)| (code, Json(err)))?;
    let items: Vec<Organization> = state.db.find_all("organizations").await.map_err(internal_db_err)?;
    let total = items.len() as u64;
    Ok(Json(PaginatedResponse::new(items, None, false).with_total_count(total)))
}

pub async fn create_organization(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<CreateOrganizationRequest>,
) -> Result<(StatusCode, Json<Organization>), (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "organizations:write").map_err(|(code, err)| (code, Json(err)))?;
    // Validation
    if req.name.len() < 2 || req.name.len() > 100 {
        return Err(validation_err("Organization name must be 2-100 characters"));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();
    let slug = req.name.to_lowercase().replace(' ', "-");

    let org_entity = serde_json::json!({
        "id": id,
        "name": req.name,
        "slug": slug,
        "plan": "free",
        "settings": {},
        "created_at": now.to_rfc3339(),
        "updated_at": now.to_rfc3339(),
    });

    state.db.insert("organizations", &org_entity).await.map_err(internal_db_err)?;

    let org: Option<Organization> = state.db.find_by_id("organizations", &id).await.map_err(internal_db_err)?;
    let org = org.ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new("INTERNAL_ERROR", "Failed to retrieve created organization")))
    })?;

    Ok((StatusCode::CREATED, Json(org)))
}

pub async fn get_organization(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<Organization>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "organizations:read").map_err(|(code, err)| (code, Json(err)))?;
    let org: Option<Organization> = state.db.find_by_id("organizations", &org_id).await.map_err(internal_db_err)?;
    org.map(Json).ok_or_else(|| not_found_err("Organization"))
}

// ==================== Metrics ====================

pub async fn get_metrics(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "apps:read").map_err(|(code, err)| (code, Json(err)))?;
    let apps_count = state.db.count("apps").await.unwrap_or(0);
    let nodes_count = state.db.count("nodes").await.unwrap_or(0);
    let builds_count = state.db.count("builds").await.unwrap_or(0);

    Ok(Json(serde_json::json!({
        "agents": {
            "connected": state.agent_count()
        },
        "builds": {
            "stats": futures::executor::block_on(state.build_queue.get_stats())
        },
        "database": {
            "apps": apps_count,
            "nodes": nodes_count,
            "builds": builds_count,
        },
        "version": env!("CARGO_PKG_VERSION")
    })))
}

// ==================== Audit Logs ====================

/// Audit log response type
#[derive(Debug, Serialize, Deserialize)]
pub struct AuditLogResponse {
    #[serde(default)]
    pub id: Uuid,
    #[serde(default)]
    pub user_id: Uuid,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub resource_type: String,
    #[serde(default)]
    pub resource_id: String,
    #[serde(default)]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub details: Option<serde_json::Value>,
}

/// GET /v1/audit-logs — list audit logs (protected)
pub async fn list_audit_logs(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<PaginatedResponse<serde_json::Value>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "audit:read").map_err(|(code, err)| (code, Json(err)))?;
    let items: Vec<serde_json::Value> = state.db.find_all("audit_logs").await
        .map_err(internal_db_err)?;
    let total = items.len() as u64;
    Ok(Json(PaginatedResponse::new(items, None, false).with_total_count(total)))
}
