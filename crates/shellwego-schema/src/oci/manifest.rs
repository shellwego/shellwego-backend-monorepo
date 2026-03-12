//! OCI Image Manifest and Index types
//!
//! The manifest defines the layers and configuration of an image.
//! The index (manifest list) enables multi-architecture images.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "openapi")]
use schemars::JsonSchema;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

use super::descriptor::{ConfigDescriptor, LayerDescriptor, ManifestDescriptor};

fn default_schema_version() -> i32 {
    2
}

/// OCI Image Manifest
///
/// The manifest is the "entrypoint" for an image, listing its config
/// and layers. It is the primary unit of distribution.
///
/// # Example
///
/// ```json
/// {
///   "schemaVersion": 2,
///   "mediaType": "application/vnd.oci.image.manifest.v1+json",
///   "config": {
///     "mediaType": "application/vnd.oci.image.config.v1+json",
///     "digest": "sha256:b5b2b...",
///     "size": 7023
///   },
///   "layers": [
///     {
///       "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
///       "digest": "sha256:983487...",
///       "size": 32654
///     }
///   ]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct Manifest {
    /// Schema version (typically 2)
    #[serde(rename = "schemaVersion", default = "default_schema_version")]
    pub schema_version: i32,

    /// Media type of this manifest
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,

    /// Image configuration descriptor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ConfigDescriptor>,

    /// Layer descriptors (ordered from base to top)
    #[serde(default)]
    pub layers: Vec<LayerDescriptor>,

    /// Manifest list entries (for multi-arch index)
    #[serde(default)]
    pub manifests: Vec<ManifestDescriptor>,

    /// Annotations (key-value metadata)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
}

impl Manifest {
    /// Create a new manifest
    pub fn new() -> Self {
        Self {
            schema_version: 2,
            media_type: None,
            config: None,
            layers: Vec::new(),
            manifests: Vec::new(),
            annotations: HashMap::new(),
        }
    }

    /// Create an OCI image manifest
    pub fn oci() -> Self {
        Self {
            media_type: Some("application/vnd.oci.image.manifest.v1+json".to_string()),
            ..Self::new()
        }
    }

    /// Create a Docker v2 manifest
    pub fn docker_v2() -> Self {
        Self {
            media_type: Some("application/vnd.docker.distribution.manifest.v2+json".to_string()),
            ..Self::new()
        }
    }

    /// Set the config descriptor
    pub fn with_config(mut self, config: ConfigDescriptor) -> Self {
        self.config = Some(config);
        self
    }

    /// Add a layer
    pub fn add_layer(mut self, layer: LayerDescriptor) -> Self {
        self.layers.push(layer);
        self
    }

    /// Add multiple layers
    pub fn with_layers(mut self, layers: Vec<LayerDescriptor>) -> Self {
        self.layers = layers;
        self
    }

    /// Add an annotation
    pub fn add_annotation(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.annotations.insert(key.into(), value.into());
        self
    }

    /// Get total size of all layers
    pub fn total_layer_size(&self) -> u64 {
        self.layers.iter().map(|l| l.size).sum()
    }

    /// Get total size including config
    pub fn total_size(&self) -> u64 {
        self.total_layer_size() + self.config.as_ref().map(|c| c.size).unwrap_or(0)
    }

    /// Get the number of layers
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Check if this is a manifest index (has multiple manifests)
    pub fn is_index(&self) -> bool {
        !self.manifests.is_empty()
    }

    /// Get the config digest
    pub fn config_digest(&self) -> Option<&str> {
        self.config.as_ref().map(|c| c.digest.as_str())
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::new()
    }
}

/// OCI Image Index (Manifest List)
///
/// An index references multiple manifests for different platforms.
/// This enables multi-architecture image distribution.
///
/// # Example
///
/// ```json
/// {
///   "schemaVersion": 2,
///   "mediaType": "application/vnd.oci.image.index.v1+json",
///   "manifests": [
///     {
///       "mediaType": "application/vnd.oci.image.manifest.v1+json",
///       "digest": "sha256:...",
///       "size": 7143,
///       "platform": {
///         "architecture": "amd64",
///         "os": "linux"
///       }
///     },
///     {
///       "mediaType": "application/vnd.oci.image.manifest.v1+json",
///       "digest": "sha256:...",
///       "size": 7685,
///       "platform": {
///         "architecture": "arm64",
///         "os": "linux"
///       }
///     }
///   ]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct ManifestIndex {
    /// Schema version (typically 2)
    #[serde(rename = "schemaVersion", default = "default_schema_version")]
    pub schema_version: i32,

    /// Media type of this index
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,

    /// Manifest descriptors for each platform
    #[serde(default)]
    pub manifests: Vec<ManifestDescriptor>,

    /// Annotations (key-value metadata)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
}

