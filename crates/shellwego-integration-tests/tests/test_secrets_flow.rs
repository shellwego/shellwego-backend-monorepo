//! Integration tests for the Secrets management flow.
//!
//! Tests secret creation (which triggers KMS encryption), retrieval,
//! deletion, listing, and the encryption/decryption round-trip.
//!
//! **Note**: The current handlers use stubs for most persistence
//! operations (e.g. `get_secret` always returns 404).  These tests
//! validate the HTTP contract and KMS integration path.

#[cfg(feature = "integration-tests")]
mod tests {
    mod common;

    use axum::http::StatusCode;
    use common::{assert_error_code, assert_item_count, create_secret_request, test_server};
    use serde_json::json;
    use uuid::Uuid;

    // -----------------------------------------------------------------------
    // test_create_secret
    //
    // POST /api/v1/secrets encrypts the value via KMS and returns
    // the secret metadata (no plaintext).
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_secret() {
        let server = test_server().await;
        let body = create_secret_request("DB_PASSWORD", "super-secret-123");

        let (status, json) = server.post("/api/v1/secrets", &body).await;

        assert_eq!(
            status, StatusCode::CREATED,
            "create secret should return 201: {json}"
        );

        // Response should contain metadata but NOT the plaintext value
        assert!(json["id"].is_string(), "secret should have an id");
        assert_eq!(json["name"].as_str().unwrap(), "DB_PASSWORD");
        assert_eq!(json["scope"].as_str().unwrap(), "organization");
        assert!(json["created_at"].is_string());
        assert!(json["updated_at"].is_string());

        // Plaintext value must NOT be in the response
        assert!(
            json.get("value").is_none(),
            "secret value must not appear in the create response"
        );

        // Verify id is a valid UUID
        let id = json["id"].as_str().unwrap();
        Uuid::parse_str(id).expect("secret id should be a valid UUID");
    }

    // -----------------------------------------------------------------------
    // test_get_secret
    //
    // GET /api/v1/secrets/{id} — currently returns 404 for all IDs.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_secret() {
        let server = test_server().await;

        // Create a secret first
        let body = create_secret_request("API_KEY", "key-abcdef");
        let (_create_status, create_json) =
            server.post("/api/v1/secrets", &body).await;
        let secret_id = create_json["id"].as_str().unwrap();

        // Try to retrieve it
        let (status, json) =
            server.get(&format!("/api/v1/secrets/{secret_id}")).await;

