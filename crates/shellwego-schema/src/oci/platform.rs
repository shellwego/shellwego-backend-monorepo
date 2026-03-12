//! Platform specification for multi-architecture images
//!
//! Platform types define the target OS and architecture for container images.

use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use schemars::JsonSchema;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Platform specification for multi-arch images
///
/// Defines the target operating system and CPU architecture.
/// Used in manifest indices to select the appropriate image variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct Platform {
    /// CPU architecture (e.g., "amd64", "arm64", "riscv64")
    pub architecture: String,

    /// Operating system (e.g., "linux", "windows", "darwin")
    pub os: String,

    /// CPU variant (e.g., "v7" for arm)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,

    /// OS version (e.g., "10.0.17763.1756" for Windows)
    #[serde(skip_serializing_if = "Option::is_none", rename = "os.version")]
    pub os_version: Option<String>,

    /// OS features (e.g., "win32k")
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "os.features")]
    pub os_features: Vec<String>,
}

impl Platform {
    /// Create a new platform specification
    pub fn new(architecture: impl Into<String>, os: impl Into<String>) -> Self {
        Self {
            architecture: architecture.into(),
            os: os.into(),
            variant: None,
            os_version: None,
            os_features: Vec::new(),
        }
    }

    /// Create with variant
    pub fn with_variant(mut self, variant: impl Into<String>) -> Self {
        self.variant = Some(variant.into());
        self
    }

    /// Create with OS version
    pub fn with_os_version(mut self, version: impl Into<String>) -> Self {
        self.os_version = Some(version.into());
        self
    }

    /// Linux AMD64 platform
    pub fn linux_amd64() -> Self {
        Self::new("amd64", "linux")
    }

    /// Linux ARM64 platform
    pub fn linux_arm64() -> Self {
        Self::new("arm64", "linux")
    }

    /// Linux ARM v7 platform
    pub fn linux_arm_v7() -> Self {
        Self::new("arm", "linux").with_variant("v7")
    }

    /// Check if this is a Linux platform
    pub fn is_linux(&self) -> bool {
        self.os == "linux"
    }

    /// Check if this is a Windows platform
    pub fn is_windows(&self) -> bool {
        self.os == "windows"
    }

    /// Check if this is an AMD64/x86_64 architecture
    pub fn is_amd64(&self) -> bool {
        self.architecture == "amd64" || self.architecture == "x86_64"
    }

    /// Check if this is an ARM64 architecture
    pub fn is_arm64(&self) -> bool {
        self.architecture == "arm64" || self.architecture == "aarch64"
    }
}

impl Default for Platform {
    fn default() -> Self {
        Self::linux_amd64()
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.os, self.architecture)?;
        if let Some(ref variant) = self.variant {
            write!(f, "/{}", variant)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_new() {
        let platform = Platform::new("arm64", "linux");
        assert_eq!(platform.architecture, "arm64");
        assert_eq!(platform.os, "linux");
        assert!(platform.variant.is_none());
    }

    #[test]
    fn test_platform_presets() {
        let amd64 = Platform::linux_amd64();
        assert!(amd64.is_linux());
        assert!(amd64.is_amd64());

        let arm64 = Platform::linux_arm64();
        assert!(arm64.is_linux());
        assert!(arm64.is_arm64());
    }

    #[test]
    fn test_platform_display() {
        let platform = Platform::linux_amd64();
        assert_eq!(format!("{}", platform), "linux/amd64");

        let armv7 = Platform::linux_arm_v7();
        assert_eq!(format!("{}", armv7), "linux/arm/v7");
    }

    #[test]
    fn test_platform_serialization() {
        let platform = Platform::linux_arm_v7();
        let json = serde_json::to_string(&platform).unwrap();
        assert!(json.contains("\"architecture\":\"arm\""));
        assert!(json.contains("\"variant\":\"v7\""));

        let decoded: Platform = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, platform);
    }

    #[test]
    fn test_platform_default() {
        let platform = Platform::default();
        assert_eq!(platform.os, "linux");
        assert_eq!(platform.architecture, "amd64");
    }
}
