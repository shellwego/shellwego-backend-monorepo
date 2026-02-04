Fixing the discovery duplication. We’re moving the `hickory-dns` plumbing into `shellwego-network` where it belongs. 

This is a **3-part refactor**:
1.  **Part 1 (This one):** Consolidate shared logic into `shellwego-network`.
2.  **Part 2:** Gut `shellwego-agent/src/discovery.rs` and point it to the shared lib.
3.  **Part 3:** Gut `shellwego-control-plane/src/services/discovery.rs` and point it to the shared lib.

### Part 1/3: Consolidating Hickory logic in `shellwego-network`

#### Patch 1: Update `crates/shellwego-network/Cargo.toml`
Ensure `hickory-dns` features are actually there for the shared code to use.

```diff
--- a/crates/shellwego-network/Cargo.toml
+++ b/crates/shellwego-network/Cargo.toml
@@ -14,8 +14,8 @@
 serde_json = { workspace = true }
 
 # DNS for service discovery (hickory-dns - trust-dns successor)
-hickory-dns = { version = "0.24", optional = true }
-hickory-resolver = { version = "0.24", optional = true }
+hickory-dns = { version = "0.24", features = ["tokio-runtime", "system-config"] }
+hickory-resolver = { version = "0.24", features = ["tokio-runtime", "system-config"] }
 
 [features]
 default = ["quinn"]
```

#### Patch 2: Register the module in `crates/shellwego-network/src/lib.rs`

```diff
--- a/crates/shellwego-network/src/lib.rs
+++ b/crates/shellwego-network/src/lib.rs
@@ -10,6 +10,7 @@
 pub mod bridge;
 pub mod tap;
 pub mod ipam;
+pub mod discovery;
 pub mod quinn;
 
 pub use cni::CniNetwork;
```

#### Patch 3: Create the shared implementation in `crates/shellwego-network/src/discovery.rs`
This replaces the duplicate TODOs in both crates with a single source of truth.

```rust
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
```

**Next up: Part 2/3 swaps the Agent's redundant logic for this implementation.**

**Part 2/3: Gutting the Agent's redundant discovery logic.**

Now that the logic lives in `shellwego-network`, we delete the copy-pasted boilerplate in the Agent and point it to the shared implementation.

#### Patch 1: Update `crates/shellwego-agent/Cargo.toml`
Ensure the agent is pulling the `quinn` and (future) `ebpf` features from network, keeping dependencies tight.

```diff
--- a/crates/shellwego-agent/Cargo.toml
+++ b/crates/shellwego-agent/Cargo.toml
@@ -10,7 +10,7 @@
 
 [dependencies]
 shellwego-core = { path = "../shellwego-core" }
-shellwego-storage = { path = "../shellwego-storage" }
-shellwego-network = { path = "../shellwego-network" }
+shellwego-storage = { path = "../shellwego-storage" }
+shellwego-network = { path = "../shellwego-network", features = ["quinn"] }
```

#### Patch 2: Refactor `crates/shellwego-agent/src/discovery.rs`
We drop the ~100 lines of `TODO` ghosts and replace them with a thin wrapper around the network crate. This is the DRYest move you can make.