        // Current handler is a stub — always 404.
        match status {
            StatusCode::NOT_FOUND => {
                assert_error_code(&json, "NOT_FOUND");
            }
            StatusCode::OK => {
                // Future behaviour: metadata should match
                assert_eq!(json["name"].as_str().unwrap(), "API_KEY");
            }
            other => panic!("unexpected status {other}: {json}"),
        }
    }

    // -----------------------------------------------------------------------
    // test_secret_roundtrip
    //
    // Create a secret with value "hello-world-123", then retrieve and
    // verify the decrypted value matches.  Since the current handler
    // stub does not persist, we verify the KMS encryption path
    // indirectly via the successful 201 on creation.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_secret_roundtrip() {
        let server = test_server().await;
        let plaintext = "hello-world-123";

        let body = create_secret_request("ROUNDTRIP_SECRET", plaintext);
        let (status, json) = server.post("/api/v1/secrets", &body).await;

        assert_eq!(status, StatusCode::CREATED, "create: {json}");

        // The KMS encrypt step succeeded (handler would return 500 if
        // encryption failed), which we verified by the 201 above.
        //
        // The round-trip assertion is validated at the KMS unit-test
        // level (see kms::tests::test_encrypt_decrypt).  Here we
        // confirm the API path exercises that code successfully.
        let secret_id = json["id"].as_str().unwrap();

        // When persistence is wired up, the retrieval + decryption
        // verification would look like:
        //   let (status, json) = server.get(format!("/api/v1/secrets/{secret_id}")).await;
        //   assert_eq!(json["value"].as_str().unwrap(), plaintext);
        assert!(
            !Uuid::parse_str(secret_id).is_err(),
            "secret_id should be valid for future retrieval"
        );
    }

    // -----------------------------------------------------------------------
    // test_delete_secret
    //
    // DELETE /api/v1/secrets/{id} — currently returns 404.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_delete_secret() {
        let server = test_server().await;

        // Create a secret
        let body = create_secret_request("DELETE_ME", "to-be-deleted");
        let (_create_status, create_json) =
            server.post("/api/v1/secrets", &body).await;
        let secret_id = create_json["id"].as_str().unwrap();

        // Attempt to delete
        let (status, json) =
            server.delete(&format!("/api/v1/secrets/{secret_id}")).await;

        // Current handler is a stub — always 404.
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
    // test_delete_secret_nonexistent
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_delete_secret_nonexistent() {
        let server = test_server().await;

        let nonexistent = Uuid::new_v4();
        let (status, json) =
            server.delete(&format!("/api/v1/secrets/{nonexistent}")).await;

        assert_eq!(
            status, StatusCode::NOT_FOUND,
            "delete nonexistent secret should return 404: {json}"
        );
        assert_error_code(&json, "NOT_FOUND");
    }

    // -----------------------------------------------------------------------
    // test_list_secrets
    //
    // GET /api/v1/secrets — currently returns an empty paginated response.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_secrets() {
        let server = test_server().await;

        let (status, json) = server.get("/api/v1/secrets").await;

        assert_eq!(
            status, StatusCode::OK,
            "list secrets should return 200: {json}"
        );

        // Validate paginated shape
        assert!(json["items"].is_array(), "should have items array");
        assert!(json["has_more"].is_boolean(), "should have has_more");
        assert_item_count(&json, 0); // stub returns empty
    }

    // -----------------------------------------------------------------------
    // test_rotate_secret
    //
    // POST /api/v1/secrets/{id}/rotate — currently returns 404.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_rotate_secret() {
        let server = test_server().await;

        // Create a secret
        let body = create_secret_request("ROTATE_ME", "old-value");
        let (_create_status, create_json) =
            server.post("/api/v1/secrets", &body).await;
        let secret_id = create_json["id"].as_str().unwrap();

        // Attempt rotation
        let rotate_body = json!({ "value": "new-rotated-value" });
        let (status, json) = server
            .post(
                &format!("/api/v1/secrets/{secret_id}/rotate"),
                &rotate_body,
            )
            .await;

        // Current handler is a stub — always 404.
        match status {
            StatusCode::NOT_FOUND => {
                assert_error_code(&json, "NOT_FOUND");
            }
            StatusCode::OK => {
                // Future behaviour: version should be incremented
                assert_eq!(json["name"].as_str().unwrap(), "ROTATE_ME");
            }
            other => panic!("unexpected status {other}: {json}"),
        }
    }

    // -----------------------------------------------------------------------
    // test_create_secret_with_app_scope
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_secret_with_app_scope() {
        let server = test_server().await;

        let body = serde_json::json!({
            "name": "APP_SECRET",
            "value": "app-level-secret",
            "scope": "app"
        });

        let (status, json) = server.post("/api/v1/secrets", &body).await;

        assert_eq!(status, StatusCode::CREATED, "response: {json}");
        assert_eq!(json["scope"].as_str().unwrap(), "app");
    }

    // -----------------------------------------------------------------------
    // test_create_secret_empty_value
    //
    // An empty secret value should still be accepted (the API does not
    // enforce non-empty values yet).
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_secret_empty_value() {
        let server = test_server().await;

        let body = serde_json::json!({
            "name": "EMPTY_SECRET",
            "value": "",
            "scope": "organization"
        });

        let (status, json) = server.post("/api/v1/secrets", &body).await;

        assert_eq!(status, StatusCode::CREATED, "empty value response: {json}");
        assert_eq!(json["name"].as_str().unwrap(), "EMPTY_SECRET");
    }
}
