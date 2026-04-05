//! Registry authentication and configuration types
//!
//! Types for authenticating with OCI-compliant registries and
//! configuring OCI client behavior.

use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use schemars::JsonSchema;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

use super::platform::Platform;

/// Authentication token from a registry
///
/// Tokens are obtained from the registry's authentication service
/// and are used for subsequent API requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct AuthToken {
    /// Token value for Authorization header
    pub token: String,

    /// Token type (usually "Bearer")
    #[serde(default = "default_token_type")]
    pub token_type: String,

    /// Expiration time in seconds from issuance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,

    /// When the token was issued (Unix timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issued_at: Option<u64>,

    /// Refresh token for obtaining new tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

impl AuthToken {
    /// Create a new auth token
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            token_type: "Bearer".to_string(),
            expires_in: None,
            issued_at: None,
            refresh_token: None,
        }
    }

    /// Create with expiration
    pub fn with_expires_in(mut self, seconds: u64) -> Self {
        self.expires_in = Some(seconds);
        self
    }

    /// Get the Authorization header value
    pub fn authorization_header(&self) -> String {
        format!("{} {}", self.token_type, self.token)
    }

    /// Check if this is a bearer token
    pub fn is_bearer(&self) -> bool {
        self.token_type.to_lowercase() == "bearer"
    }
}

/// Registry authentication credentials
///
/// Supports multiple authentication methods:
/// - Basic auth (username/password)
/// - Bearer token
/// - Anonymous (for public images)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct RegistryAuth {
    /// Username for basic authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    /// Password for basic authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Pre-existing bearer token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    /// Registry URL (for provider-specific auth like ECR, GCR)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_url: Option<String>,

    /// Identity token (for OIDC-based auth)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_token: Option<String>,
}

impl RegistryAuth {
    /// Create anonymous authentication (for public images)
    pub fn anonymous() -> Self {
        Self {
            username: None,
            password: None,
            token: None,
            registry_url: None,
            identity_token: None,
        }
    }

    /// Create basic authentication
    pub fn basic(username: &str, password: &str) -> Self {
        Self {
            username: Some(username.to_string()),
            password: Some(password.to_string()),
            token: None,
            registry_url: None,
            identity_token: None,
        }
    }

    /// Create token-based authentication
    pub fn token(token: &str) -> Self {
        Self {
            username: None,
            password: None,
            token: Some(token.to_string()),
            registry_url: None,
            identity_token: None,
        }
    }

    /// Create with registry URL
    pub fn with_registry_url(mut self, url: impl Into<String>) -> Self {
        self.registry_url = Some(url.into());
        self
    }

    /// Check if this is anonymous auth
    pub fn is_anonymous(&self) -> bool {
        self.username.is_none()
            && self.password.is_none()
            && self.token.is_none()
            && self.identity_token.is_none()
    }

    /// Check if using basic auth
    pub fn is_basic(&self) -> bool {
        self.username.is_some() && self.password.is_some()
    }

    /// Check if using token auth
    pub fn is_token(&self) -> bool {
        self.token.is_some()
    }

    /// Get base64-encoded basic auth header
    pub fn basic_auth_header(&self) -> Option<String> {
        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            let credentials = format!("{}:{}", user, pass);
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            Some(format!("Basic {}", STANDARD.encode(credentials)))
        } else {
            None
        }
    }
}

impl Default for RegistryAuth {
    fn default() -> Self {
        Self::anonymous()
    }
}

/// OCI client configuration
///
/// Configures the OCI client for pulling images from registries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct OciConfig {
    /// Registry host (e.g., "docker.io", "ghcr.io")
    pub registry: String,

    /// Username for authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    /// Password for authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Allow insecure (HTTP) connections
    #[serde(default)]
    pub insecure: bool,

    /// Target platform for multi-arch images
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,

    /// Skip TLS verification (dangerous!)
    #[serde(default)]
    pub skip_tls_verify: bool,

    /// Timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// User agent for requests
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
}

fn default_timeout() -> u64 {
    300
}

fn default_user_agent() -> String {
    "shellwego-oci/1.0".to_string()
}

