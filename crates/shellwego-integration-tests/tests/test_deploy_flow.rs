//! Integration tests for the deployment flow.
//!
//! Covers the end-to-end deploy pipeline: create an app → trigger a
//! build → check build status.  Also tests build listing, cancellation,
//! and log retrieval.
//!
//! The build system is backed by the in-memory [`BuildQueue`] which
//! processes builds asynchronously.  Tests may need small delays to
//! observe status transitions.

#[cfg(feature = "integration-tests")]
mod tests {
    mod common;

    use axum::http::StatusCode;
    use common::{create_app_request, test_server};
    use serde_json::json;
    use uuid::Uuid;

    // -----------------------------------------------------------------------
    // test_create_app_for_deploy
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_app_for_deploy() {
        let server = test_server().await;
        let body = create_app_request(Some("deploy-pipeline-app"));

        let (status, json) = server.post("/api/v1/apps", &body).await;

        assert_eq!(status, StatusCode::CREATED, "create app: {json}");
        assert_eq!(json["name"].as_str().unwrap(), "deploy-pipeline-app");
        assert_eq!(json["status"].as_str().unwrap(), "creating");
        assert_eq!(json["image"].as_str().unwrap(), common::constants::TEST_IMAGE);

        // Save the ID for subsequent steps
        let app_id = json["id"].as_str().unwrap();
        assert!(!Uuid::parse_str(app_id).is_err());
    }

    // -----------------------------------------------------------------------
    // test_trigger_build
    //
    // POST /api/v1/apps/{id}/deploy triggers a deployment, which in
    // turn creates a build.  The deploy endpoint returns immediately
    // with a pending deployment.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_trigger_build() {
        let server = test_server().await;

        // Create an app
        let body = create_app_request(Some("build-trigger-app"));
        let (_create_status, create_json) =
            server.post("/api/v1/apps", &body).await;
        let app_id = create_json["id"].as_str().unwrap();

        // Trigger a deploy (which initiates a build)
        let (status, json) = server
            .post(&format!("/api/v1/apps/{app_id}/deploy"), &json!({}))
            .await;

        assert_eq!(status, StatusCode::OK, "deploy response: {json}");
        assert_eq!(json["app_id"].as_str().unwrap(), app_id);
        assert_eq!(json["status"].as_str().unwrap(), "pending");

        // deployment_id should be a valid UUID
        let deployment_id = json["deployment_id"].as_str().unwrap();
        assert!(
            !Uuid::parse_str(deployment_id).is_err(),
            "deployment_id should be a valid UUID"
        );
    }

    // -----------------------------------------------------------------------
    // test_build_status
    //
    // GET /api/v1/builds/{id} — current handler returns 404 for all IDs.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_build_status() {
        let server = test_server().await;

        let build_id = Uuid::new_v4();
        let (status, json) =
            server.get(&format!("/api/v1/builds/{build_id}")).await;

        // Current handler returns 404
        assert_eq!(
            status, StatusCode::NOT_FOUND,
            "build status for nonexistent build should be 404: {json}"
        );
    }

    // -----------------------------------------------------------------------
    // test_list_builds
    //
    // GET /api/v1/builds returns build queue statistics.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_builds() {
        let server = test_server().await;

        let (status, json) = server.get("/api/v1/builds").await;

        assert_eq!(status, StatusCode::OK, "list builds: {json}");

        // Response contains stats
        assert!(json["items"].is_array(), "should have items array");
        if let Some(items) = json["items"].as_array() {
            if let Some(first) = items.first() {
                // Stats have pending/running/completed counts
                assert!(first["pending"].is_number(), "should have pending count");
                assert!(first["running"].is_number(), "should have running count");
                assert!(first["completed"].is_number(), "should have completed count");
            }
        }
    }

    // -----------------------------------------------------------------------
    // test_build_logs
    //
    // GET /api/v1/builds/{id}/logs — returns an empty log array.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_build_logs() {
        let server = test_server().await;

        let build_id = Uuid::new_v4();
        let (status, json) = server
            .get(&format!("/api/v1/builds/{build_id}/logs"))
            .await;

        assert_eq!(status, StatusCode::OK, "build logs: {json}");
        assert!(json.is_array(), "logs should be an array");
        // Current handler returns an empty vec
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    // -----------------------------------------------------------------------
    // test_cancel_build
    //
    // POST /api/v1/builds/{id}/cancel — returns a cancellation response.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_cancel_build() {
        let server = test_server().await;

        let build_id = Uuid::new_v4();
        let (status, json) = server
            .post(&format!("/api/v1/builds/{build_id}/cancel"), &json!({}))
            .await;

        assert_eq!(status, StatusCode::OK, "cancel build: {json}");
        assert_eq!(json["status"].as_str().unwrap(), "cancelled");
        assert_eq!(json["build_id"].as_str().unwrap(), build_id.to_string());
    }

    // -----------------------------------------------------------------------
    // test_full_deploy_pipeline
    //
    // End-to-end: create app → deploy → verify deployment object.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_full_deploy_pipeline() {
        let server = test_server().await;

        // Step 1: Create app
        let app_body = create_app_request(Some("pipeline-e2e-app"));
        let (status, json) = server.post("/api/v1/apps", &app_body).await;
        assert_eq!(status, StatusCode::CREATED, "step 1 create: {json}");
        let app_id = json["id"].as_str().unwrap().to_string();

        // Step 2: List builds (should show stats)
        let (status, json) = server.get("/api/v1/builds").await;
        assert_eq!(status, StatusCode::OK, "step 2 list builds: {json}");

        // Step 3: Trigger deploy
        let (status, json) = server
            .post(&format!("/api/v1/apps/{app_id}/deploy"), &json!({}))
            .await;
        assert_eq!(status, StatusCode::OK, "step 3 deploy: {json}");
        let deployment_id = json["deployment_id"].as_str().unwrap();

        // Step 4: Verify deployment response
        assert_eq!(json["app_id"].as_str().unwrap(), app_id);
        assert_eq!(json["status"].as_str().unwrap(), "pending");
        assert!(!deployment_id.is_empty());

        // Step 5: Check build logs (empty for now)
        let (status, json) = server
            .get(&format!("/api/v1/builds/{deployment_id}/logs"))
            .await;
        assert_eq!(status, StatusCode::OK, "step 5 logs: {json}");
        assert!(json.is_array());
    }
}
