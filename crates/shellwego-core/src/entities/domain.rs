//! Domain and TLS certificate entity definitions.

use crate::prelude::*;
#[cfg(feature = "orm")]
use sea_orm::entity::prelude::*;
#[cfg(feature = "orm")]
use sea_query::IdenStatic;

pub type DomainId = Uuid;

/// Domain operational status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_orm::EnumIter, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum DomainStatus {
    #[cfg_attr(feature = "orm", sea_orm(string_value = "pending"))]
    Pending,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "validating"))]
    Validating,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "active"))]
    Active,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "expired"))]
    Expired,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "error"))]
    Error,
}

/// TLS certificate status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_orm::EnumIter, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum TlsStatus {
    #[cfg_attr(feature = "orm", sea_orm(string_value = "none"))]
    None,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "issuing"))]
    Issuing,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "active"))]
    Active,
    #[cfg_attr(feature = "orm", sea_orm(string_value = "failed"))]
    Failed,
}

/// TLS certificate details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromJsonQueryResult))]
pub struct TlsCertificate {
    pub issuer: String,
    pub expires_at: chrono::DateTime<Utc>,
    pub serial_number: String,
}

/// DNS validation record (for ACME)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromJsonQueryResult))]
pub struct DnsValidation {
    pub record_type: String,
    pub name: String,
    pub value: String,
}

/// Routing configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromJsonQueryResult))]
pub struct RoutingConfig {
    pub app_id: Uuid,
    pub path_prefix: String,
    pub target_port: u16,
}

/// Edge performance and security features
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromJsonQueryResult))]
pub struct EdgeFeatures {
    pub waf_enabled: bool,
    pub compression_enabled: bool,
    pub caching_enabled: bool,
}

/// Custom Domain entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(DeriveEntityModel))]
#[cfg_attr(feature = "orm", sea_orm(table_name = "domains"))]
pub struct Model {
    #[cfg_attr(feature = "orm", sea_orm(primary_key, auto_increment = false))]
    pub id: DomainId,
    pub hostname: String,
    pub status: DomainStatus,
    pub tls_status: TlsStatus,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary", nullable))]
    pub certificate: Option<TlsCertificate>,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary", nullable))]
    pub validation: Option<DnsValidation>,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub routing: RoutingConfig,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub features: EdgeFeatures,
    pub organization_id: Uuid,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[cfg(feature = "orm")]
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[cfg(feature = "orm")]
impl ActiveModelBehavior for ActiveModel {}

/// Create domain request
#[derive(Debug, Clone, Serialize, Deserialize, Validate, PartialEq)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
pub struct CreateDomainRequest {
    pub hostname: String,
    pub app_id: Uuid,
    #[serde(default)]
    pub tls_enabled: bool,
}

/// Upload certificate request
#[derive(Debug, Clone, Serialize, Deserialize, Validate, PartialEq)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
pub struct UploadCertificateRequest {
    pub certificate_chain: String,
    pub private_key: String,
}