impl OciConfig {
    /// Create a new OCI config for a registry
    pub fn new(registry: impl Into<String>) -> Self {
        Self {
            registry: registry.into(),
            username: None,
            password: None,
            insecure: false,
            platform: None,
            skip_tls_verify: false,
            timeout_secs: default_timeout(),
            user_agent: default_user_agent(),
        }
    }

    /// Create for Docker Hub
    pub fn docker_hub() -> Self {
        Self::new("registry-1.docker.io")
    }

    /// Create for GitHub Container Registry
    pub fn ghcr() -> Self {
        Self::new("ghcr.io")
    }

    /// Create for Google Container Registry
    pub fn gcr() -> Self {
        Self::new("gcr.io")
    }

    /// Create for AWS ECR
    pub fn ecr(account_id: &str, region: &str) -> Self {
        Self::new(format!("{}.dkr.ecr.{}.amazonaws.com", account_id, region))
    }

    /// Set credentials
    pub fn with_credentials(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }

    /// Set platform
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Enable insecure mode
    pub fn insecure(mut self) -> Self {
        self.insecure = true;
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Get the registry URL (with protocol)
    pub fn registry_url(&self) -> String {
        if self.insecure {
            format!("http://{}", self.registry)
        } else if self.registry.starts_with(':') {
            // Bare port, e.g. ":5000" → "https://:5000:443"
            format!("https://{}:443", self.registry)
        } else {
            format!("https://{}", self.registry)
        }
    }

    /// Convert to RegistryAuth
    pub fn to_auth(&self) -> RegistryAuth {
        match (&self.username, &self.password) {
            (Some(user), Some(pass)) => RegistryAuth::basic(user, pass),
            _ => RegistryAuth::anonymous(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_token_new() {
        let token = AuthToken::new("mytoken123");
        assert_eq!(token.token, "mytoken123");
        assert_eq!(token.token_type, "Bearer");
        assert!(token.is_bearer());
    }

    #[test]
    fn test_auth_token_header() {
        let token = AuthToken::new("abc123");
        assert_eq!(token.authorization_header(), "Bearer abc123");
    }

    #[test]
    fn test_registry_auth_anonymous() {
        let auth = RegistryAuth::anonymous();
        assert!(auth.is_anonymous());
        assert!(!auth.is_basic());
        assert!(!auth.is_token());
    }

    #[test]
    fn test_registry_auth_basic() {
        let auth = RegistryAuth::basic("user", "pass");
        assert!(!auth.is_anonymous());
        assert!(auth.is_basic());
        assert!(!auth.is_token());

        let header = auth.basic_auth_header().unwrap();
        assert!(header.starts_with("Basic "));
    }

    #[test]
    fn test_registry_auth_token() {
        let auth = RegistryAuth::token("mytoken");
        assert!(!auth.is_anonymous());
        assert!(!auth.is_basic());
        assert!(auth.is_token());
    }

    #[test]
    fn test_oci_config_new() {
        let config = OciConfig::new("ghcr.io");
        assert_eq!(config.registry, "ghcr.io");
        assert!(!config.insecure);
        assert_eq!(config.timeout_secs, 300);
    }

    #[test]
    fn test_oci_config_presets() {
        let docker = OciConfig::docker_hub();
        assert_eq!(docker.registry, "registry-1.docker.io");

        let ghcr = OciConfig::ghcr();
        assert_eq!(ghcr.registry, "ghcr.io");
    }

    #[test]
    fn test_oci_config_url() {
        let config = OciConfig::new("ghcr.io");
        assert_eq!(config.registry_url(), "https://ghcr.io");

        let insecure = OciConfig::new("localhost:5000").insecure();
        assert_eq!(insecure.registry_url(), "http://localhost:5000");
    }

    #[test]
    fn test_oci_config_to_auth() {
        let with_creds = OciConfig::new("ghcr.io")
            .with_credentials("user", "pass");
        let auth = with_creds.to_auth();
        assert!(auth.is_basic());

        let no_creds = OciConfig::new("ghcr.io");
        let auth = no_creds.to_auth();
        assert!(auth.is_anonymous());
    }
}
