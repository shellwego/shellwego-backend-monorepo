//! Content descriptors for OCI images
//!
//! Descriptors describe content-addressable references to blobs and manifests.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "openapi")]
use schemars::JsonSchema;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

use super::platform::Platform;

/// Content descriptor for addressing content in a registry
///
/// A descriptor describes the type, size, and digest of content.
/// Used for configs, layers, and manifests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct Descriptor {
    /// Media type of the referenced content
    #[serde(rename = "mediaType")]
    #[cfg_attr(feature = "openapi", schema(example = "application/vnd.oci.image.manifest.v1+json"))]
    pub media_type: Option<String>,

    /// Digest of the content (e.g., "sha256:abc123...")
    pub digest: String,

    /// Size in bytes
    pub size: u64,

    /// URLs for direct download (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,

    /// Annotations (optional)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,

    /// Platform for multi-arch manifests (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
}

impl Descriptor {
    /// Create a new descriptor
    pub fn new(digest: String, size: u64) -> Self {
        Self {
            media_type: None,
            digest,
            size,
            urls: Vec::new(),
            annotations: HashMap::new(),
            platform: None,
        }
    }

    /// Create a descriptor with media type
    pub fn with_media_type(mut self, media_type: impl Into<String>) -> Self {
        self.media_type = Some(media_type.into());
        self
    }

    /// Create a descriptor with platform
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Get the short digest (first 12 characters)
    pub fn short_digest(&self) -> &str {
        if self.digest.starts_with("sha256:") {
            &self.digest[7..19]
        } else {
            &self.digest
        }
    }
}

/// Config descriptor for image configuration
///
/// Points to the image's configuration blob containing
/// environment variables, entrypoint, cmd, etc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct ConfigDescriptor {
    /// Media type of the config blob
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(example = "application/vnd.oci.image.config.v1+json"))]
    pub media_type: Option<String>,

    /// Digest of the config blob
    pub digest: String,

    /// Size in bytes
    pub size: u64,
}

impl ConfigDescriptor {
    /// Create a new config descriptor
    pub fn new(digest: String, size: u64) -> Self {
        Self {
            media_type: None,
            digest,
            size,
        }
    }

    /// Create with media type
    pub fn with_media_type(mut self, media_type: impl Into<String>) -> Self {
        self.media_type = Some(media_type.into());
        self
    }
}

impl From<ConfigDescriptor> for Descriptor {
    fn from(config: ConfigDescriptor) -> Self {
        Self {
            media_type: config.media_type,
            digest: config.digest,
            size: config.size,
            urls: Vec::new(),
            annotations: HashMap::new(),
            platform: None,
        }
    }
}

impl From<Descriptor> for ConfigDescriptor {
    fn from(desc: Descriptor) -> Self {
        Self {
            media_type: desc.media_type,
            digest: desc.digest,
            size: desc.size,
        }
    }
}

/// Layer descriptor for image layers
///
/// Points to a layer blob (usually a tar.gz) that makes up
/// part of the container filesystem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct LayerDescriptor {
    /// Media type of the layer blob
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(example = "application/vnd.oci.image.layer.v1.tar+gzip"))]
    pub media_type: Option<String>,

    /// Digest of the layer blob
    pub digest: String,

    /// Size in bytes
    pub size: u64,

    /// URLs for direct download (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,

    /// Annotations (optional)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
}

impl LayerDescriptor {
    /// Create a new layer descriptor
    pub fn new(digest: String, size: u64) -> Self {
        Self {
            media_type: None,
            digest,
            size,
            urls: Vec::new(),
            annotations: HashMap::new(),
        }
    }

    /// Create with media type
    pub fn with_media_type(mut self, media_type: impl Into<String>) -> Self {
        self.media_type = Some(media_type.into());
        self
    }

    /// Check if this is a gzip-compressed layer
    pub fn is_gzip(&self) -> bool {
        self.media_type
            .as_ref()
            .map(|m| m.contains("gzip"))
            .unwrap_or(false)
    }
}

impl From<LayerDescriptor> for Descriptor {
    fn from(layer: LayerDescriptor) -> Self {
        Self {
            media_type: layer.media_type,
            digest: layer.digest,
            size: layer.size,
            urls: layer.urls,
            annotations: layer.annotations,
            platform: None,
        }
    }
}

impl From<Descriptor> for LayerDescriptor {
    fn from(desc: Descriptor) -> Self {
        Self {
            media_type: desc.media_type,
            digest: desc.digest,
            size: desc.size,
            urls: desc.urls,
            annotations: desc.annotations,
        }
    }
}

/// Manifest descriptor for manifest list/index entries
///
/// Used in multi-arch images to reference per-platform manifests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct ManifestDescriptor {
    /// Media type of the manifest
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(example = "application/vnd.oci.image.manifest.v1+json"))]
    pub media_type: Option<String>,

    /// Digest of the manifest
    pub digest: String,

    /// Size in bytes
    pub size: u64,

    /// Platform for this manifest (for multi-arch)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,

    /// Annotations (optional)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
}

impl ManifestDescriptor {
    /// Create a new manifest descriptor
    pub fn new(digest: String, size: u64) -> Self {
        Self {
            media_type: None,
            digest,
            size,
            platform: None,
            annotations: HashMap::new(),
        }
    }

    /// Create with platform
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }
}

impl From<ManifestDescriptor> for Descriptor {
    fn from(m: ManifestDescriptor) -> Self {
        Self {
            media_type: m.media_type,
            digest: m.digest,
            size: m.size,
            urls: Vec::new(),
            annotations: m.annotations,
            platform: m.platform,
        }
    }
}

impl From<Descriptor> for ManifestDescriptor {
    fn from(desc: Descriptor) -> Self {
        Self {
            media_type: desc.media_type,
            digest: desc.digest,
            size: desc.size,
            platform: desc.platform,
            annotations: desc.annotations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptor_new() {
        let desc = Descriptor::new("sha256:abc123".to_string(), 1024);
        assert_eq!(desc.digest, "sha256:abc123");
        assert_eq!(desc.size, 1024);
        assert!(desc.media_type.is_none());
    }

    #[test]
    fn test_short_digest() {
        let desc = Descriptor::new("sha256:0123456789abcdef0123456789abcdef0123456789abcdef".to_string(), 100);
        assert_eq!(desc.short_digest(), "0123456789ab");
    }

    #[test]
    fn test_layer_is_gzip() {
        let gzip_layer = LayerDescriptor::new("sha256:abc".to_string(), 100)
            .with_media_type("application/vnd.oci.image.layer.v1.tar+gzip");
        assert!(gzip_layer.is_gzip());

        let tar_layer = LayerDescriptor::new("sha256:abc".to_string(), 100)
            .with_media_type("application/vnd.oci.image.layer.v1.tar");
        assert!(!tar_layer.is_gzip());
    }

    #[test]
    fn test_config_descriptor_conversion() {
        let config = ConfigDescriptor::new("sha256:config".to_string(), 2048)
            .with_media_type("application/vnd.oci.image.config.v1+json");

        let desc: Descriptor = config.clone().into();
        assert_eq!(desc.digest, "sha256:config");
        assert_eq!(desc.size, 2048);

        let back: ConfigDescriptor = desc.into();
        assert_eq!(back.digest, config.digest);
    }
}
