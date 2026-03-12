//! API request types for app endpoints
//!
//! Query parameters and request bodies for app-related API endpoints.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Query params for list apps endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct ListAppsQuery {
    /// Filter by organization ID
    #[serde(default)]
    pub organization_id: Option<Uuid>,
    /// Filter by status
    #[serde(default)]
    pub status: Option<String>,
    /// Maximum number of results
    #[serde(default)]
    pub limit: Option<u32>,
    /// Pagination cursor
    #[serde(default)]
    pub cursor: Option<String>,
}

impl Default for ListAppsQuery {
    fn default() -> Self {
        Self {
            organization_id: None,
            status: None,
            limit: Some(50),
            cursor: None,
        }
    }
}

/// Scale request body
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct ScaleRequest {
    /// Desired number of replicas
    pub replicas: u32,
}

impl ScaleRequest {
    /// Create a new scale request
    pub fn new(replicas: u32) -> Self {
        Self { replicas }
    }
}

/// Deploy request body
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct DeployRequest {
    /// Image to deploy
    pub image: String,
    /// Optional git reference
    #[serde(default)]
    pub git_ref: Option<String>,
    /// Deploy strategy
    #[serde(default)]
    pub strategy: DeployStrategy,
}

impl Default for DeployRequest {
    fn default() -> Self {
        Self {
            image: String::new(),
            git_ref: None,
            strategy: DeployStrategy::Rolling,
        }
    }
}

/// Deploy strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum DeployStrategy {
    /// Rolling update (gradual replacement)
    Rolling,
    /// Recreate (stop all, then start new)
    Recreate,
    /// Blue-green deployment
    BlueGreen,
}

impl Default for DeployStrategy {
    fn default() -> Self {
        Self::Rolling
    }
}

/// Query params for list nodes endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct ListNodesQuery {
    /// Filter by status
    #[serde(default)]
    pub status: Option<String>,
    /// Filter by region
    #[serde(default)]
    pub region: Option<String>,
    /// Filter by zone
    #[serde(default)]
    pub zone: Option<String>,
    /// Maximum number of results
    #[serde(default)]
    pub limit: Option<u32>,
}

impl Default for ListNodesQuery {
    fn default() -> Self {
        Self {
            status: None,
            region: None,
            zone: None,
            limit: Some(50),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_apps_query_default() {
        let query = ListAppsQuery::default();
        assert!(query.organization_id.is_none());
        assert!(query.status.is_none());
        assert_eq!(query.limit, Some(50));
    }

    #[test]
    fn test_scale_request_new() {
        let req = ScaleRequest::new(3);
        assert_eq!(req.replicas, 3);
    }

    #[test]
    fn test_deploy_strategy_default() {
        assert_eq!(DeployStrategy::default(), DeployStrategy::Rolling);
    }

    #[test]
    fn test_list_nodes_query_default() {
        let query = ListNodesQuery::default();
        assert!(query.status.is_none());
        assert_eq!(query.limit, Some(50));
    }
}
