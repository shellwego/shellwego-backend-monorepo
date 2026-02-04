//! OpenAPI documentation generation using aide
//!
//! Uses aide for derive-free OpenAPI generation with axum.
//! Serves Swagger/ReDoc UI at /docs

use std::sync::Arc;

use aide::openapi::{OpenApi, Info, Contact, License, Tag, Schema};
use aide::schemars;
use axum::{
    routing::get,
    response::IntoResponse,
    Json,
    Router,
    extract::Path,
};

use shellwego_core::entities::{
    app::{App, AppStatus, CreateAppRequest, UpdateAppRequest, AppInstance, InstanceStatus},
    node::{Node, NodeStatus, RegisterNodeRequest, NodeJoinResponse},
    volume::{Volume, VolumeStatus, CreateVolumeRequest},
    domain::{Domain, DomainStatus, CreateDomainRequest},
    database::{Database, DatabaseStatus, CreateDatabaseRequest},
    secret::{Secret, SecretScope, CreateSecretRequest},
};

/// Main OpenAPI spec generator using aide
pub fn api_docs() -> OpenApi {
    let mut api = OpenApi::default();
    
    // Set API info
    api.info = Box::new(Info {
        title: "ShellWeGo Control Plane API".to_string(),
        version: "v1.0.0-alpha.1".to_string(),
        description: Some("REST API for managing Firecracker microVMs, volumes, domains, and databases".to_string()),
        terms_of_service: None,
        contact: Some(Contact {
            name: Some("ShellWeGo Team".to_string()),
            email: Some("dev@shellwego.com".to_string()),
            url: Some("https://shellwego.com".to_string()),
        }),
        license: Some(License {
            name: "AGPL-3.0".to_string(),
            url: Some("https://www.gnu.org/licenses/agpl-3.0.html".to_string()),
        }),
        ..Default::default()
    });
    
    // Register schemas
    register_schemas(&mut api);
    
    // Add tags
    add_tags(&mut api);
    
    api
}

/// Register all entity schemas with OpenAPI
fn register_schemas(api: &mut OpenApi) {
    // Register App schemas
    api.schema_registry_mut().register("App", <App as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("AppStatus", <AppStatus as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("CreateAppRequest", <CreateAppRequest as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("UpdateAppRequest", <UpdateAppRequest as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("AppInstance", <AppInstance as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("InstanceStatus", <InstanceStatus as schemars::JsonSchema>::schema());
    
    // Register Node schemas
    api.schema_registry_mut().register("Node", <Node as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("NodeStatus", <NodeStatus as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("RegisterNodeRequest", <RegisterNodeRequest as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("NodeJoinResponse", <NodeJoinResponse as schemars::JsonSchema>::schema());
    
    // Register Volume schemas
    api.schema_registry_mut().register("Volume", <Volume as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("VolumeStatus", <VolumeStatus as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("CreateVolumeRequest", <CreateVolumeRequest as schemars::JsonSchema>::schema());
    
    // Register Domain schemas
    api.schema_registry_mut().register("Domain", <Domain as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("DomainStatus", <DomainStatus as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("CreateDomainRequest", <CreateDomainRequest as schemars::JsonSchema>::schema());
    
    // Register Database schemas
    api.schema_registry_mut().register("Database", <Database as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("DatabaseStatus", <DatabaseStatus as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("CreateDatabaseRequest", <CreateDatabaseRequest as schemars::JsonSchema>::schema());
    
    // Register Secret schemas
    api.schema_registry_mut().register("Secret", <Secret as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("SecretScope", <SecretScope as schemars::JsonSchema>::schema());
    api.schema_registry_mut().register("CreateSecretRequest", <CreateSecretRequest as schemars::JsonSchema>::schema());
}

/// Add API tags
fn add_tags(api: &mut OpenApi) {
    let _ = api.tags.insert("Apps".to_string(), Tag {
        name: "Apps".to_string(),
        description: Some("Application lifecycle management".to_string()),
        external_docs: None,
    });
    let _ = api.tags.insert("Nodes".to_string(), Tag {
        name: "Nodes".to_string(),
        description: Some("Worker node management".to_string()),
        external_docs: None,
    });
    let _ = api.tags.insert("Volumes".to_string(), Tag {
        name: "Volumes".to_string(),
        description: Some("Persistent storage".to_string()),
        external_docs: None,
    });
    let _ = api.tags.insert("Domains".to_string(), Tag {
        name: "Domains".to_string(),
        description: Some("TLS and routing configuration".to_string()),
        external_docs: None,
    });
    let _ = api.tags.insert("Databases".to_string(), Tag {
        name: "Databases".to_string(),
        description: Some("Managed database instances".to_string()),
        external_docs: None,
    });
    let _ = api.tags.insert("Secrets".to_string(), Tag {
        name: "Secrets".to_string(),
        description: Some("Encrypted configuration".to_string()),
        external_docs: None,
    });
    let _ = api.tags.insert("Auth".to_string(), Tag {
        name: "Auth".to_string(),
        description: Some("Authentication and authorization".to_string()),
        external_docs: None,
    });
}

/// Mount Swagger UI router (serves static files from CDN)
pub fn swagger_ui() -> Router {
    Router::new()
        .route("/swagger-ui", get(swagger_ui_index))
        .route("/swagger-ui/*path", get(swagger_ui_static))
}

/// Swagger UI index HTML
async fn swagger_ui_index() -> impl IntoResponse {
    let html = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>ShellWeGo API - Swagger UI</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css">
    <link rel="icon" type="image/png" href="/swagger-ui/favicon-32x32.png" sizes="32x32">
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script>
        window.onload = function() {
            SwaggerUIBundle({
                url: '/api-docs/openapi.json',
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [
                    SwaggerUIBundle.presets.apis,
                    SwaggerUIBundle.SwaggerUIStandalonePreset
                ],
            });
        };
    </script>
</body>
</html>
"#;
    axum::response::Html(html)
}

/// Swagger UI static asset proxy
async fn swagger_ui_static(Path(path): Path<String>) -> impl IntoResponse {
    let client = reqwest::Client::new();
    let url = format!("https://unpkg.com/swagger-ui-dist@5/{}", path);
    let response = client.get(url).send().await;
    match response {
        Ok(res) => {
            let body = res.bytes().await.unwrap_or_default();
            let content_type = res
                .headers()
                .get("content-type")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("application/octet-stream");
            ([("content-type", content_type)], body)
        }
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "Not found".as_bytes().to_vec()),
    }
}

/// Mount ReDoc router
pub fn redoc_ui() -> Router {
    Router::new()
        .route("/redoc", get(redoc_index))
}

/// ReDoc index HTML
async fn redoc_index() -> impl IntoResponse {
    let html = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>ShellWeGo API - ReDoc</title>
    <link rel="stylesheet" href="https://unpkg.com/redoc@latest/bundles/redoc.standalone.css">
</head>
<body>
    <redoc spec-url='/api-docs/openapi.json'></redoc>
    <script src="https://unpkg.com/redoc@latest/bundles/redoc.standalone.js"></script>
</body>
</html>
"#;
    axum::response::Html(html)
}

/// Generate OpenAPI JSON response
pub async fn openapi_json() -> impl IntoResponse {
    let api = api_docs();
    Json(api)
}
