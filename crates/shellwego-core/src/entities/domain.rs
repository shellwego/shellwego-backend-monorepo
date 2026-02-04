//! Domain and TLS certificate entity definitions.
//!
//! Edge routing and SSL termination configuration.

use crate::prelude::*;

#[cfg(feature = "orm")]
use sea_orm::entity::prelude::*;

pub type DomainId = Uuid;

/// Domain verification status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum DomainStatus {
    Pending,
    Active,
    Error,
    Expired,
    Suspended,
}

/// TLS certificate status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
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
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct DnsValidation {
    pub record_type: String,
    pub name: String,
    pub value: String,
}

/// Routing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(DeriveEntityModel))]
#[cfg_attr(feature = "orm", sea_orm(table_name = "domains"))]
pub struct Domain {
    #[cfg_attr(feature = "orm", sea_orm(primary_key, auto_increment = false))]
    pub id: DomainId,
    pub hostname: String,
    pub status: DomainStatus,
    pub tls_status: TlsStatus,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary", nullable))]
    pub certificate: Option<TlsCertificate>,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary", nullable))]
    pub validation: Option<DnsValidation>,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub routing: RoutingConfig,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub features: EdgeFeatures,
    pub organization_id: Uuid,
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

/// Create domain request
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct UploadCertificateRequest {
    pub certificate: String,
    pub private_key: String,
    #[serde(default)]
    pub chain: Option<String>,
}