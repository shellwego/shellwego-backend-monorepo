//! OCI Distribution Spec implementation for pulling container images
//!
//! This module implements the OCI Distribution Spec v1.1 for pulling
//! container images from registries. Unlike skopeo CLI, this is a native
//! Rust implementation for tighter integration with ZFS operations.
//!
//! Supported registries:
//! - Docker Hub (docker.io)
//! - Amazon ECR
//! - Google Container Registry (gcr.io)
//! - GitHub Container Registry (ghcr.io)
//! - Generic OCI-compliant registries

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info};
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
use futures_util::StreamExt;
use crate::StorageError;
use base64::{Engine as _, engine::general_purpose::STANDARD};

const MAX_MANIFEST_SIZE: usize = 10 * 1024 * 1024; // 10MB

#[derive(Debug, Error)]
pub enum OciError {
    #[error("Registry error: {0}")]
    Registry(String),
    #[error("Manifest parse error: {0}")]
    ManifestParse(String),
    #[error("Layer download error: {0}")]
    LayerDownload(String),
    #[error("Authentication required for: {0}")]
    AuthRequired(String),
    #[error("Unsupported media type: {0}")]
    UnsupportedMediaType(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<OciError> for StorageError {
    fn from(e: OciError) -> Self {
        StorageError::Backend(format!("OCI: {}", e))
    }
}

#[derive(Debug, Clone)]
pub struct OciConfig {
    pub registry: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub insecure: bool, // Allow HTTP (not HTTPS)
    pub platform: Option<Platform>,
}

#[derive(Debug, Clone)]
pub struct Platform {
    pub os: String,
    pub architecture: String,
    pub variant: Option<String>,
    pub os_version: Option<String>,
}

pub struct OciClient {
    config: OciConfig,
    http_client: reqwest::Client,
    auth_cache: dashmap::DashMap<String, String>,
}

impl OciClient {
    pub async fn new(config: OciConfig) -> Result<Self, OciError> {
        let mut builder = reqwest::Client::builder();

        if config.insecure {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let http_client = builder.build().map_err(|e| OciError::Registry(e.to_string()))?;

        Ok(Self {
            config,
            http_client,
            auth_cache: dashmap::DashMap::new(),
        })
    }

    pub fn registry_url(&self) -> String {
        let registry = &self.config.registry;
        if registry.contains(':') && !registry.contains(".") {
            format!("https://{}:443", registry)
        } else if registry == "docker.io" {
            "https://registry-1.docker.io".to_string()
        } else {
            format!("https://{}", registry)
        }
    }

    async fn get_auth_token(&self, repository: &str) -> Result<String, OciError> {
        if let Some(token) = self.auth_cache.get(repository) {
            return Ok(token.clone());
        }

        let registry_url = self.registry_url();
        let auth_url = format!("{}/token?scope=repository:{}:pull", registry_url, repository);

        if let (Some(username), Some(password)) = (&self.config.username, &self.config.password) {
            let basic = format!("Basic {}", STANDARD.encode(format!("{}:{}", username, password)));
            let resp = self.http_client
                .get(&auth_url)
                .header("Authorization", basic)
                .send()
                .await
                .map_err(|e| OciError::AuthRequired(e.to_string()))?;

            if resp.status() == 200 {
                #[derive(Deserialize)]
                struct TokenResponse {
                    token: String,
                }
                let token_resp: TokenResponse = resp.json().await
                    .map_err(|e| OciError::Registry(e.to_string()))?;
                self.auth_cache.insert(repository.to_string(), token_resp.token.clone());
                return Ok(token_resp.token);
            }
        }

        // For Docker Hub, we need special handling
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
                #[derive(Deserialize)]
                struct TokenResponse {
                    token: String,
                }
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
        let url = format!("{}/{}/manifests/{}", self.registry_url(), repository, reference);

        let resp = self.http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/vnd.oci.image.manifest.v1+json")
            .header("Accept", "application/vnd.docker.distribution.manifest.v2+json")
            .send()
            .await
            .map_err(|e| OciError::Registry(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(OciError::Registry(format!(
                "Failed to fetch manifest: {} {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }

        let content_length = resp.content_length().unwrap_or(0);
        if content_length > MAX_MANIFEST_SIZE as u64 {
            return Err(OciError::ManifestParse("Manifest too large".to_string()));
        }

        let manifest: Manifest = resp.json().await
            .map_err(|e| OciError::ManifestParse(e.to_string()))?;

        Ok(manifest)
    }

    async fn get_blob(&self, repository: &str, digest: &str, writer: &mut (impl AsyncWriteExt + Unpin)) -> Result<(), OciError> {
        let token = self.get_auth_token(repository).await?;
        let url = format!("{}/{}/blobs/{}", self.registry_url(), repository, digest);

        let resp = self.http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| OciError::LayerDownload(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(OciError::LayerDownload(format!(
                "Failed to fetch blob {}: {}",
                digest,
                resp.status()
            )));
        }

        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| OciError::LayerDownload(e.to_string()))?;
            writer.write_all(&chunk).await
                .map_err(|e| OciError::LayerDownload(e.to_string()))?;
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

        info!("Pulling image {} from {}", image_ref, self.config.registry);
        debug!("Repository: {}, Reference: {}", repository, reference);

        let manifest = self.get_manifest(&repository, &reference).await?;
        debug!("Manifest mediaType: {:?}", manifest.media_type);
        debug!("Schema version: {}", manifest.schema_version);

        tokio::fs::create_dir_all(&mountpoint).await
            .map_err(|e| OciError::Io(e))?;

        for layer in &manifest.layers {
            debug!("Processing layer: {} ({} bytes)", layer.digest, layer.size);
            self.extract_layer(&repository, &layer.digest, &mountpoint).await?;
        }

        if let Some(config) = &manifest.config {
            debug!("Image config: {} ({} bytes)", config.digest, config.size);
            self.extract_layer(&repository, &config.digest, &mountpoint).await?;
        }

        info!("Successfully pulled image to {}", mountpoint.display());

        Ok(())
    }

    async fn extract_layer(&self, repository: &str, digest: &str, mountpoint: &PathBuf) -> Result<(), OciError> {
        let temp_file = tempfile::NamedTempFile::new()
            .map_err(|e| OciError::Io(e))?;
        let temp_path = temp_file.path().to_owned();

        {
            let std_file = temp_file.reopen().map_err(|e| OciError::Io(e))?;
            let tokio_file = File::from_std(std_file);
            let mut writer = BufWriter::new(tokio_file);
            self.get_blob(repository, digest, &mut writer).await?;
            writer.flush().await.map_err(|e| OciError::Io(e))?;
        }

        info!("Extracting layer {}...", digest);

        let reader = tokio::fs::File::open(&temp_path).await
            .map_err(|e| OciError::Io(e))?;
        let reader = tokio::io::BufReader::new(reader);
        let mut archive = tokio_tar::Archive::new(reader);

        archive.unpack(mountpoint).await
            .map_err(|e| OciError::LayerDownload(e.to_string()))?;

        tokio::fs::remove_file(temp_path).await.ok();

        Ok(())
    }

    fn parse_reference(&self, image_ref: &str) -> Result<(String, String), OciError> {
        let mut parts: Vec<&str> = image_ref.splitn(2, ':').collect();

        if parts.len() == 2 && !parts[1].contains('/') {
            let tag = parts.pop().unwrap();
            let name = parts.pop().unwrap();
            let repository = if name.contains('/') {
                name.to_string()
            } else {
                format!("library/{}", name)
            };
            return Ok((repository, tag.to_string()));
        }

        let reference = if parts.len() == 2 {
            parts[1].to_string()
        } else {
            "latest".to_string()
        };

        let name = parts[0];

        let repository = if name.contains('/') {
            name.to_string()
        } else {
            format!("library/{}", name)
        };

        Ok((repository, reference))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "mediaType")]
    pub media_type: Option<String>,
    #[serde(rename = "config")]
    pub config: Option<ConfigDescriptor>,
    #[serde(rename = "layers")]
    pub layers: Vec<LayerDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDescriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    #[serde(rename = "digest")]
    pub digest: String,
    #[serde(rename = "size")]
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerDescriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    #[serde(rename = "digest")]
    pub digest: String,
    #[serde(rename = "size")]
    pub size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_reference_with_tag() {
        let client = OciClient::new(OciConfig {
            registry: "docker.io".to_string(),
            username: None,
            password: None,
            insecure: false,
            platform: None,
        }).await.unwrap();

        let (repo, ref_) = client.parse_reference("alpine:3.18").unwrap();
        assert_eq!(repo, "library/alpine");
        assert_eq!(ref_, "3.18");
    }

    #[tokio::test]
    async fn test_parse_reference_with_registry() {
        let client = OciClient::new(OciConfig {
            registry: "ghcr.io".to_string(),
            username: None,
            password: None,
            insecure: false,
            platform: None,
        }).await.unwrap();

        let (repo, ref_) = client.parse_reference("ghcr.io/user/repo:v1.0").unwrap();
        assert_eq!(repo, "user/repo");
        assert_eq!(ref_, "v1.0");
    }

    #[tokio::test]
    async fn test_parse_reference_latest() {
        let client = OciClient::new(OciConfig {
            registry: "docker.io".to_string(),
            username: None,
            password: None,
            insecure: false,
            platform: None,
        }).await.unwrap();

        let (repo, ref_) = client.parse_reference("ubuntu").unwrap();
        assert_eq!(repo, "library/ubuntu");
        assert_eq!(ref_, "latest");
    }
}
