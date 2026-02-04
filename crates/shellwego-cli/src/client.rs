//! HTTP API client with typed methods

use reqwest::{Client, Response, StatusCode};
use serde::de::DeserializeOwned;
use std::time::Duration;

use shellwego_core::entities::{
    app::{App, CreateAppRequest, UpdateAppRequest},
    node::Node,
    volume::{Volume, CreateVolumeRequest},
    domain::{Domain, CreateDomainRequest},
    database::{Database, CreateDatabaseRequest},
    secret::{Secret, CreateSecretRequest},
};

/// Typed API client
pub struct ApiClient {
    client: Client,
    base_url: String,
    token: String,
}

impl ApiClient {
    pub fn new(base_url: &str, token: &str) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
            
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        })
    }

    // === Apps ===

    pub async fn list_apps(&self) -> anyhow::Result<Vec<App>> {
        self.get("/v1/apps").await
    }

    pub async fn create_app(&self, req: &CreateAppRequest) -> anyhow::Result<App> {
        self.post("/v1/apps", req).await
    }

    pub async fn get_app(&self, id: uuid::Uuid) -> anyhow::Result<App> {
        self.get(&format!("/v1/apps/{}", id)).await
    }

    pub async fn update_app(&self, id: uuid::Uuid, req: &UpdateAppRequest) -> anyhow::Result<App> {
        self.patch(&format!("/v1/apps/{}", id), req).await
    }

    pub async fn delete_app(&self, id: uuid::Uuid) -> anyhow::Result<()> {
        self.delete(&format!("/v1/apps/{}", id)).await
    }

    pub async fn deploy_app(&self, id: uuid::Uuid, image: &str) -> anyhow::Result<()> {
        self.post::<_, serde_json::Value>(
            &format!("/v1/apps/{}/deploy", id),
            &serde_json::json!({ "image": image }),
        ).await?;
        Ok(())
    }

    pub async fn scale_app(&self, id: uuid::Uuid, replicas: u32) -> anyhow::Result<()> {
        self.post::<_, serde_json::Value>(
            &format!("/v1/apps/{}/scale", id),
            &serde_json::json!({ "replicas": replicas }),
        ).await?;
        Ok(())
    }

    pub async fn get_logs(&self, id: uuid::Uuid, follow: bool) -> anyhow::Result<String> {
        let url = format!("{}/v1/apps/{}/logs?follow={}", self.base_url, id, follow);
        
        let resp = self.client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;
            
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("API error: {}", resp.status()));
        }
        
        Ok(resp.text().await?)
    }

    // === Nodes ===

    pub async fn list_nodes(&self) -> anyhow::Result<Vec<Node>> {
        self.get("/v1/nodes").await
    }

    // === Volumes ===

    pub async fn list_volumes(&self) -> anyhow::Result<Vec<Volume>> {
        self.get("/v1/volumes").await
    }

    pub async fn create_volume(&self, req: &CreateVolumeRequest) -> anyhow::Result<Volume> {
        self.post("/v1/volumes", req).await
    }

    // === Domains ===

    pub async fn list_domains(&self) -> anyhow::Result<Vec<Domain>> {
        self.get("/v1/domains").await
    }

    pub async fn create_domain(&self, req: &CreateDomainRequest) -> anyhow::Result<Domain> {
        self.post("/v1/domains", req).await
    }

    // === Databases ===

    pub async fn list_databases(&self) -> anyhow::Result<Vec<Database>> {
        self.get("/v1/databases").await
    }

    pub async fn create_database(&self, req: &CreateDatabaseRequest) -> anyhow::Result<Database> {
        self.post("/v1/databases", req).await
    }

    // === Secrets ===

    pub async fn list_secrets(&self) -> anyhow::Result<Vec<Secret>> {
        self.get("/v1/secrets").await
    }

    pub async fn create_secret(&self, req: &CreateSecretRequest) -> anyhow::Result<Secret> {
        self.post("/v1/secrets", req).await
    }

    // === Auth ===

    pub async fn login(&self, email: &str, password: &str) -> anyhow::Result<String> {
        let resp: serde_json::Value = self.post("/v1/auth/token", &serde_json::json!({
            "email": email,
            "password": password,
        })).await?;
        
        resp.get("token")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("No token in response"))
    }

    pub async fn get_user(&self) -> anyhow::Result<serde_json::Value> {
        self.get("/v1/user").await
    }

    // === Generic HTTP methods ===

    async fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;
            
        self.handle_response(resp).await
    }

    async fn post<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> anyhow::Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.client
            .post(&url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await?;
            
        self.handle_response(resp).await
    }

    async fn patch<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> anyhow::Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.client
            .patch(&url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await?;
            
        self.handle_response(resp).await
    }

    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.client
            .delete(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;
            
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("API error: {}", resp.status()));
        }
        
        Ok(())
    }

    async fn handle_response<T: DeserializeOwned>(&self, resp: Response) -> anyhow::Result<T> {
        let status = resp.status();
        
        if status.is_success() {
            Ok(resp.json().await?)
        } else {
            let text = resp.text().await?;
            Err(anyhow::anyhow!("HTTP {}: {}", status, text))
        }
    }
}