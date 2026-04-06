//! Container image registry cache and pull operations
//!
//! Integrates with skopeo/umoci for OCI image handling.

use thiserror::Error;

pub mod cache;
pub mod pull;
pub mod mirror;
pub mod p2p;
pub mod gc;
pub mod distribution;

// Re-export OCI types from schema
pub use shellwego_schema::oci::{
    AuthToken, RegistryAuth, Manifest, ManifestIndex,
    ConfigDescriptor, LayerDescriptor, ManifestDescriptor,
    Platform, ImageConfig, ContainerConfig, RootFs, HistoryEntry,
    Descriptor, OciConfig,
};

// Re-export cache types
pub use cache::{LayerCache, CachedImageInfo, CacheStats, LayerInfo};

// Re-export pull types
pub use pull::{ImagePuller, PulledImage, PullProgress, ImageReference};

// Re-export mirror types
pub use mirror::MirrorChain;

// Re-export P2P types
pub use p2p::{DragonflyClient, PeerId, PeerInfo, PieceTracker, PieceScheduler};

// Re-export GC types
pub use gc::{GarbageCollector, GcConfig, GcResult, LayerRefCount};

// Re-export distribution types
pub use distribution::{DistributionManager, DistributionManagerBuilder, PullOptions};

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

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Mirror error: {0}")]
    Mirror(String),

    #[error("P2P error: {0}")]
    P2P(String),

    #[error("GC error: {0}")]
    Gc(String),
}

/// Registry backend trait for pluggable image sources
#[async_trait::async_trait]
pub trait RegistryBackend: Send + Sync {
    /// Authenticate to remote registry
    async fn authenticate(&self, creds: &RegistryAuth) -> Result<AuthToken, RegistryError>;

    /// Check if image exists in remote
    async fn exists(&self, image_ref: &str) -> Result<bool, RegistryError>;

    /// Pull image manifest
    async fn pull_manifest(&self, image_ref: &str) -> Result<Manifest, RegistryError>;

    /// Pull layer blob
    async fn pull_layer(&self, digest: &str) -> Result<Vec<u8>, RegistryError>;

    /// Get image config
    async fn get_config(&self, image_ref: &str) -> Result<ImageConfig, RegistryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_auth_anonymous() {
        let auth = RegistryAuth::anonymous();
        assert!(auth.is_anonymous());
    }

    #[test]
    fn test_registry_auth_basic() {
        let auth = RegistryAuth::basic("user", "pass");
        assert!(auth.is_basic());
    }

    #[test]
    fn test_registry_auth_token() {
        let auth = RegistryAuth::token("mytoken");
        assert!(auth.is_token());
    }

    #[test]
    fn test_manifest_deserialization() {
        let json = r#"{
            "schemaVersion": 2,
            "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
            "config": {
                "mediaType": "application/vnd.docker.container.image.v1+json",
                "digest": "sha256:abc123",
                "size": 1234
            },
            "layers": []
        }"#;

        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.schema_version, 2);
    }

    #[test]
    fn test_image_config_default() {
        let config = ImageConfig::default();
        assert_eq!(config.architecture, "amd64");
        assert_eq!(config.os, "linux");
    }

    #[test]
    fn test_container_config_env() {
        let config = ContainerConfig::new();
        assert!(config.Env.is_empty());
    }
}
