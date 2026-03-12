//! Agent-side Service Discovery
//! Wraps shared discovery logic from shellwego-network

// Import types from schema (single source of truth)
use shellwego_schema::network::{DiscoveryError, ServiceInstance};

// Import resolver from network (business logic)
use shellwego_network::discovery::DiscoveryResolver;

pub use shellwego_schema::network::{DiscoveryError, ServiceInstance};

pub struct ServiceDiscovery {
    inner: DiscoveryResolver,
}

impl ServiceDiscovery {
    pub fn new(domain: String) -> Self {
        Self {
            inner: DiscoveryResolver::new(domain),
        }
    }

    pub async fn discover_cp(&self) -> Result<std::net::SocketAddr, DiscoveryError> {
        let instances = self.inner.resolve_srv("control-plane").await?;
        instances.into_iter().next().ok_or(DiscoveryError::NotFound("cp".to_string()))
    }
}
