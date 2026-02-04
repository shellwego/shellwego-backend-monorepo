//! Application entity definitions.
//! 
//! The core resource: deployable workloads running in Firecracker microVMs.

use crate::prelude::*;

// TODO: Add `utoipa::ToSchema` derive for OpenAPI generation
// TODO: Add `Validate` derive for input sanitization

/// Unique identifier for an App
pub type AppId = Uuid;

/// Application deployment status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum AppStatus {
    Creating,
    Deploying,
    Running,
    Stopped,
    Error,
    Paused,
    Draining,
}

/// Resource allocation for an App
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct ResourceSpec {
    /// Memory limit (e.g., "512m", "2g")
    // TODO: Validate format with regex
    pub memory: String,
    
    /// CPU cores (e.g., "0.5", "2.0")
    pub cpu: String,
    
    /// Disk allocation
    #[serde(default)]
    pub disk: Option<String>,
}

/// Environment variable with optional encryption
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct EnvVar {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub encrypted: bool,
}

/// Domain configuration attached to an App
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct DomainConfig {
    pub hostname: String,
    #[serde(default)]
    pub tls_enabled: bool,
    // TODO: Add path-based routing, headers, etc.
}

/// Persistent volume mount
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct VolumeMount {
    pub volume_id: Uuid,
    pub mount_path: String,
    #[serde(default)]
    pub read_only: bool,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct HealthCheck {
    pub path: String,
    pub port: u16,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_retries")]
    pub retries: u32,
}

fn default_interval() -> u64 { 10 }
fn default_timeout() -> u64 { 5 }
fn default_retries() -> u32 { 3 }

/// Source code origin for deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceSpec {
    Git {
        repository: String,
        #[serde(default)]
        branch: Option<String>,
        #[serde(default)]
        commit: Option<String>,
    },
    Docker {
        image: String,
        #[serde(default)]
        registry_auth: Option<RegistryAuth>,
    },
    Tarball {
        url: String,
        checksum: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct RegistryAuth {
    pub username: String,
    // TODO: This should be a secret reference, not inline
    pub password: String,
}

/// Main Application entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct App {
    pub id: AppId,
    pub name: String,
    pub slug: String,
    pub status: AppStatus,
    pub image: String,
    #[serde(default)]
    pub command: Option<Vec<String>>,
    pub resources: ResourceSpec,
    #[serde(default)]
    pub env: Vec<EnvVar>,
    #[serde(default)]
    pub domains: Vec<DomainConfig>,
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    #[serde(default)]
    pub health_check: Option<HealthCheck>,
    pub source: SourceSpec,
    pub organization_id: Uuid,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // TODO: Add replica count, networking policy, tags
}

/// Request to create a new App
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct CreateAppRequest {
    #[validate(length(min = 1, max = 64))]
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub command: Option<Vec<String>>,
    pub resources: ResourceSpec,
    #[serde(default)]
    pub env: Vec<EnvVar>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    #[serde(default)]
    pub health_check: Option<HealthCheck>,
    #[serde(default)]
    pub replicas: u32,
}

/// Request to update an App (partial)
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct UpdateAppRequest {
    #[validate(length(min = 1, max = 64))]
    pub name: Option<String>,
    pub resources: Option<ResourceSpec>,
    #[serde(default)]
    pub env: Option<Vec<EnvVar>>,
    pub replicas: Option<u32>,
    // TODO: Add other mutable fields
}

/// App instance (runtime representation)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct AppInstance {
    pub id: Uuid,
    pub app_id: AppId,
    pub node_id: Uuid,
    pub status: InstanceStatus,
    pub internal_ip: String,
    pub started_at: DateTime<Utc>,
    pub health_checks_passed: u64,
    pub health_checks_failed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Starting,
    Healthy,
    Unhealthy,
    Stopping,
    Exited,
}