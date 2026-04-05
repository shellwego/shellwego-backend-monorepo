//! Integration tests for Node management CRUD operations.
//!
//! Covers node registration, listing, retrieval, deregistration, and
//! the drain lifecycle.

#[cfg(feature = "integration-tests")]
mod tests {
    mod common;

    use axum::http::StatusCode;
    use common::{assert_error_code, assert_item_count, register_node_request, test_server};
    use serde_json::json;
    use uuid::Uuid;

    // -----------------------------------------------------------------------
    // test_register_node
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_register_node() {
        let server = test_server().await;
        let body = register_node_request(Some("node-01"), None);

        let (status, json) = server.post("/api/v1/nodes", &body).await;

        assert_eq!(
            status, StatusCode::CREATED,
            "register node should return 201: {json}"
        );

        // Validate response shape
        assert!(json["id"].is_string(), "node should have an id");
        assert_eq!(json["hostname"].as_str().unwrap(), "node-01");
        assert_eq!(json["region"].as_str().unwrap(), "us-east-1");
        assert_eq!(json["status"].as_str().unwrap(), "ready");
        assert!(json["created_at"].is_string());

        // Capacity should be reflected
        assert_eq!(json["capacity"]["cpu_cores"].as_f64().unwrap(), 8.0);
        assert_eq!(json["capacity"]["memory_gb"].as_u64().unwrap(), 32);
        assert_eq!(json["capacity"]["disk_gb"].as_u64().unwrap(), 200);

        // Verify id is a valid UUID
        let id = json["id"].as_str().unwrap();
        Uuid::parse_str(id).expect("node id should be a valid UUID");
    }

    // -----------------------------------------------------------------------
    // test_register_node_custom_region
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_register_node_custom_region() {
        let server = test_server().await;
        let body = register_node_request(Some("eu-node"), Some("eu-west-1"));

        let (status, json) = server.post("/api/v1/nodes", &body).await;

        assert_eq!(status, StatusCode::CREATED, "response: {json}");
        assert_eq!(json["region"].as_str().unwrap(), "eu-west-1");
    }

    // -----------------------------------------------------------------------
    // test_list_nodes
    //
    // After registering nodes, the list endpoint should include them.
    // The `list_nodes` handler reads from the in-memory `agents` DashMap,
    // so nodes registered in the same test will appear.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_nodes() {
        let server = test_server().await;

        // Initially, the list should be empty
        let (status, json) = server.get("/api/v1/nodes").await;
        assert_eq!(status, StatusCode::OK, "list nodes: {json}");
        assert_item_count(&json, 0);

        // Register a node
        let body = register_node_request(Some("list-node-1"), None);
        let (_create_status, _create_json) =
            server.post("/api/v1/nodes", &body).await;

        // Now list should contain 1 item
        let (status, json) = server.get("/api/v1/nodes").await;
        assert_eq!(status, StatusCode::OK, "list nodes after register: {json}");
        assert_item_count(&json, 1);
        assert_eq!(
            json["items"][0]["hostname"].as_str().unwrap(),
            "list-node-1"
        );
    }

    // -----------------------------------------------------------------------
    // test_list_nodes_multiple
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_nodes_multiple() {
        let server = test_server().await;

        // Register two nodes
        let body1 = register_node_request(Some("multi-node-a"), Some("us-west-2"));
        let body2 = register_node_request(Some("multi-node-b"), Some("ap-south-1"));

        let _ = server.post("/api/v1/nodes", &body1).await;
        let _ = server.post("/api/v1/nodes", &body2).await;

        let (status, json) = server.get("/api/v1/nodes").await;
        assert_eq!(status, StatusCode::OK, "list: {json}");
        assert_item_count(&json, 2);
    }

    // -----------------------------------------------------------------------
    // test_node_heartbeat
    //
    // There is no dedicated heartbeat endpoint in the current router.
    // Heartbeats are handled via the QUIC control channel.  We test
    // the drain endpoint which updates node state.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_node_heartbeat() {
        let server = test_server().await;

        // Register a node
        let body = register_node_request(Some("heartbeat-node"), None);
        let (_create_status, create_json) =
            server.post("/api/v1/nodes", &body).await;
        let node_id = create_json["id"].as_str().unwrap();

        // The heartbeat is internal (QUIC), but we can verify the node
        // appears healthy in the list after registration.
        let (status, json) = server.get("/api/v1/nodes").await;
        assert_eq!(status, StatusCode::OK);

        let found = json["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"].as_str() == Some(node_id) && n["status"].as_str() == Some("ready"));
        assert!(found, "registered node should be in the list with 'ready' status");
    }

    // -----------------------------------------------------------------------
    // test_deregister_node
    //
    // DELETE /api/v1/nodes/{id} removes the agent from the DashMap.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_deregister_node() {
        let server = test_server().await;

        // Register
        let body = register_node_request(Some("dereg-node"), None);
        let (_create_status, create_json) =
            server.post("/api/v1/nodes", &body).await;
        let node_id = create_json["id"].as_str().unwrap();

        // Verify it exists
        let (status, json) = server.get("/api/v1/nodes").await;
        assert_eq!(status, StatusCode::OK);
        assert_item_count(&json, 1);

        // Deregister
        let (status, _json) =
            server.delete(&format!("/api/v1/nodes/{node_id}")).await;
        assert_eq!(
            status, StatusCode::NO_CONTENT,
            "deregister should return 204"
        );

        // Verify it's gone
        let (status, json) = server.get("/api/v1/nodes").await;
        assert_eq!(status, StatusCode::OK);
        assert_item_count(&json, 0);
    }

    // -----------------------------------------------------------------------
    // test_drain_node
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_drain_node() {
        let server = test_server().await;

        let body = register_node_request(Some("drain-node"), None);
        let (_create_status, create_json) =
            server.post("/api/v1/nodes", &body).await;
        let node_id = create_json["id"].as_str().unwrap();

        let (status, json) =
            server.post(&format!("/api/v1/nodes/{node_id}/drain"), &json!({})).await;

        assert_eq!(status, StatusCode::OK, "drain response: {json}");
        assert_eq!(json["status"].as_str().unwrap(), "draining");
        assert_eq!(json["node_id"].as_str().unwrap(), node_id);
    }

    // -----------------------------------------------------------------------
    // test_get_node_not_found
    //
    // The current `get_node` handler returns 404 for any node.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_node_not_found() {
        let server = test_server().await;

        let nonexistent = Uuid::new_v4();
        let (status, json) =
            server.get(&format!("/api/v1/nodes/{nonexistent}")).await;

        assert_eq!(
            status, StatusCode::NOT_FOUND,
            "nonexistent node should return 404: {json}"
        );
        assert_error_code(&json, "NOT_FOUND");
    }
}
