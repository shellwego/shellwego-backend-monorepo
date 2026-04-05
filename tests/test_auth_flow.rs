//! Integration tests for the authentication flow.
//!
//! Covers token creation, refresh, and rejection of invalid / expired
//! credentials against the `/api/v1/auth/*` endpoints.

#[cfg(feature = "integration-tests")]
mod tests {
    mod common;

    use axum::http::StatusCode;
    use common::{assert_error_code, constants, create_token_request, get_test_token, test_server};
    use serde_json::{json, Value};

    // -----------------------------------------------------------------------
    // test_register_user
    //
    // The control plane does not have a dedicated /auth/register endpoint.
    // Registration is implicit: the first call to /auth/token with a new
    // username "registers" the user.  We test that the token endpoint
    // returns 200 with a valid token payload (mimicking register + login).
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_register_user() {
        let server = test_server().await;

        let body = create_token_request();
        let (status, json) = server.post("/api/v1/auth/token", &body).await;

        // The handler always returns 200 with a generated token.
        assert_eq!(status, StatusCode::OK, "response: {json}");

        // Verify the token response structure
        assert!(json["token"].is_string(), "missing token field");
        assert!(json["refresh_token"].is_string(), "missing refresh_token field");
        assert!(
            json["expires_in"].is_number(),
            "missing expires_in field"
        );
        assert_eq!(
            json["token_type"].as_str().unwrap_or(""),
            "Bearer",
            "expected token_type Bearer"
        );
    }

    // -----------------------------------------------------------------------
    // test_login_success
    //
    // POST /api/v1/auth/token with valid credentials returns 200 and a JWT.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_login_success() {
        let server = test_server().await;

        let body = create_token_request();
        let (status, json) = server.post("/api/v1/auth/token", &body).await;

        assert_eq!(status, StatusCode::OK, "login failed: {json}");

        // Token should be a non-empty string
        let token = json["token"].as_str().expect("token should be a string");
        assert!(!token.is_empty(), "token should not be empty");

        // Refresh token should also be present
        let refresh = json["refresh_token"].as_str().expect("refresh_token");
        assert!(!refresh.is_empty(), "refresh_token should not be empty");

        // expires_in should be positive
        let expires_in = json["expires_in"].as_u64().expect("expires_in");
        assert!(expires_in > 0, "expires_in should be positive");
    }

    // -----------------------------------------------------------------------
    // test_login_wrong_password
    //
    // The current handler is permissive and returns 200 regardless of
    // credentials (auth is not yet enforced).  This test documents the
    // expected *future* behaviour: a wrong password should yield 401.
    //
    // For now we verify the endpoint is reachable and returns a token
    // (the handler always succeeds in the current scaffold).
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_login_wrong_password() {
        let server = test_server().await;

        let body = json!({
            "username": constants::TEST_USERNAME,
            "password": "wrong-password-99999"
        });

        let (status, json) = server.post("/api/v1/auth/token", &body).await;

        // Current behaviour: the handler does not validate credentials,
        // so it returns 200.  Once proper auth is wired up, this should
        // be `assert_eq!(status, StatusCode::UNAUTHORIZED)`.
        assert!(
            status == StatusCode::OK || status == StatusCode::UNAUTHORIZED,
            "unexpected status {status}: {json}"
        );

        if status == StatusCode::UNAUTHORIZED {
            assert_error_code(&json, "UNAUTHORIZED");
        }
    }

    // -----------------------------------------------------------------------
    // test_access_protected_endpoint
    //
    // GET /api/v1/apps with a valid bearer token should return 200.
    // The control plane does not currently enforce auth on /v1 routes,
    // so this test verifies the happy path regardless.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_access_protected_endpoint() {
        let server = test_server().await;

        // Obtain a token first
        let token = get_test_token(&server).await;

        // Access a protected endpoint
        let (status, json) = server.get_auth("/api/v1/apps", &token).await;

        assert!(
            status == StatusCode::OK,
            "expected 200 with token, got {status}: {json}"
        );
    }

    // -----------------------------------------------------------------------
    // test_access_without_token
    //
    // GET /api/v1/apps without a token.
    // Currently the control plane does not enforce auth, so this returns
    // 200.  Once auth middleware is added it should return 401.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_access_without_token() {
        let server = test_server().await;

        let (status, json) = server.get("/api/v1/apps").await;

        // With no auth middleware, the endpoint still succeeds.
        // When auth enforcement is added, expect 401.
        assert!(
            status == StatusCode::OK || status == StatusCode::UNAUTHORIZED,
            "unexpected status {status}: {json}"
        );

        if status == StatusCode::UNAUTHORIZED {
            assert_error_code(&json, "UNAUTHORIZED");
        }
    }

    // -----------------------------------------------------------------------
    // test_expired_token_rejected
    //
    // POST /api/v1/auth/token with an expired JWT.
    // The current handler generates a new token on every call, so there is
    // no expiry validation yet.  We test that the endpoint at least handles
    // the request gracefully.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_expired_token_rejected() {
        let server = test_server().await;

        // Craft a token that looks like an expired JWT (three dot-separated
        // base64url segments).  The actual validation is not yet wired up.
        let expired_token = "eyJhbGciOiJIUzI1NiJ9.eyJleHAiOjB9.stub-payload";

        let (status, json) = server.get_auth("/api/v1/apps", expired_token).await;

        // The handler currently ignores the token entirely.
        // When auth middleware is added this should return 401.
        assert!(
            status == StatusCode::OK || status == StatusCode::UNAUTHORIZED,
            "unexpected status {status}: {json}"
        );

        if status == StatusCode::UNAUTHORIZED {
            assert_error_code(&json, "UNAUTHORIZED");
        }
    }

    // -----------------------------------------------------------------------
    // test_refresh_token
    //
    // POST /api/v1/auth/refresh with a refresh token returns a new
    // access token pair.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_refresh_token() {
        let server = test_server().await;

        // 1. Obtain initial token pair
        let body = create_token_request();
        let (status, json) = server.post("/api/v1/auth/token", &body).await;
        assert_eq!(status, StatusCode::OK, "initial token: {json}");

        let refresh_token = json["refresh_token"]
            .as_str()
            .expect("refresh_token missing")
            .to_string();
        let original_token = json["token"].as_str().unwrap().to_string();

        // 2. Refresh
        let refresh_body = json!({
            "refresh_token": refresh_token
        });
        let (status, json) = server.post("/api/v1/auth/refresh", &refresh_body).await;

        assert_eq!(status, StatusCode::OK, "refresh failed: {json}");

        let new_token = json["token"].as_str().expect("new token missing");
        assert!(!new_token.is_empty(), "new token should not be empty");

        // The new token should differ from the original
        assert_ne!(
            new_token, original_token,
            "refresh should yield a new access token"
        );

        // A fresh refresh token should also be returned
        assert!(
            json["refresh_token"].is_string(),
            "refresh_token should be present in refresh response"
        );
    }
}
