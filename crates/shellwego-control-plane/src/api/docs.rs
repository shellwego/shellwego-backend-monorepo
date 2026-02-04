//! OpenAPI documentation generation
//! 
//! Uses utoipa to derive specs from our handler signatures.
//! Serves Swagger UI at /docs

use axum::Router;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use shellwego_core::entities::{
    app::{App, AppStatus, CreateAppRequest, UpdateAppRequest, AppInstance, InstanceStatus},
    node::{Node, NodeStatus, RegisterNodeRequest, NodeJoinResponse},
    volume::{Volume, VolumeStatus, CreateVolumeRequest},
    domain::{Domain, DomainStatus, CreateDomainRequest},
    database::{Database, DatabaseStatus, CreateDatabaseRequest},
    secret::{Secret, SecretScope, CreateSecretRequest},
};

/// Main OpenAPI spec generator
#[derive(OpenApi)]
#[openapi(
    info(
        title = "ShellWeGo Control Plane API",
        version = "v1.0.0-alpha.1",
        description = "REST API for managing Firecracker microVMs, volumes, domains, and databases",
        license(name = "AGPL-3.0", url = "https://www.gnu.org/licenses/agpl-3.0.html"),
    ),
    paths(
        // Apps
        crate::api::handlers::apps::list_apps,
        crate::api::handlers::apps::create_app,
        crate::api::handlers::apps::get_app,
        crate::api::handlers::apps::update_app,
        crate::api::handlers::apps::delete_app,
        // TODO: Add all other handlers here
    ),
    components(
        schemas(
            // App schemas
            App, AppStatus, CreateAppRequest, UpdateAppRequest, 
            AppInstance, InstanceStatus,
            // Node schemas
            Node, NodeStatus, RegisterNodeRequest, NodeJoinResponse,
            // Volume schemas
            Volume, VolumeStatus, CreateVolumeRequest,
            // Domain schemas
            Domain, DomainStatus, CreateDomainRequest,
            // Database schemas
            Database, DatabaseStatus, CreateDatabaseRequest,
            // Secret schemas
            Secret, SecretScope, CreateSecretRequest,
            // Common
            shellwego_core::entities::ResourceSpec,
            shellwego_core::entities::EnvVar,
        )
    ),
    tags(
        (name = "Apps", description = "Application lifecycle management"),
        (name = "Nodes", description = "Worker node management"),
        (name = "Volumes", description = "Persistent storage"),
        (name = "Domains", description = "TLS and routing configuration"),
        (name = "Databases", description = "Managed database instances"),
        (name = "Secrets", description = "Encrypted configuration"),
        (name = "Auth", description = "Authentication and authorization"),
    ),
)]
pub struct ApiDoc;

/// Mount Swagger UI router
pub fn swagger_ui() -> Router {
    Router::new()
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .route("/api-docs/openapi.json", axum::routing::get(openapi_json))
}

async fn openapi_json() -> impl axum::response::IntoResponse {
    axum::Json(ApiDoc::openapi())
}