impl ManifestIndex {
    /// Create a new manifest index
    pub fn new() -> Self {
        Self {
            schema_version: 2,
            media_type: None,
            manifests: Vec::new(),
            annotations: HashMap::new(),
        }
    }

    /// Create an OCI image index
    pub fn oci() -> Self {
        Self {
            media_type: Some("application/vnd.oci.image.index.v1+json".to_string()),
            ..Self::new()
        }
    }

    /// Create a Docker manifest list
    pub fn docker_list() -> Self {
        Self {
            media_type: Some("application/vnd.docker.distribution.manifest.list.v2+json".to_string()),
            ..Self::new()
        }
    }

    /// Add a manifest for a platform
    pub fn add_manifest(mut self, manifest: ManifestDescriptor) -> Self {
        self.manifests.push(manifest);
        self
    }

    /// Find manifest for a specific platform
    pub fn find_for_platform(&self, os: &str, arch: &str) -> Option<&ManifestDescriptor> {
        self.manifests.iter().find(|m| {
            m.platform
                .as_ref()
                .map(|p| p.os == os && p.architecture == arch)
                .unwrap_or(false)
        })
    }

    /// Get all supported architectures for an OS
    pub fn architectures_for_os(&self, os: &str) -> Vec<&str> {
        self.manifests
            .iter()
            .filter_map(|m| {
                m.platform
                    .as_ref()
                    .and_then(|p| if p.os == os { Some(p.architecture.as_str()) } else { None })
            })
            .collect()
    }
}

impl Default for ManifestIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_new() {
        let manifest = Manifest::new();
        assert_eq!(manifest.schema_version, 2);
        assert!(manifest.media_type.is_none());
        assert!(manifest.config.is_none());
        assert!(manifest.layers.is_empty());
    }

    #[test]
    fn test_manifest_with_layers() {
        let manifest = Manifest::new()
            .with_config(ConfigDescriptor::new("sha256:config".to_string(), 100))
            .add_layer(LayerDescriptor::new("sha256:layer1".to_string(), 500))
            .add_layer(LayerDescriptor::new("sha256:layer2".to_string(), 300));

        assert_eq!(manifest.layer_count(), 2);
        assert_eq!(manifest.total_layer_size(), 800);
        assert_eq!(manifest.total_size(), 900);
    }

    #[test]
    fn test_manifest_annotations() {
        let manifest = Manifest::new()
            .add_annotation("org.opencontainers.image.title", "myimage")
            .add_annotation("org.opencontainers.image.version", "1.0.0");

        assert_eq!(manifest.annotations.len(), 2);
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
            "layers": [
                {
                    "mediaType": "application/vnd.docker.image.rootfs.diff.tar.gzip",
                    "digest": "sha256:layer1",
                    "size": 5000
                }
            ]
        }"#;

        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.schema_version, 2);
        assert!(manifest.config.is_some());
        assert_eq!(manifest.layers.len(), 1);
    }

    #[test]
    fn test_manifest_index_new() {
        let index = ManifestIndex::oci();
        assert_eq!(index.schema_version, 2);
        assert!(index.media_type.is_some());
    }

    #[test]
    fn test_manifest_index_find_platform() {
        use crate::oci::Platform;

        let index = ManifestIndex::oci()
            .add_manifest(
                ManifestDescriptor::new("sha256:amd64manifest".to_string(), 1000)
                    .with_platform(Platform::linux_amd64())
            )
            .add_manifest(
                ManifestDescriptor::new("sha256:arm64manifest".to_string(), 1100)
                    .with_platform(Platform::linux_arm64())
            );

        let found = index.find_for_platform("linux", "amd64");
        assert!(found.is_some());
        assert_eq!(found.unwrap().digest, "sha256:amd64manifest");

        let archs = index.architectures_for_os("linux");
        assert_eq!(archs.len(), 2);
    }
}
