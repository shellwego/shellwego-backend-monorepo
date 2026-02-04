//! Domain and TLS certificate entity definitions.
//! 
//! Edge routing and SSL termination configuration.

use crate::prelude::*;

pub type DomainId = Uuid;

/// Domain verification status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DomainStatus {
    Pending,
    Active,
    Error,
    Expired,
    Suspended,
}

/// TLS certificate status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TlsStatus {
    Pending,
    Provisioning,
    Active,
    ExpiringSoon,
    Expired,
    Failed,
}

/// TLS certificate details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TlsCertificate {
    pub issuer: String,
    pub subject: String,
    pub sans: Vec<String>,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub auto_renew: bool,
}

/// DNS validation record (for ACME)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DnsValidation {
    pub record_type: String, // CNAME, TXT, A
    pub name: String,
    pub value: String,
}

/// Routing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RoutingConfig {
    pub app_id: Uuid,
    pub port: u16,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub strip_prefix: bool,
    #[serde(default)]
    pub preserve_host: bool,
}

/// CDN/WAF features
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EdgeFeatures {
    #[serde(default)]
    pub cdn_enabled: bool,
    #[serde(default)]
    pub cache_ttl_seconds: u64,
    #[serde(default)]
    pub waf_enabled: bool,
    #[serde(default)]
    pub ddos_protection: bool,
}

/// Domain entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Domain {
    pub id: DomainId,
    pub hostname: String,
    pub status: DomainStatus,
    pub tls_status: TlsStatus,
    #[serde(default)]
    pub certificate: Option<TlsCertificate>,
    #[serde(default)]
    pub validation: Option<DnsValidation>,
    pub routing: RoutingConfig,
    pub features: EdgeFeatures,
    pub organization_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create domain request
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateDomainRequest {
    #[validate(hostname)]
    pub hostname: String,
    pub app_id: Uuid,
    pub port: u16,
    #[serde(default)]
    pub tls: bool,
    #[serde(default)]
    pub cdn: bool,
}

/// Upload custom certificate request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UploadCertificateRequest {
    pub certificate: String,
    pub private_key: String,
    #[serde(default)]
    pub chain: Option<String>,
}