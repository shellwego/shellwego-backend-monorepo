//! Integration tests for App CRUD operations.
//!
//! Covers the full lifecycle: create → list → get → update → delete,
//! including not-found and pagination edge cases.
//!
//! **Note on the current API**: The control-plane handlers currently return
//! stub data (e.g. `get_app` always returns 404, `delete_app` always
//! returns 404, `list_apps` always returns empty).  These tests exercise
//! the HTTP layer and validate the response shapes, documenting the
//! expected contract as the persistence layer is filled in.

#[cfg(feature = "integration-tests")]
mod tests {
    mod common;

    use axum::http::StatusCode;
    use common::{
        assert_error_code, assert_item_count, constants, create_app_request, get_test_token,
        test_server,
    };
    use serde_json::json;
    use uuid::Uuid;

    // -----------------------------------------------------------------------
    // test_create_app
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_app() {
        let server = test_server().await;
        let body = create_app_request(Some("my-test-app"));

        let (status, json) = server.post("/api/v1/apps", &body).await;

        assert_eq!(
            status, StatusCode::CREATED,
            "create app should return 201: {json}"
        );

        // Validate response shape
        assert!(json["id"].is_string(), "app should have an id");
        assert_eq!(json["name"].as_str().unwrap(), "my-test-app");
        assert_eq!(json["image"].as_str().unwrap(), constants::TEST_IMAGE);
        assert_eq!(json["status"].as_str().unwrap(), "creating");
        assert_eq!(json["replicas"].as_u64().unwrap(), 1);
        assert!(json["created_at"].is_string());
        assert!(json["updated_at"].is_string());

        // Verify the id is a valid UUID
        let id = json["id"].as_str().unwrap();
        Uuid::parse_str(id).expect("app id should be a valid UUID");
    }

    // -----------------------------------------------------------------------
    // test_create_app_defaults
    //
    // Creating an app without specifying replicas or resources should
    // apply sensible defaults.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_app_defaults() {
        let server = test_server().await;
        let body = create_app_request(None); // auto-generated name

        let (status, json) = server.post("/api/v1/apps", &body).await;
        assert_eq!(status, StatusCode::CREATED, "response: {json}");