```diff
--- a/crates/shellwego-agent/src/discovery.rs
+++ b/crates/shellwego-agent/src/discovery.rs
@@ -1,108 +1,28 @@
-//! DNS-based service discovery using hickory-dns (trust-dns successor)
-//!
-//! Provides service discovery via DNS SRV records for agent-to-agent
-//! and agent-to-control-plane communication without external dependencies.
+//! Agent-side Service Discovery
+//! Wraps shared discovery logic from shellwego-network
 
-use std::collections::HashMap;
-use std::net::SocketAddr;
-use std::sync::Arc;
-use std::time::Duration;
-use tokio::sync::RwLock;
-use thiserror::Error;
-
-#[derive(Debug, Error)]
-pub enum DnsDiscoveryError {
-    #[error("DNS resolver error: {source}")]
-    ResolverError {
-        source: hickory_resolver::error::ResolveError,
-    },
-
-    #[error("Service not found: {service_name}")]
-    ServiceNotFound {
-        service_name: String,
-    },
-
-    #[error("No healthy instances for service: {service_name}")]
-    NoHealthyInstances {
-        service_name: String,
-    },
-
-    #[error("Configuration error: {message}")]
-    ConfigError { message: String },
-}
+pub use shellwego_network::discovery::{DiscoveryResolver, ServiceInstance, DiscoveryError};
 
 pub struct ServiceDiscovery {
-    // TODO: Add resolver Arc<hickory_resolver::Resolver>
-    resolver: Arc<hickory_resolver::Resolver>,
-
-    // TODO: Add cache RwLock<HashMap<String, CachedServices>>
-    cache: Arc<RwLock<HashMap<String, CachedServices>>>,
-
-    // TODO: Add domain_suffix String
-    domain_suffix: String,
-
-    // TODO: Add refresh_interval Duration
-    refresh_interval: Duration,
+    inner: DiscoveryResolver,
 }
 
-struct CachedServices {
-    // TODO: Add instances Vec<ServiceInstance>
-    instances: Vec<ServiceInstance>,
-
-    // TODO: Add cached_at chrono::DateTime<chrono::Utc>
-    cached_at: chrono::DateTime<chrono::Utc>,
-}
-
-#[derive(Debug, Clone)]
-pub struct ServiceInstance {
-    // TODO: Add service_name String
-    pub service_name: String,
-
-    // TODO: Add instance_id String
-    pub instance_id: String,
-
-    // TODO: Add host String
-    pub host: String,
-
-    // TODO: Add port u16
-    pub port: u16,
-
-    // TODO: Add priority u16
-    pub priority: u16,
-
-    // TODO: Add weight u16
-    pub weight: u16,
-
-    // TODO: Add metadata HashMap<String, String>
-    pub metadata: HashMap<String, String>,
-}
-
 impl ServiceDiscovery {
-    // TODO: Implement new() constructor
-    // - Initialize hickory_resolver with system config
-    // - Set default domain suffix
-    // - Set default refresh interval
-
-    // TODO: Implement new_with_config() for custom DNS servers
-    // - Accept custom nameserver addresses
-    // - Configure resolver with custom config
-
-    // TODO: Implement discover() method
-    // - Check cache first
-    // - Perform DNS SRV lookup if cache miss or expired
-    // - Return healthy instances sorted by priority/weight
-
-    // TODO: Implement discover_all() for all instances
-    // - Include unhealthy/degraded instances
-
-    // TODO: Implement register() for self-registration
-    // - Create DNS records for local service
-    // - Support dynamic DNS updates if available
-
-    // TODO: Implement deregister() for cleanup
-    // - Remove DNS records for service
-
-    // TODO: Implement watch() for streaming updates
-    // - Return tokio::sync::mpsc::Receiver for changes
-    // - Notify on service add/remove/update
+    pub fn new(domain: String) -> Self {
+        Self {
+            inner: DiscoveryResolver::new(domain),
+        }
+    }
 
-    // TODO: Implement resolve_srv() direct SRV lookup
-    // - Query SRV records for service name
-    // - Parse priority/weight/port from records
-
-    // TODO: Implement resolve_txt() for metadata
-    // - Query TXT records for service metadata
-
-    // TODO: Implement get_healthy_instances() method
-    // - Filter by health status
-    // - Apply load balancing
-
-    // TODO: Implement get_zone_instances() for zone-aware
-    // - Filter by availability zone
-    // - Support zone-aware routing
+    pub async fn discover_cp(&self) -> Result<std::net::SocketAddr, DiscoveryError> {
+        let instances = self.inner.resolve_srv("control-plane").await?;
+        instances.into_iter().next().ok_or(DiscoveryError::NotFound("cp".to_string()))
+    }
 }
```

**Part 3/3 coming up: Cleaning the Control Plane's registry logic.**


**Part 3/3: Purging the Control Plane's duplicate registry.**

Final cleanup. We’re stripping the ghost code from the CP's service layer and plugging in the `DiscoveryRegistry` from `shellwego-network`.

#### Patch 1: Update `crates/shellwego-control-plane/Cargo.toml`
Connect the CP to the shared network crate.

```diff
--- a/crates/shellwego-control-plane/Cargo.toml
+++ b/crates/shellwego-control-plane/Cargo.toml
@@ -9,6 +9,7 @@
 
 [dependencies]
 shellwego-core = { path = "../shellwego-core", features = ["openapi"] }
+shellwego-network = { path = "../shellwego-network" }
 
 # Async runtime
```

#### Patch 2: Refactor `crates/shellwego-control-plane/src/services/discovery.rs`
Deletes ~120 lines of redundant scaffolding. The CP now acts as the authoritative source for the registry using the shared primitives.

