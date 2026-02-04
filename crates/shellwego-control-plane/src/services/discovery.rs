//! Control Plane Service Registry
//! Leverages shared discovery logic from shellwego-network

pub use shellwego_network::discovery::{DiscoveryRegistry, ServiceInstance, DiscoveryError};

pub struct ServiceRegistry {
    inner: DiscoveryRegistry,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            inner: DiscoveryRegistry::new(),
        }
    }

    pub async fn register_node(&self, instance: ServiceInstance) {
        self.inner.register(instance).await;
    }

    pub async fn list_instances(&self) -> Vec<ServiceInstance> {
        let instances = self.inner.instances.read().await;
        instances.values().cloned().collect()
    }
}