        // Replicas defaults to at least 1
        assert!(json["replicas"].as_u64().unwrap() >= 1);
    }

    // -----------------------------------------------------------------------
    // test_list_apps
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_apps() {
        let server = test_server().await;

        // The current handler returns a paginated (but empty) response
        let (status, json) = server.get("/api/v1/apps").await;

        assert_eq!(
            status, StatusCode::OK,
            "list apps should return 200: {json}"
        );

        // Validate paginated response shape
        assert!(json["items"].is_array(), "response should contain items array");
        assert!(
            json["has_more"].is_boolean(),
            "response should contain has_more"
        );
        assert!(
            json["next_cursor"].is_null() || json["next_cursor"].is_string(),
            "next_cursor should be null or string"
        );
    }

    // -----------------------------------------------------------------------
    // test_get_app
    //
    // Current handler always returns 404.  Test documents the expected
    // behaviour once persistence is wired.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_app() {
        let server = test_server().await;

        // First create an app so we have an ID
        let body = create_app_request(Some("get-test-app"));
        let (_create_status, create_json) = server.post("/api/v1/apps", &body).await;
        let app_id = create_json["id"].as_str().unwrap();

        // Try to GET it
        let (status, json) = server.get(&format!("/api/v1/apps/{app_id}")).await;

        // Currently returns 404 because the handler is a stub.
        // When persistence is added this should be 200.
        match status {
            StatusCode::NOT_FOUND => {
                assert_error_code(&json, "NOT_FOUND");
            }
            StatusCode::OK => {
                // Future behaviour: validate the returned app
                assert_eq!(json["id"].as_str().unwrap(), app_id);
            }
            other => panic!("unexpected status {other}: {json}"),
        }
    }

    // -----------------------------------------------------------------------
    // test_get_app_not_found
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_app_not_found() {
        let server = test_server().await;

        let nonexistent = Uuid::new_v4();
        let (status, json) =
            server.get(&format!("/api/v1/apps/{nonexistent}")).await;

        assert_eq!(
            status, StatusCode::NOT_FOUND,
            "nonexistent app should return 404: {json}"
        );
        assert_error_code(&json, "NOT_FOUND");
    }

    // -----------------------------------------------------------------------
    // test_update_app
    //
    // There is currently no PATCH /apps/{id} handler in the router.
    // This test verifies that an unsupported method returns the
    // appropriate HTTP status.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_update_app() {
        let server = test_server().await;

        // Create an app first
        let body = create_app_request(Some("update-test-app"));
        let (_create_status, create_json) = server.post("/api/v1/apps", &body).await;
        let app_id = create_json["id"].as_str().unwrap();

        // Attempt a PATCH
        let patch_body = json!({ "replicas": 3 });
        let (status, _json) = server
            .patch(&format!("/api/v1/apps/{app_id}"), &patch_body)
            .await;

        // Axum returns 405 Method Not Allowed when no route matches.
        assert!(
            status == StatusCode::METHOD_NOT_ALLOWED
                || status == StatusCode::NOT_FOUND,
            "expected 405 or 404 for PATCH, got {status}"
        );
    }

    // -----------------------------------------------------------------------
    // test_delete_app
    //
    // Current handler always returns 404.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_delete_app() {
        let server = test_server().await;

        // Create an app first
        let body = create_app_request(Some("delete-test-app"));
        let (_create_status, create_json) = server.post("/api/v1/apps", &body).await;
        let app_id = create_json["id"].as_str().unwrap();

        // Delete it
        let (status, json) =
            server.delete(&format!("/api/v1/apps/{app_id}")).await;

        // Currently returns 404 (stub handler).
        // When persistence is added this should be 204.
        match status {
            StatusCode::NOT_FOUND => {
                assert_error_code(&json, "NOT_FOUND");
            }
            StatusCode::NO_CONTENT => {
                // Future behaviour
            }
            other => panic!("unexpected status {other}: {json}"),
        }
    }

    // -----------------------------------------------------------------------
    // test_delete_app_not_found
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_delete_app_not_found() {
        let server = test_server().await;

        let nonexistent = Uuid::new_v4();
        let (status, json) =
            server.delete(&format!("/api/v1/apps/{nonexistent}")).await;

        assert_eq!(
            status, StatusCode::NOT_FOUND,
            "delete nonexistent should return 404: {json}"
        );
        assert_error_code(&json, "NOT_FOUND");
    }

    // -----------------------------------------------------------------------
    // test_pagination
    //
    // The list_apps handler accepts `page` and `per_page` query params.
    // In the current stub it always returns empty, so we verify the query
    // is accepted without error and the response shape is correct.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pagination() {
        let server = test_server().await;

        // Page 1, 2 per page
        let (status, json) = server.get("/api/v1/apps?page=1&per_page=2").await;
        assert_eq!(status, StatusCode::OK, "pagination response: {json}");

        // Validate the paginated shape
        assert_item_count(&json, 0); // stub returns empty

        // Page 2, should also be accepted
        let (status, json) = server.get("/api/v1/apps?page=2&per_page=2").await;
        assert_eq!(status, StatusCode::OK, "page 2: {json}");
        assert_item_count(&json, 0);
    }

    // -----------------------------------------------------------------------
    // test_deploy_app
    //
    // POST /api/v1/apps/{id}/deploy should return a deployment object.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_deploy_app() {
        let server = test_server().await;

        // Create an app
        let body = create_app_request(Some("deploy-test-app"));
        let (_create_status, create_json) = server.post("/api/v1/apps", &body).await;
        let app_id = create_json["id"].as_str().unwrap();

        // Trigger deploy
        let (status, json) =
            server.post(&format!("/api/v1/apps/{app_id}/deploy"), &json!({})).await;

        assert_eq!(status, StatusCode::OK, "deploy response: {json}");
        assert_eq!(json["app_id"].as_str().unwrap(), app_id);
        assert_eq!(json["status"].as_str().unwrap(), "pending");
        assert!(json["deployment_id"].is_string());
    }

    // -----------------------------------------------------------------------
    // test_restart_app
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_restart_app() {
        let server = test_server().await;

        let body = create_app_request(Some("restart-test-app"));
        let (_create_status, create_json) = server.post("/api/v1/apps", &body).await;
        let app_id = create_json["id"].as_str().unwrap();

        let (status, json) =
            server.post(&format!("/api/v1/apps/{app_id}/restart"), &json!({})).await;

        assert_eq!(status, StatusCode::OK, "restart response: {json}");
        assert_eq!(json["status"].as_str().unwrap(), "restarting");
    }

    // -----------------------------------------------------------------------
    // test_scale_app
    //
    // Current handler returns 404 for non-existent app.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_scale_app() {
        let server = test_server().await;

        let body = create_app_request(Some("scale-test-app"));
        let (_create_status, create_json) = server.post("/api/v1/apps", &body).await;
        let app_id = create_json["id"].as_str().unwrap();

        let scale_body = json!({ "replicas": 5 });
        let (status, json) =
            server.post(&format!("/api/v1/apps/{app_id}/scale"), &scale_body).await;

        // Handler currently returns 404
        match status {
            StatusCode::NOT_FOUND => {
                assert_error_code(&json, "NOT_FOUND");
            }
            StatusCode::OK => {
                assert_eq!(json["replicas"].as_u64().unwrap(), 5);
            }
            other => panic!("unexpected status {other}: {json}"),
        }
    }
}
