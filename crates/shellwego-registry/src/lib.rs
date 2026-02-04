//! Container image registry cache and pull operations
//! 
//! Integrates with skopeo/umoci for OCI image handling.

use thiserror::Error;

pub mod cache;
pub mod pull;

#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("Image not found: {0}")]
    NotFound(String),
    
    #[error("Pull failed: {0}")]
    PullFailed(String),
    
    #[error("Cache corrupted: {0}")]
    CacheCorrupted(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Registry backend trait for pluggable image sources
#[async_trait::async_trait]
pub trait RegistryBackend: Send + Sync {
    // TODO: Authenticate to remote registry
    // fn authenticate(&self, creds: &RegistryAuth) -> Result<AuthToken, RegistryError>;
    
    // TODO: Check if image exists in remote
    // async fn exists(&self, image_ref: &str) -> Result<bool, RegistryError>;
    
    // TODO: Pull image manifest
    // async fn pull_manifest(&self, image_ref: &str) -> Result<Manifest, RegistryError>;
    
    // TODO: Pull layer blob
    // async fn pull_layer(&self, digest: &str) -> Result<Bytes, RegistryError>;
    
    // TODO: Get image config
    // async fn get_config(&self, image_ref: &str) -> Result<ImageConfig, RegistryError>;
}

/// OCI image manifest structure
#[derive(Debug, Clone)]
pub struct Manifest {
    // TODO: Add schemaVersion, mediaType, layers, config
}

/// Image configuration (env, cmd, entrypoint, etc)
#[derive(Debug, Clone)]
pub struct ImageConfig {
    // TODO: Add Env, Cmd, Entrypoint, WorkingDir, User, ExposedPorts
}

/// Registry authentication credentials
#[derive(Debug, Clone)]
pub struct RegistryAuth {
    // TODO: Add username, password, token, registry_url
}