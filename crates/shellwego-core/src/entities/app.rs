//! Application entity definitions.
//!
//! The core resource: deployable workloads running in Firecracker microVMs.
//!
//! When the `orm` feature is enabled, the `App` struct directly derives
//! Sea-ORM's entity traits, eliminating duplication between core and control-plane.

#[cfg(feature = "orm")]
use sea_orm::entity::prelude::*;

use crate::prelude::*;

// TODO: Add `utoipa::ToSchema` derive for OpenAPI generation
// TODO: Add `Validate` derive for input sanitization

/// Unique identifier for an App
pub type AppId = Uuid;

/// Application deployment status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
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

/// Resource allocation using canonical units (bytes and milli-CPU)
#[derive(Debug, Clone, Serialize, Deserialize, Validate, Default, PartialEq)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct ResourceSpec {
    /// Memory limit in bytes
    #[serde(default)]
    pub memory_bytes: u64,

    /// CPU limit in milli-CPU units (1000 = 1 full CPU)
    #[serde(default)]
    pub cpu_milli: u32,

    /// Disk allocation in bytes
    #[serde(default)]
    pub disk_bytes: u64,
}

impl ResourceSpec {
    pub fn memory_mb(&self) -> u64 {
        self.memory_bytes / (1024 * 1024)
    }

    pub fn cpu_shares(&self) -> u64 {
        (self.cpu_milli as u64) * 1024 / 1000
    }
}

/// CLI resource request with human-readable string parsing
/// String parsing (1g -> 1073741824) happens ONLY at CLI input layer
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct ResourceRequest {
    #[serde(default, deserialize_with = "deserialize_memory")]
    pub memory_bytes: u64,
    #[serde(default, deserialize_with = "deserialize_cpu")]
    pub cpu_milli: u32,
    #[serde(default, deserialize_with = "deserialize_disk")]
    pub disk_bytes: u64,
}

impl Default for ResourceRequest {
    fn default() -> Self {
        Self {
            memory_bytes: 256 * 1024 * 1024, // 256MB default
            cpu_milli: 500,                   // 0.5 CPU default
            disk_bytes: 5 * 1024 * 1024 * 1024, // 5GB default
        }
    }
}

fn deserialize_memory<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(s) => parse_memory(&s).map_err(serde::de::Error::custom),
        None => Ok(256 * 1024 * 1024), // default 256MB
    }
}

fn deserialize_cpu<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(s) => parse_cpu(&s).map_err(serde::de::Error::custom),
        None => Ok(500), // default 0.5 CPU
    }
}

fn deserialize_disk<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(s) => parse_memory(&s).map_err(serde::de::Error::custom),
        None => Ok(5 * 1024 * 1024 * 1024), // default 5GB
    }
}

/// Parse memory string to bytes (e.g., "512m", "2g", "1g", "256k")
pub fn parse_memory(s: &str) -> anyhow::Result<u64> {
    let s = s.trim().to_lowercase();
    let (num, unit) = s.split_at(
        s.find(|c: char| !c.is_ascii_digit() && c != '.')
            .ok_or_else(|| anyhow::anyhow!("Invalid memory format: {}", s))?,
    );
    let value: f64 = num.parse()?;
    match unit {
        "b" => Ok(value as u64),
        "k" | "kb" => Ok((value * 1024.0) as u64),
        "m" | "mb" => Ok((value * 1024.0 * 1024.0) as u64),
        "g" | "gb" => Ok((value * 1024.0 * 1024.0 * 1024.0) as u64),
        "t" | "tb" => Ok((value * 1024.0 * 1024.0 * 1024.0 * 1024.0) as u64),
        _ => anyhow::bail!("Unknown memory unit: {}", unit),
    }
}

/// Parse CPU string to milli-CPU (e.g., "0.5", "2.0", "1000m", "2")
pub fn parse_cpu(s: &str) -> anyhow::Result<u32> {
    let s = s.trim().to_lowercase();
    if s.ends_with('m') {
        let num: u32 = s.trim_end_matches('m').parse()?;
        return Ok(num);
    }
    let value: f64 = s.parse()?;
    Ok((value * 1000.0) as u32)
}

impl From<ResourceRequest> for ResourceSpec {
    fn from(req: ResourceRequest) -> Self {
        ResourceSpec {
            memory_bytes: req.memory_bytes,
            cpu_milli: req.cpu_milli,
            disk_bytes: req.disk_bytes,
        }
    }
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(DeriveEntityModel))]
#[cfg_attr(feature = "orm", sea_orm(table_name = "apps"))]
pub struct App {
    #[cfg_attr(feature = "orm", sea_orm(primary_key, auto_increment = false))]
    pub id: AppId,
    pub name: String,
    pub slug: String,
    pub status: AppStatus,
    pub image: String,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary", nullable))]
    pub command: Option<Vec<String>>,
    pub resources: ResourceSpec,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub env: Vec<EnvVar>,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub domains: Vec<DomainConfig>,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub volumes: Vec<VolumeMount>,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary", nullable))]
    pub health_check: Option<HealthCheck>,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub source: SourceSpec,
    pub organization_id: Uuid,
    pub created_by: Uuid,
    #[cfg_attr(feature = "orm", sea_orm(default_value = "sea_orm::prelude::DateTimeWithchrono::Utc::now()"))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "orm", sea_orm(default_value = "sea_orm::prelude::DateTimeWithchrono::Utc::now()"))]
    pub updated_at: DateTime<Utc>,
}

#[cfg(feature = "orm")]
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[cfg(feature = "orm")]
impl ActiveModelBehavior for ActiveModel {}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Starting,
    Healthy,
    Unhealthy,
    Stopping,
    Exited,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(DeriveEntityModel))]
#[cfg_attr(feature = "orm", sea_orm(table_name = "app_instances"))]
pub struct AppInstance {
    #[cfg_attr(feature = "orm", sea_orm(primary_key, auto_increment = false))]
    pub id: Uuid,
    pub app_id: AppId,
    pub node_id: Uuid,
    pub status: InstanceStatus,
    pub internal_ip: String,
    #[cfg_attr(feature = "orm", sea_orm(default_value = "sea_orm::prelude::DateTimeWithchrono::Utc::now()"))]
    pub started_at: DateTime<Utc>,
    pub health_checks_passed: u64,
    pub health_checks_failed: u64,
}

#[cfg(feature = "orm")]
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum AppInstanceRelation {}

#[cfg(feature = "orm")]
impl ActiveModelBehavior for AppInstanceActiveModel {}