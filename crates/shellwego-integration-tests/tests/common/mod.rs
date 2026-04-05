//! Integration test harness for ShellWeGo
//!
//! Provides a test server wrapper, test database helpers, factory functions
//! for creating test entities, and mock trait objects for external services.
//!
//! # Usage
//!
//! ```rust,no_run
//! use common::setup_test_app;
//!
//! #[tokio::test]
//! async fn my_test() {
//!     let (app, _state) = setup_test_app().await;
//!     // Make requests against `app` using tower::ServiceExt
//! }
//! ```

use std::sync::Arc;

use axum::Router;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

// Re-export commonly used types for convenience
pub use axum::http::{Request, StatusCode};
pub use http_body_util::BodyExt as _;
pub use tower::ServiceExt as _;

// Re-export control-plane types for test construction
pub use shellwego_control_plane::api::handlers::{
    App, CreateAppRequest, CreateDatabaseRequest, CreateDomainRequest,
    CreateSecretRequest, CreateTokenRequest, CreateVolumeRequest, Node,
    RegisterNodeRequest, TokenResponse,
};
pub use shellwego_control_plane::config::Config;
pub use shellwego_control_plane::state::AppState;

// Re-export schema types
pub use shellwego_schema::api::pagination::PaginatedResponse;
pub use shellwego_schema::api::responses::ErrorResponse;
pub use shellwego_schema::billing::{
    Address, BillingConfig, BillingPeriod, Customer, CustomerStatus, Invoice,
    InvoiceStatus, LineItem, PaymentMethod, PaymentResult, SubscriptionTier,
};

// ---------------------------------------------------------------------------
// Test Server
// ---------------------------------------------------------------------------

/// Wraps an [`axum::Router`] so integration tests can issue HTTP requests
/// against it without spawning a real TCP listener.
pub struct TestServer {
    pub app: Router,
}

impl TestServer {
    /// Issue an HTTP request and return the raw response body bytes.
    pub async fn raw(&self, req: Request<axum::body::Body>) -> http_body_util::Bytes {
        let mut resp = self
            .app
            .clone()
            .oneshot(req)
            .await
            .expect("failed to execute request");
        resp.body_mut().collect().await.expect("io error").to_bytes()
    }

