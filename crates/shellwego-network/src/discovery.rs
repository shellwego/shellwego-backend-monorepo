//! Shared Service Discovery Logic
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use hickory_resolver::TokioAsyncResolver;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("DNS resolver error: {0}")]
    ResolverError(String),
    #[error("Service instance not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    pub id: String,
    pub service_name: String,
    pub address: SocketAddr,
    pub metadata: HashMap<String, String>,
    pub healthy: bool,
}

/// The Resolver handles the "client" side (Agent finding CP, or Agent finding Peers)
pub struct DiscoveryResolver {
    resolver: TokioAsyncResolver,
    domain_suffix: String,
}

impl DiscoveryResolver {
    pub fn new(domain_suffix: String) -> Self {
        let resolver = TokioAsyncResolver::tokio_from_system_conf().unwrap();
        Self { resolver, domain_suffix }
    }

    pub async fn resolve_srv(&self, service: &str) -> Result<Vec<SocketAddr>, DiscoveryError> {
        let query = format!("{}.{}", service, self.domain_suffix);
        let lookup = self.resolver.srv_lookup(query).await
            .map_err(|e| DiscoveryError::ResolverError(e.to_string()))?;

        // TODO: Map SRV records to SocketAddrs using IP lookups for targets
        Ok(vec![])
    }
}

/// The Registry handles the "server" side (Control Plane tracking state)
pub struct DiscoveryRegistry {
    pub instances: Arc<tokio::sync::RwLock<HashMap<String, ServiceInstance>>>,
}

impl DiscoveryRegistry {
    pub fn new() -> Self {
        Self { instances: Arc::new(tokio::sync::RwLock::new(HashMap::new())) }
    }

    pub async fn register(&self, instance: ServiceInstance) {
        let mut instances = self.instances.write().await;
        instances.insert(instance.id.clone(), instance);
    }
}
