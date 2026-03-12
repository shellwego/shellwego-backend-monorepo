//! Service discovery types
//!
//! Types for service discovery and instance registration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use thiserror::Error;

/// Service discovery errors
#[derive(Debug, Clone, Error, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum DiscoveryError {
    /// DNS resolver error
    #[error("DNS resolver error: {0}")]
    ResolverError(String),

    /// Service instance not found
    #[error("Service instance not found: {0}")]
    NotFound(String),
}

/// A service instance in the discovery registry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct ServiceInstance {
    /// Unique identifier for this instance
    pub id: String,
    /// Name of the service this instance belongs to
    pub service_name: String,
    /// Network address of the instance
    pub address: SocketAddr,
    /// Additional metadata about the instance
    pub metadata: HashMap<String, String>,
    /// Whether the instance is currently healthy
    pub healthy: bool,
}

impl ServiceInstance {
    /// Create a new service instance
    pub fn new(id: String, service_name: String, address: SocketAddr) -> Self {
        Self {
            id,
            service_name,
            address,
            metadata: HashMap::new(),
            healthy: true,
        }
    }

    /// Add metadata to the instance
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Set the health status
    pub fn with_health(mut self, healthy: bool) -> Self {
        self.healthy = healthy;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_service_instance_new() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let instance = ServiceInstance::new(
            "instance-1".to_string(),
            "control-plane".to_string(),
            addr,
        );

        assert_eq!(instance.id, "instance-1");
        assert_eq!(instance.service_name, "control-plane");
        assert_eq!(instance.address, addr);
        assert!(instance.healthy);
        assert!(instance.metadata.is_empty());
    }

    #[test]
    fn test_service_instance_builder() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 4433);
        let instance = ServiceInstance::new(
            "agent-1".to_string(),
            "agent".to_string(),
            addr,
        )
        .with_metadata("region".to_string(), "us-east-1".to_string())
        .with_health(false);

        assert_eq!(instance.metadata.get("region"), Some(&"us-east-1".to_string()));
        assert!(!instance.healthy);
    }

    #[test]
    fn test_discovery_error_display() {
        let err = DiscoveryError::ResolverError("timeout".to_string());
        assert_eq!(format!("{}", err), "DNS resolver error: timeout");

        let err = DiscoveryError::NotFound("control-plane".to_string());
        assert_eq!(format!("{}", err), "Service instance not found: control-plane");
    }
}