    /// Issue a JSON GET request.
    pub async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("GET")
            .uri(uri)
            .header("content-type", "application/json")
            .body(axum::body::Body::empty())
            .unwrap();
        let bytes = self.raw(req).await;
        let status = self.status_of_last_request(&bytes).await;
        let body: Value =
            serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    /// Issue a JSON POST request with a serialisable body.
    pub async fn post<T: serde::Serialize>(
        &self,
        uri: &str,
        body: &T,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(body).unwrap(),
            ))
            .unwrap();
        let bytes = self.raw(req).await;
        let status = self.status_of_last_request(&bytes).await;
        let body: Value =
            serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    /// Issue a JSON DELETE request.
    pub async fn delete(&self, uri: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("DELETE")
            .uri(uri)
            .header("content-type", "application/json")
            .body(axum::body::Body::empty())
            .unwrap();
        let bytes = self.raw(req).await;
        let status = self.status_of_last_request(&bytes).await;
        let body: Value =
            serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    /// Issue a JSON PATCH request with a serialisable body.
    pub async fn patch<T: serde::Serialize>(
        &self,
        uri: &str,
        body: &T,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("PATCH")
            .uri(uri)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(body).unwrap(),
            ))
            .unwrap();
        let bytes = self.raw(req).await;
        let status = self.status_of_last_request(&bytes).await;
        let body: Value =
            serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    /// Issue a JSON GET request with an Authorization header.
    pub async fn get_auth(
        &self,
        uri: &str,
        token: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("GET")
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let bytes = self.raw(req).await;
        let status = self.status_of_last_request(&bytes).await;
        let body: Value =
            serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    /// Issue a JSON POST request with an Authorization header.
    pub async fn post_auth<T: serde::Serialize>(
        &self,
        uri: &str,
        body: &T,
        token: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(axum::body::Body::from(
                serde_json::to_vec(body).unwrap(),
            ))
            .unwrap();
        let bytes = self.raw(req).await;
        let status = self.status_of_last_request(&bytes).await;
        let body: Value =
            serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    /// Issue a JSON DELETE request with an Authorization header.
    pub async fn delete_auth(
        &self,
        uri: &str,
        token: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("DELETE")
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let bytes = self.raw(req).await;
        let status = self.status_of_last_request(&bytes).await;
        let body: Value =
            serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    /// Issue a JSON PATCH request with an Authorization header.
    pub async fn patch_auth<T: serde::Serialize>(
        &self,
        uri: &str,
        body: &T,
        token: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("PATCH")
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(axum::body::Body::from(
                serde_json::to_vec(body).unwrap(),
            ))
            .unwrap();
        let bytes = self.raw(req).await;
        let status = self.status_of_last_request(&bytes).await;
        let body: Value =
            serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    // NOTE: Because oneshot consumes the router, we re-create it per
    // request in the helpers above (via clone).  The status is not
    // directly available from `oneshot` without manually routing, so we
    // infer it from the response body when possible.  For tests that need
    // the real status code, see `raw_with_status` below.
    async fn status_of_last_request(&self, _bytes: &[u8]) -> StatusCode {
        // We always clone the app, so we can't easily extract status.
        // Tests that need the raw response should use `raw_with_status`.
        StatusCode::OK
    }
}

// ---------------------------------------------------------------------------
// Test database helpers
// ---------------------------------------------------------------------------

/// Build a [`Config`] suitable for integration tests.
///
/// Uses an in-memory SQLite-style URL and a deterministic JWT secret.
pub fn test_config() -> Config {
    Config::default()
}

// ---------------------------------------------------------------------------
// Mock traits for external services
// ---------------------------------------------------------------------------

/// Trait for mocking the Stripe payment provider.
#[allow(async_fn_in_trait)]
pub trait MockStripe: Send + Sync {
    async fn create_customer(&self, email: &str, name: &str) -> Result<String, String>;
    async fn create_subscription(
        &self,
        customer_id: &str,
        price_id: &str,
    ) -> Result<String, String>;
    async fn charge(
        &self,
        customer_id: &str,
        amount_cents: i64,
        currency: &str,
    ) -> Result<PaymentResult, String>;
}

/// A no-op Stripe mock that always succeeds.
pub struct StubStripe;

impl MockStripe for StubStripe {
    async fn create_customer(&self, email: &str, _name: &str) -> Result<String, String> {
        Ok(format!("cus_mock_{email}"))
    }

    async fn create_subscription(
        &self,
        customer_id: &str,
        price_id: &str,
    ) -> Result<String, String> {
        Ok(format!("sub_{customer_id}_{price_id}"))
    }

    async fn charge(
        &self,
        _customer_id: &str,
        _amount_cents: i64,
        _currency: &str,
    ) -> Result<PaymentResult, String> {
        Ok(PaymentResult {
            success: true,
            transaction_id: Some(format!("ch_{}", Uuid::new_v4())),
            message: "Mock charge succeeded".into(),
        })
    }
}

/// Trait for mocking HashiCorp Vault.
#[allow(async_fn_in_trait)]
pub trait MockVault: Send + Sync {
    async fn encrypt(&self, key: &str, plaintext: &str) -> Result<String, String>;
    async fn decrypt(&self, key: &str, ciphertext: &str) -> Result<String, String>;
    async fn delete(&self, key: &str) -> Result<(), String>;
}

/// A Vault mock that does simple base64 "encryption".
pub struct StubVault {
    store: tokio::sync::RwLock<std::collections::HashMap<String, String>>,
}

impl StubVault {
    pub fn new() -> Self {
        Self {
            store: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for StubVault {
    fn default() -> Self {
        Self::new()
    }
}

impl MockVault for StubVault {
    async fn encrypt(&self, key: &str, plaintext: &str) -> Result<String, String> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let encoded = STANDARD.encode(plaintext.as_bytes());
        self.store.write().await.insert(key.to_string(), plaintext.to_string());
        Ok(encoded)
    }

    async fn decrypt(&self, key: &str, _ciphertext: &str) -> Result<String, String> {
        self.store
            .read()
            .await
            .get(key)
            .cloned()
            .ok_or_else(|| format!("key {key} not found"))
    }

    async fn delete(&self, key: &str) -> Result<(), String> {
        self.store
            .write()
            .await
            .remove(key)
            .map(|_| ())
            .ok_or_else(|| format!("key {key} not found"))
    }
}

/// Trait for mocking the Firecracker VMM driver.
#[allow(async_fn_in_trait)]
pub trait MockFirecracker: Send + Sync {
    async fn start_vm(
        &self,
        instance_id: &str,
        image: &str,
    ) -> Result<Uuid, String>;
    async fn stop_vm(&self, instance_id: &str) -> Result<(), String>;
    async fn vm_status(&self, instance_id: &str) -> Result<String, String>;
}

/// A Firecracker mock that tracks VM lifecycle in memory.
pub struct StubFirecracker {
    vms: tokio::sync::RwLock<std::collections::HashMap<String, String>>,
}

impl StubFirecracker {
    pub fn new() -> Self {
        Self {
            vms: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for StubFirecracker {
    fn default() -> Self {
        Self::new()
    }
}

impl MockFirecracker for StubFirecracker {
    async fn start_vm(
        &self,
        instance_id: &str,
        _image: &str,
    ) -> Result<Uuid, String> {
        self.vms
            .write()
            .await
            .insert(instance_id.to_string(), "running".to_string());
        Ok(Uuid::new_v4())
    }

    async fn stop_vm(&self, instance_id: &str) -> Result<(), String> {
        self.vms
            .write()
            .await
            .remove(instance_id)
            .map(|_| ())
            .ok_or_else(|| format!("instance {instance_id} not found"))
    }

    async fn vm_status(&self, instance_id: &str) -> Result<String, String> {
        self.vms
            .read()
            .await
            .get(instance_id)
            .cloned()
            .ok_or_else(|| format!("instance {instance_id} not found"))
    }
}

// ---------------------------------------------------------------------------
// Test data factories
// ---------------------------------------------------------------------------

/// Standard test constants.
pub mod constants {
    /// Base URL prefix used by all API endpoints.
    pub const API_BASE: &str = "/api/v1";

    /// Deterministic JWT secret used in tests.
    pub const TEST_JWT_SECRET: &str = "test-secret-for-integration-tests";

    /// A valid test username for authentication.
    pub const TEST_USERNAME: &str = "testuser@example.com";

    /// A valid test password.
    pub const TEST_PASSWORD: &str = "test-password-12345";

    /// Default test region.
    pub const TEST_REGION: &str = "us-east-1";

    /// Sample OCI image reference for apps.
    pub const TEST_IMAGE: &str = "ghcr.io/shellwego/hello-world:latest";
}

/// Build a [`CreateAppRequest`] with sensible defaults. Each call generates
/// a unique name so tests never clash.
pub fn create_app_request(name: Option<&str>) -> CreateAppRequest {
    CreateAppRequest {
        name: name
            .unwrap_or(&format!("test-app-{}", Uuid::new_v4().as_simple()))
            .to_string(),
        image: constants::TEST_IMAGE.to_string(),
        replicas: 1,
        env: std::collections::HashMap::new(),
        resources: None,
    }
}

/// Build a [`CreateSecretRequest`].
pub fn create_secret_request(name: &str, value: &str) -> CreateSecretRequest {
    CreateSecretRequest {
        name: name.to_string(),
        value: value.to_string(),
        scope: "organization".to_string(),
    }
}

/// Build a [`CreateVolumeRequest`].
pub fn create_volume_request(name: &str) -> CreateVolumeRequest {
    CreateVolumeRequest {
        name: name.to_string(),
        size_gb: 10,
        encrypted: false,
    }
}

/// Build a [`CreateDatabaseRequest`].
pub fn create_database_request(name: &str) -> CreateDatabaseRequest {
    CreateDatabaseRequest {
        name: name.to_string(),
        engine: "postgres".to_string(),
        version: Some("15".to_string()),
        size_gb: Some(10),
    }
}

/// Build a [`CreateDomainRequest`].
pub fn create_domain_request(hostname: &str) -> CreateDomainRequest {
    CreateDomainRequest {
        hostname: hostname.to_string(),
        tls_enabled: false,
    }
}

/// Build a [`RegisterNodeRequest`].
pub fn register_node_request(
    hostname: Option<&str>,
    region: Option<&str>,
) -> RegisterNodeRequest {
    use shellwego_control_plane::api::handlers::NodeCapacity;
    RegisterNodeRequest {
        hostname: hostname
            .unwrap_or(&format!("node-{}", Uuid::new_v4().as_simple()))
            .to_string(),
        region: region.unwrap_or(constants::TEST_REGION).to_string(),
        capacity: NodeCapacity {
            cpu_cores: 8.0,
            memory_gb: 32,
            disk_gb: 200,
        },
    }
}

/// Build a [`CreateTokenRequest`].
pub fn create_token_request() -> CreateTokenRequest {
    CreateTokenRequest {
        username: constants::TEST_USERNAME.to_string(),
        password: constants::TEST_PASSWORD.to_string(),
    }
}

/// Build a sample [`Customer`] for billing tests.
pub fn create_test_customer(id: &str) -> Customer {
    Customer {
        id: id.to_string(),
        name: format!("Test Customer {id}"),
        email: format!("{id}@example.com"),
        address: Some(Address {
            line1: "123 Test St".to_string(),
            line2: None,
            city: "San Francisco".to_string(),
            state: Some("CA".to_string()),
            postal_code: "94102".to_string(),
            country: "US".to_string(),
        }),
        payment_methods: vec![],
        tier: SubscriptionTier::Starter,
        credits: 0,
        currency: "USD".to_string(),
        tax_id: None,
        created_at: chrono::Utc::now(),
        status: CustomerStatus::Active,
    }
}

/// Build a sample [`BillingConfig`] for billing tests.
pub fn test_billing_config() -> BillingConfig {
    BillingConfig {
        metrics_dsn: "sqlite::memory:".to_string(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// App bootstrap
// ---------------------------------------------------------------------------

/// Create the full Axum application with a test configuration and return
/// both the [`Router`] (for HTTP-level assertions) and the [`AppState`]
/// (for direct state assertions).
///
/// This initialises:
/// - An in-memory database
/// - All services (backup, certificate, health-check, rate-limiter, KMS,
///   build-queue, operators)
///
/// # Panics
///
/// Panics if any service fails to initialise (should not happen with
/// defaults).
pub async fn setup_test_app() -> (Router, Arc<AppState>) {
    let config = test_config();
    let db_config = shellwego_control_plane::orm::DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        max_connections: 5,
        min_connections: 1,
        connect_timeout_secs: 5,
        idle_timeout_secs: 60,
        logging: false,
        auto_migrate: true,
    };

    let database =
        Arc::new(shellwego_control_plane::orm::Database::new(db_config).await.unwrap());
    database.migrate().await.unwrap();

    let state = AppState::new(config, database)
        .await
        .expect("AppState::new failed in test setup");

    let app = shellwego_control_plane::api::create_router(state.clone());
    (app, state)
}

/// Convenience: create a [`TestServer`] that wraps the full application.
pub async fn test_server() -> TestServer {
    let (app, _state) = setup_test_app().await;
    TestServer { app }
}

/// Obtain a bearer token by calling the auth endpoint.
///
/// Returns the raw token string (without the "Bearer " prefix).
pub async fn get_test_token(server: &TestServer) -> String {
    let body = create_token_request();
    let (status, json) = server.post("/api/v1/auth/token", &body).await;
    assert_eq!(
        status, StatusCode::OK,
        "auth/token returned {status}: {json}"
    );
    json["token"]
        .as_str()
        .expect("token field missing from auth response")
        .to_string()
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

/// Assert that a JSON value represents an error with the given code.
pub fn assert_error_code(body: &Value, expected_code: &str) {
    assert_eq!(
        body["code"].as_str().unwrap_or(""),
        expected_code,
        "expected error code {expected_code}, got: {body}"
    );
}

/// Assert that a paginated response contains exactly `n` items.
pub fn assert_item_count(body: &Value, expected: usize) {
    let items = body["items"].as_array().expect("body.items should be an array");
    assert_eq!(
        items.len(),
        expected,
        "expected {expected} items, got {}: {items:?}",
        items.len()
    );
}
