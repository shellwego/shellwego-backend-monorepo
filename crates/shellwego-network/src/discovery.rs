//! Service Discovery Logic
//!
//! Provides DNS-based service discovery and instance registry functionality.
//! Types are re-exported from `shellwego-schema`.

use std::sync::Arc;
use hickory_resolver::TokioAsyncResolver;

// Re-export types from schema
pub use shellwego_schema::network::{DiscoveryError, ServiceInstance};

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

    pub async fn resolve_srv(&self, service: &str) -> Result<Vec<std::net::SocketAddr>, DiscoveryError> {
        let query = format!("{}.{}", service, self.domain_suffix);
        let lookup = self.resolver.srv_lookup(query).await
            .map_err(|e| DiscoveryError::ResolverError(e.to_string()))?;

        let mut addrs = Vec::new();
        for srv in lookup.iter() {
            // SRV target is a name, we need to resolve it to IP
            let target = srv.target().to_utf8();
            let port = srv.port();

            if let Ok(ips) = self.resolver.lookup_ip(target).await {
                for ip in ips.iter() {
                    addrs.push(std::net::SocketAddr::new(ip, port));
                }
            }
        }

        Ok(addrs)
    }
}

/// The Registry handles the "server" side (Control Plane tracking state)
pub struct DiscoveryRegistry {
    pub instances: Arc<tokio::sync::RwLock<std::collections::HashMap<String, ServiceInstance>>>,
}

impl Default for DiscoveryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscoveryRegistry {
    pub fn new() -> Self {
        Self { instances: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())) }
    }

    pub async fn register(&self, instance: ServiceInstance) {
        let mut instances = self.instances.write().await;
        instances.insert(instance.id.clone(), instance);
    }

    pub async fn unregister(&self, id: &str) {
        let mut instances = self.instances.write().await;
        instances.remove(id);
    }

    pub async fn get_instances_by_service(&self, service_name: &str) -> Vec<ServiceInstance> {
        let instances = self.instances.read().await;
        instances.values()
            .filter(|i| i.service_name == service_name)
            .cloned()
            .collect()
    }
}