```diff
--- a/crates/shellwego-control-plane/src/services/discovery.rs
+++ b/crates/shellwego-control-plane/src/services/discovery.rs
@@ -1,118 +1,24 @@
-//! DNS-based service registry using hickory-dns (trust-dns successor)
-//!
-//! Provides service discovery via DNS SRV records for control plane.
-//! Supports in-memory registry with DNS record publishing.
+//! Control Plane Service Registry
+//! Leverages shared discovery logic from shellwego-network
 
-use std::collections::HashMap;
-use std::net::SocketAddr;
-use std::sync::Arc;
-use std::time::Duration;
-use tokio::sync::RwLock;
-use thiserror::Error;
-
-#[derive(Debug, Error)]
-pub enum RegistryError {
-    #[error("Instance already exists: {0}")]
-    AlreadyExists(String),
-
-    #[error("Instance not found: {0}")]
-    NotFound(String),
-
-    #[error("DNS publish error: {source}")]
-    DnsPublishError {
-        source: hickory_resolver::error::ResolveError,
-    },
-
-    #[error("Configuration error: {message}")]
-    ConfigError { message: String },
-}
+pub use shellwego_network::discovery::{DiscoveryRegistry, ServiceInstance, DiscoveryError};
 
 pub struct ServiceRegistry {
-    // TODO: Add instances RwLock<HashMap<String, HashMap<String, ServiceInstance>>>
-    instances: Arc<RwLock<HashMap<String, HashMap<String, ServiceInstance>>>>,
-
-    // TODO: Add dns_publisher Option<DnsPublisher>
-    dns_publisher: Option<DnsPublisher>,
-
-    // TODO: Add domain_suffix String
-    domain_suffix: String,
-
-    // TODO: Add cleanup_interval Duration
-    cleanup_interval: Duration,
-}
-
-struct DnsPublisher {
-    // TODO: Add resolver hickory_resolver::Resolver
-    resolver: hickory_resolver::Resolver,
-
-    // TODO: Add zone String
-    zone: String,
-}
-
-#[derive(Debug, Clone)]
-pub struct ServiceInstance {
-    // TODO: Add id String
-    pub id: String,
-
-    // TODO: Add service_name String
-    pub service_name: String,
-
-    // TODO: Add app_id uuid::Uuid
-    pub app_id: uuid::Uuid,
-
-    // TODO: Add node_id String
-    pub node_id: String,
-
-    // TODO: Add address SocketAddr
-    pub address: SocketAddr,
-
-    // TODO: Add metadata HashMap<String, String>
-    pub metadata: HashMap<String, String>,
-
-    // TODO: Add registered_at chrono::DateTime<chrono::Utc>
-    pub registered_at: chrono::DateTime<chrono::Utc>,
-
-    // TODO: Add last_heartbeat chrono::DateTime<chrono::Utc>
-    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
-
-    // TODO: Add healthy bool
-    pub healthy: bool,
+    inner: DiscoveryRegistry,
 }
 
 impl ServiceRegistry {
-    // TODO: Implement new() constructor
-    // - Initialize in-memory registry
-    // - Set default domain suffix
-
-    // TODO: Implement new_with_dns() for DNS publishing
-    // - Initialize DnsPublisher
-    // - Configure zone for DNS records
+    pub fn new() -> Self {
+        Self {
+            inner: DiscoveryRegistry::new(),
+        }
+    }
 
-    // TODO: Implement register() method
-    // - Validate instance
-    // - Store in memory
-    // - Publish DNS SRV record if DNS enabled
-    // - Return error if already exists
+    pub async fn register_node(&self, instance: ServiceInstance) {
+        self.inner.register(instance).await;
+    }
 
-    // TODO: Implement deregister() method
-    // - Remove from memory
-    // - Remove DNS SRV record if DNS enabled
-    // - Return error if not found
-
-    // TODO: Implement get_healthy() method
-    // - Filter by healthy flag
-    // - Filter by expiry timestamp
-    // - Return sorted by priority
-
-    // TODO: Implement get_all() for all instances
-    // - Include unhealthy instances
-
-    // TODO: Implement update_health() method
-    // - Update healthy flag
-    // - Update last_heartbeat timestamp
-
-    // TODO: Implement update_heartbeat() method
-    // - Refresh last_heartbeat for instance
-    // - Mark healthy if was unhealthy
-
-    // TODO: Implement cleanup() method
-    // - Remove instances with expired heartbeats
-    // - Return count of removed instances
-
-    // TODO: Implement publish_srv_record() private method
-    // - Create SRV record for instance
-    // - Update DNS zone
-
-    // TODO: Implement remove_srv_record() private method
-    // - Remove SRV record from DNS zone
-
-    // TODO: Implement list_services() method
-    // - Return all registered service names
+    pub async fn list_instances(&self) -> Vec<ServiceInstance> {
+        let instances = self.inner.instances.read().await;
+        instances.values().cloned().collect()
+    }
 }
```

**Refactor complete.** 
*   **Result:** Deleted ~250 lines of duplicate TODOs. 
*   **Why it matters:** When you eventually implement the SRV record logic in `shellwego-network/src/discovery.rs`, it will "just work" for both the Agent and the CP simultaneously. This is how you build a sovereign cloud without drowning in your own boilerplate.