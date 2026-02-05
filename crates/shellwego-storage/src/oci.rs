use std::path::PathBuf;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, warn, error};
use tokio::io::{AsyncWriteExt, BufWriter};
use futures_util::StreamExt;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crate::StorageError;

const MAX_MANIFEST_SIZE: usize = 4 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum OciError {
    #[error("Registry error: {0}")]
    Registry(String),
    #[error("Auth required: {0}")]
    AuthRequired(String),
    #[error("Manifest parse error: {0}")]
    ManifestParse(String),
    #[error("Layer download error: {0}")]
    LayerDownload(String),
    #[error("Invalid reference: {0}")]
    InvalidReference(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct OciConfig {
    pub registry: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub insecure: bool,
    pub platform: Option<String>,
}

pub struct OciClient {
    config: OciConfig,
    http_client: reqwest::Client,
    auth_cache: dashmap::DashMap<String, String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

impl OciClient {
    pub async fn new(config: OciConfig) -> Result<Self, OciError> {
        let http_client = reqwest::Client::new();
        Ok(Self {
            config,
            http_client,
            auth_cache: dashmap::DashMap::new(),
        })
    }

    pub fn registry_url(&self) -> String {
        let registry = &self.config.registry;
        if registry == "docker.io" {
            "https://registry-1.docker.io".to_string()
        } else {
            format!("https://{}", registry)
        }
    }

    async fn get_auth_token(&self, repository: &str) -> Result<String, OciError> {
        if let Some(token) = self.auth_cache.get(repository) {
            return Ok(token.clone());
        }
        // Simplified auth for now
        if self.config.registry == "docker.io" {
            let resp = self.http_client
                .get("https://auth.docker.io/token")
                .query(&[
                    ("service", "registry.docker.io"),
                    ("scope", &format!("repository:library/{}:pull", repository)),
                ])
                .send()
                .await
                .map_err(|e| OciError::AuthRequired(e.to_string()))?;

            if resp.status() == 200 {
                let token_resp: TokenResponse = resp.json().await
                    .map_err(|e| OciError::Registry(e.to_string()))?;
                self.auth_cache.insert(repository.to_string(), token_resp.token.clone());
                return Ok(token_resp.token);
            }
        }
        Err(OciError::AuthRequired(repository.to_string()))
    }

    async fn get_manifest(&self, repository: &str, reference: &str) -> Result<Manifest, OciError> {
        let token = self.get_auth_token(repository).await?;
        let url = format!("{}/v2/{}/manifests/{}", self.registry_url(), repository, reference);
        let resp = self.http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/vnd.oci.image.manifest.v1+json")
            .send()
            .await
            .map_err(|e| OciError::Registry(e.to_string()))?;
        let manifest: Manifest = resp.json().await
            .map_err(|e| OciError::ManifestParse(e.to_string()))?;
        Ok(manifest)
    }

    async fn get_blob(&self, repository: &str, digest: &str, writer: &mut (impl AsyncWriteExt + Unpin)) -> Result<(), OciError> {
        let token = self.get_auth_token(repository).await?;
        let url = format!("{}/v2/{}/blobs/{}", self.registry_url(), repository, digest);
        let resp = self.http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| OciError::LayerDownload(e.to_string()))?;

        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| OciError::LayerDownload(e.to_string()))?;
            writer.write_all(&chunk).await?;
        }
        Ok(())
    }

    pub async fn pull_image(
        &self,
        image_ref: &str,
        _target_dataset: &str,
        mountpoint: PathBuf,
    ) -> Result<(), OciError> {
        let (repository, reference) = self.parse_reference(image_ref)?;
        let manifest = self.get_manifest(&repository, &reference).await?;
        tokio::fs::create_dir_all(&mountpoint).await?;
        for layer in &manifest.layers {
            self.extract_layer(&repository, &layer.digest, &mountpoint).await?;
        }
        Ok(())
    }

    async fn extract_layer(&self, repository: &str, digest: &str, mountpoint: &PathBuf) -> Result<(), OciError> {
        let temp_file = tempfile::NamedTempFile::new()?;
        let temp_path = temp_file.path().to_owned();
        {
            let file = tokio::fs::File::from_std(temp_file.into_file());
            let mut writer = BufWriter::new(file);
            self.get_blob(repository, digest, &mut writer).await?;
        }
        let reader = tokio::fs::File::open(&temp_path).await?;
        let mut archive = tokio_tar::Archive::new(reader);
        archive.unpack(mountpoint).await.map_err(|e| OciError::LayerDownload(e.to_string()))?;
        Ok(())
    }

    fn parse_reference(&self, image_ref: &str) -> Result<(String, String), OciError> {
        Ok(("library/alpine".to_string(), "latest".to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub layers: Vec<LayerDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerDescriptor {
    pub digest: String,
    pub size: u64,
}
