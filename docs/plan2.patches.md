
Kimi: **`crates/shellwego-registry/src/lib.rs`**
```rust
//! Container image registry cache and pull operations
//! 
//! Integrates with skopeo/umoci for OCI image handling.

use thiserror::Error;

pub mod cache;
pub mod pull;

#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("Image not found: {0}")]
    NotFound(String),
    
    #[error("Pull failed: {0}")]
    PullFailed(String),
    
    #[error("Cache corrupted: {0}")]
    CacheCorrupted(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Registry backend trait for pluggable image sources
#[async_trait::async_trait]
pub trait RegistryBackend: Send + Sync {
    // TODO: Authenticate to remote registry
    // fn authenticate(&self, creds: &RegistryAuth) -> Result<AuthToken, RegistryError>;
    
    // TODO: Check if image exists in remote
    // async fn exists(&self, image_ref: &str) -> Result<bool, RegistryError>;
    
    // TODO: Pull image manifest
    // async fn pull_manifest(&self, image_ref: &str) -> Result<Manifest, RegistryError>;
    
    // TODO: Pull layer blob
    // async fn pull_layer(&self, digest: &str) -> Result<Bytes, RegistryError>;
    
    // TODO: Get image config
    // async fn get_config(&self, image_ref: &str) -> Result<ImageConfig, RegistryError>;
}

/// OCI image manifest structure
#[derive(Debug, Clone)]
pub struct Manifest {
    // TODO: Add schemaVersion, mediaType, layers, config
}

/// Image configuration (env, cmd, entrypoint, etc)
#[derive(Debug, Clone)]
pub struct ImageConfig {
    // TODO: Add Env, Cmd, Entrypoint, WorkingDir, User, ExposedPorts
}

/// Registry authentication credentials
#[derive(Debug, Clone)]
pub struct RegistryAuth {
    // TODO: Add username, password, token, registry_url
}
```

**`crates/shellwego-registry/src/cache.rs`**
```rust
//! Image layer caching with ZFS backend
//! 
//! Converts OCI layers to ZFS datasets for instant cloning.

use std::path::PathBuf;

use crate::RegistryError;

/// Layer cache manager
pub struct LayerCache {
    // TODO: Add base_dataset, zfs_cli, manifest_index
}

impl LayerCache {
    /// Initialize cache on ZFS pool
    pub async fn new(pool: &str) -> Result<Self, RegistryError> {
        // TODO: Verify pool exists
        // TODO: Create shellwego/registry dataset if missing
        // TODO: Load manifest index from disk
        unimplemented!("LayerCache::new")
    }

    /// Check if image is cached locally
    pub async fn is_cached(&self, image_ref: &str) -> bool {
        // TODO: Lookup in manifest index
        // TODO: Verify all layer datasets exist
        unimplemented!("LayerCache::is_cached")
    }

    /// Get cached image rootfs path
    pub async fn get_rootfs(&self, image_ref: &str) -> Result<PathBuf, RegistryError> {
        // TODO: Return mountpoint of image dataset
        unimplemented!("LayerCache::get_rootfs")
    }

    /// Import OCI image into ZFS cache
    pub async fn import_image(
        &self,
        image_ref: &str,
        manifest: &crate::Manifest,
        layers: &[bytes::Bytes],
    ) -> Result<PathBuf, RegistryError> {
        // TODO: Create dataset for image
        // TODO: Extract layers in order (bottom to top)
        // TODO: Snapshot at @base tag
        // TODO: Update manifest index
        unimplemented!("LayerCache::import_image")
    }

    /// Garbage collect unused layers
    pub async fn gc(&self, keep_recent: usize) -> Result<u64, RegistryError> {
        // TODO: Find unreferenced layer datasets
        // TODO: Destroy oldest first, keeping keep_recent
        // TODO: Return bytes freed
        unimplemented!("LayerCache::gc")
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        // TODO: Return total size, hit rate, layer count
        unimplemented!("LayerCache::stats")
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    // TODO: Add total_bytes, layer_count, image_count, hit_rate
}
```

**`crates/shellwego-registry/src/pull.rs`**
```rust
//! Image pulling from remote registries
//! 
//! Supports Docker Hub, GHCR, ECR, GCR, and private registries.

use crate::{RegistryBackend, RegistryAuth, RegistryError, Manifest, ImageConfig};

/// Image puller with progress tracking
pub struct ImagePuller {
    // TODO: Add client (reqwest), auth_store, cache
}

impl ImagePuller {
    /// Create new puller instance
    pub fn new() -> Self {
        // TODO: Initialize HTTP client with timeouts
        // TODO: Setup authentication store
        unimplemented!("ImagePuller::new")
    }

    /// Pull image to local cache
    pub async fn pull(
        &self,
        image_ref: &str,
        auth: Option<&RegistryAuth>,
    ) -> Result<PulledImage, RegistryError> {
        // TODO: Parse image reference (registry/name:tag)
        // TODO: Authenticate if needed
        // TODO: Fetch manifest
        // TODO: Check cache for existing layers
        // TODO: Download missing layers with progress
        // TODO: Import to cache
        unimplemented!("ImagePuller::pull")
    }

    /// Pull with streaming progress
    pub async fn pull_with_progress(
        &self,
        image_ref: &str,
        auth: Option<&RegistryAuth>,
        progress: &mut dyn PullProgress,
    ) -> Result<PulledImage, RegistryError> {
        // TODO: Same as pull but call progress callbacks
        unimplemented!("ImagePuller::pull_with_progress")
    }

    /// Verify image signature (cosign)
    pub async fn verify_signature(
        &self,
        image_ref: &str,
        key: &str,
    ) -> Result<bool, RegistryError> {
        // TODO: Fetch signature from registry
        // TODO: Verify with cosign verification
        unimplemented!("ImagePuller::verify_signature")
    }
}

/// Pulled image result
#[derive(Debug, Clone)]
pub struct PulledImage {
    // TODO: Add image_ref, manifest, config, rootfs_path, size_bytes
}

/// Pull progress callback trait
pub trait PullProgress {
    // TODO: fn on_layer_start(&mut self, digest: &str, size: u64);
    // TODO: fn on_layer_progress(&mut self, digest: &str, downloaded: u64);
    // TODO: fn on_layer_complete(&mut self, digest: &str);
    // TODO: fn on_complete(&mut self);
}
```

**`crates/shellwego-registry/Cargo.toml`**
```toml
[package]
name = "shellwego-registry"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
description = "Container image registry cache and OCI pull operations"

[dependencies]
# Core async
tokio = { workspace = true, features = ["process", "fs"] }

# HTTP client for registry
reqwest = { workspace = true, features = ["json", "stream"] }

# OCI/Docker types
oci-distribution = "0.10"

# Async traits
async-trait = "0.1"

# Serialization
serde = { workspace = true }
serde_json = { workspace = true }

# Image format handling
tar = "0.4"
flate2 = "1.0"

# Crypto for verification
sha2 = "0.10"
hex = "0.4"

# Errors
thiserror = { workspace = true }
anyhow = { workspace = true }

# Tracing
tracing = { workspace = true }

[dev-dependencies]
tempfile = "3.8"
tokio-test = "0.4"
```

**`crates/shellwego-network/src/ebpf/mod.rs`**
```rust
//! eBPF/XDP programs for high-performance networking
//! 
//! Uses aya-rs for safe eBPF loading and management.

use thiserror::Error;

pub mod firewall;
pub mod qos;

#[derive(Error, Debug)]
pub enum EbpfError {
    #[error("eBPF load failed: {0}")]
    LoadFailed(String),
    
    #[error("Program not attached: {0}")]
    NotAttached(String),
    
    #[error("Map operation failed: {0}")]
    MapError(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// eBPF program manager
pub struct EbpfManager {
    // TODO: Add loaded_programs, bpf_loader, map_fds
}

impl EbpfManager {
    /// Initialize eBPF subsystem
    pub async fn new() -> Result<Self, EbpfError> {
        // TODO: Check kernel version (5.10+ required)
        // TODO: Verify BPF filesystem mounted
        // TODO: Initialize aya::BpfLoader
        unimplemented!("EbpfManager::new")
    }

    /// Load and attach XDP program to interface
    pub async fn attach_xdp(
        &self,
        iface: &str,
        program: XdpProgram,
    ) -> Result<ProgramHandle, EbpfError> {
        // TODO: Load eBPF ELF
        // TODO: Set XDP mode (SKB_MODE or DRV_MODE)
        // TODO: Attach to interface
        unimplemented!("EbpfManager::attach_xdp")
    }

    /// Load and attach TC (traffic control) program
    pub async fn attach_tc(
        &self,
        iface: &str,
        direction: TcDirection,
        program: TcProgram,
    ) -> Result<ProgramHandle, EbpfError> {
        // TODO: Load clsact qdisc if needed
        // TODO: Attach filter program
        unimplemented!("EbpfManager::attach_tc")
    }

    /// Load cgroup eBPF program for socket filtering
    pub async fn attach_cgroup(
        &self,
        cgroup_path: &std::path::Path,
        program: CgroupProgram,
    ) -> Result<ProgramHandle, EbpfError> {
        // TODO: Open cgroup FD
        // TODO: Attach SOCK_OPS or SOCK_ADDR program
        unimplemented!("EbpfManager::attach_cgroup")
    }

    /// Detach program by handle
    pub async fn detach(&self, handle: ProgramHandle) -> Result<(), EbpfError> {
        // TODO: Lookup program by handle
        // TODO: Call aya detach
        unimplemented!("EbpfManager::detach")
    }

    /// Update eBPF map entry
    pub async fn map_update<K, V>(
        &self,
        map_name: &str,
        key: &K,
        value: &V,
    ) -> Result<(), EbpfError> {
        // TODO: Lookup map FD
        // TODO: Insert or update key-value
        unimplemented!("EbpfManager::map_update")
    }

    /// Read eBPF map entry
    pub async fn map_lookup<K, V>(
        &self,
        map_name: &str,
        key: &K,
    ) -> Result<Option<V>, EbpfError> {
        // TODO: Lookup map FD
        // TODO: Lookup key, return value if exists
        unimplemented!("EbpfManager::map_lookup")
    }
}

/// XDP program types
pub enum XdpProgram {
    // TODO: Firewall, DdosProtection, LoadBalancer
}

/// TC program types
pub enum TcProgram {
    // TODO: BandwidthLimit, LatencyInjection
}

/// Cgroup program types
pub enum CgroupProgram {
    // TODO: SocketFilter, ConnectHook
}

/// TC attachment direction
pub enum TcDirection {
    Ingress,
    Egress,
}

/// Opaque handle to loaded program
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProgramHandle(u64);
```

**`crates/shellwego-network/src/ebpf/firewall.rs`**
```rust
//! XDP-based firewall for DDoS protection and filtering

use crate::ebpf::{EbpfManager, EbpfError, XdpProgram, ProgramHandle};

/// XDP firewall controller
pub struct XdpFirewall {
    // TODO: Add ebpf_manager, attached_handles, rules_map
}

impl XdpFirewall {
    /// Create new firewall instance
    pub fn new(manager: &EbpfManager) -> Self {
        // TODO: Store reference to manager
        unimplemented!("XdpFirewall::new")
    }

    /// Attach firewall to network interface
    pub async fn attach(&mut self, iface: &str) -> Result<(), EbpfError> {
        // TODO: Load xdp_firewall.o
        // TODO: Attach XDP program
        // TODO: Store handle for later detach
        unimplemented!("XdpFirewall::attach")
    }

    /// Add IP to blocklist
    pub async fn block_ip(&self, ip: std::net::IpAddr) -> Result<(), EbpfError> {
        // TODO: Convert IP to u32/u128
        // TODO: Insert into blocked_ips map
        unimplemented!("XdpFirewall::block_ip")
    }

    /// Remove IP from blocklist
    pub async fn unblock_ip(&self, ip: std::net::IpAddr) -> Result<(), EbpfError> {
        // TODO: Remove from blocked_ips map
        unimplemented!("XdpFirewall::unblock_ip")
    }

    /// Add rate limit rule
    pub async fn add_rate_limit(
        &self,
        ip: std::net::IpAddr,
        packets_per_sec: u32,
    ) -> Result<(), EbpfError> {
        // TODO: Insert into rate_limits map
        unimplemented!("XdpFirewall::add_rate_limit")
    }

    /// Get dropped packet statistics
    pub async fn stats(&self) -> FirewallStats {
        // TODO: Read stats map from eBPF
        unimplemented!("XdpFirewall::stats")
    }

    /// Detach firewall
    pub async fn detach(&mut self) -> Result<(), EbpfError> {
        // TODO: Detach all programs
        unimplemented!("XdpFirewall::detach")
    }
}

/// Firewall statistics
#[derive(Debug, Clone, Default)]
pub struct FirewallStats {
    // TODO: Add packets_allowed, packets_dropped, packets_ratelimited
    // TODO: Add bytes_allowed, bytes_dropped
    // TODO: Add top_blocked_ips
}
```

**`crates/shellwego-network/src/ebpf/qos.rs`**
```rust
//! eBPF-based Quality of Service and traffic shaping
//! 
//! Replaces tc (traffic control) with faster eBPF implementations.

use crate::ebpf::{EbpfManager, EbpfError, TcProgram, TcDirection, ProgramHandle};

/// eBPF QoS controller
pub struct EbpfQos {
    // TODO: Add manager, active_shapers
}

impl EbpfQos {
    /// Create new QoS controller
    pub fn new(manager: &EbpfManager) -> Self {
        // TODO: Store manager reference
        unimplemented!("EbpfQos::new")
    }

    /// Apply bandwidth limit to interface
    pub async fn limit_bandwidth(
        &self,
        iface: &str,
        direction: TcDirection,
        bits_per_sec: u64,
    ) -> Result<ShaperHandle, EbpfError> {
        // TODO: Load tc_bandwidth.o
        // TODO: Set rate limit in map
        // TODO: Attach to interface
        unimplemented!("EbpfQos::limit_bandwidth")
    }

    /// Apply latency/packet loss simulation (testing)
    pub async fn add_impairment(
        &self,
        iface: &str,
        direction: TcDirection,
        latency_ms: u32,
        jitter_ms: u32,
        loss_percent: f32,
    ) -> Result<ShaperHandle, EbpfError> {
        // TODO: Load tc_impairment.o
        // TODO: Configure parameters
        unimplemented!("EbpfQos::add_impairment")
    }

    /// Prioritize traffic by DSCP/TOS
    pub async fn set_priority(
        &self,
        iface: &str,
        dscp: u8,
        priority: TrafficPriority,
    ) -> Result<(), EbpfError> {
        // TODO: Update priority map
        unimplemented!("EbpfQos::set_priority")
    }

    /// Remove shaper by handle
    pub async fn remove_shaper(&self, handle: ShaperHandle) -> Result<(), EbpfError> {
        // TODO: Detach TC program
        // TODO: Cleanup maps
        unimplemented!("EbpfQos::remove_shaper")
    }

    /// Get current shaping statistics
    pub async fn shaper_stats(&self, handle: ShaperHandle) -> ShaperStats {
        // TODO: Read per-shaper statistics
        unimplemented!("EbpfQos::shaper_stats")
    }
}

/// Handle to active traffic shaper
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShaperHandle(u64);

/// Traffic priority levels
pub enum TrafficPriority {
    BestEffort,
    Bronze,
    Silver,
    Gold,
    Platinum,
}

/// Shaper statistics
#[derive(Debug, Clone, Default)]
pub struct ShaperStats {
    // TODO: Add bytes_processed, bytes_dropped, bytes_delayed
    // TODO: Add current_rate, burst_allowance
}
```

**`crates/shellwego-control-plane/src/kms/mod.rs`**
```rust
//! Key Management Service integration
//! 
//! Supports HashiCorp Vault, cloud KMS, and file-based keys.

use thiserror::Error;

pub mod providers;

#[derive(Error, Debug)]
pub enum KmsError {
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    
    #[error("Provider error: {0}")]
    ProviderError(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// KMS provider trait
#[async_trait::async_trait]
pub trait KmsProvider: Send + Sync {
    // TODO: async fn encrypt(&self, plaintext: &[u8], key_id: &str) -> Result<Vec<u8>, KmsError>;
    // TODO: async fn decrypt(&self, ciphertext: &[u8], key_id: &str) -> Result<Vec<u8>, KmsError>;
    // TODO: async fn generate_data_key(&self, key_id: &str) -> Result<DataKey, KmsError>;
    // TODO: async fn rotate_key(&self, key_id: &str) -> Result<(), KmsError>;
    // TODO: async fn health_check(&self) -> Result<(), KmsError>;
}

/// Data encryption key (DEK) with encrypted key encryption key (KEK)
#[derive(Debug, Clone)]
pub struct DataKey {
    // TODO: Add plaintext (base64), ciphertext, key_id, algorithm
}

/// Master key reference
#[derive(Debug, Clone)]
pub struct MasterKey {
    // TODO: Add key_id, provider, created_at, status
}

/// KMS client that routes to appropriate provider
pub struct KmsClient {
    // TODO: Add providers HashMap, default_provider
}

impl KmsClient {
    /// Create client from configuration
    pub async fn from_config(config: &KmsConfig) -> Result<Self, KmsError> {
        // TODO: Initialize providers based on config
        // TODO: Health check all providers
        unimplemented!("KmsClient::from_config")
    }

    /// Encrypt data using envelope encryption
    pub async fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedBlob, KmsError> {
        // TODO: Generate DEK locally
        // TODO: Encrypt DEK with master key
        // TODO: Encrypt data with DEK
        // TODO: Return blob with ciphertext + encrypted DEK
        unimplemented!("KmsClient::encrypt")
    }

    /// Decrypt envelope-encrypted data
    pub async fn decrypt(&self, blob: &EncryptedBlob) -> Result<Vec<u8>, KmsError> {
        // TODO: Decrypt DEK with master key
        // TODO: Decrypt data with DEK
        // TODO: Return plaintext
        unimplemented!("KmsClient::decrypt")
    }

    /// Rotate master key
    pub async fn rotate_master_key(&self) -> Result<(), KmsError> {
        // TODO: Generate new master key
        // TODO: Re-encrypt all DEKs
        // TODO: Mark old key as deprecated
        unimplemented!("KmsClient::rotate_master_key")
    }
}

/// KMS configuration
#[derive(Debug, Clone)]
pub enum KmsConfig {
    // TODO: Vault { address, token, mount_point }
    // TODO: AwsKms { region, key_id, role_arn }
    // TODO: GcpKms { project, location, key_ring, key_name }
    // TODO: AzureKeyVault { vault_url, key_name, tenant_id }
    // TODO: File { path, passphrase }
}

/// Encrypted data blob with metadata
#[derive(Debug, Clone)]
pub struct EncryptedBlob {
    // TODO: Add ciphertext, encrypted_dek, key_id, algorithm, iv, auth_tag
}
```

**`crates/shellwego-control-plane/src/kms/providers.rs`**
```rust
//! KMS provider implementations

use async_trait::async_trait;

use crate::kms::{KmsProvider, KmsError, DataKey, MasterKey};

/// HashiCorp Vault provider
pub struct VaultProvider {
    // TODO: Add client, mount_point, token
}

#[async_trait]
impl KmsProvider for VaultProvider {
    // TODO: impl encrypt via Vault transit API
    // TODO: impl decrypt via Vault transit API
    // TODO: impl generate_data_key
    // TODO: impl rotate_key
    // TODO: impl health_check
}

/// AWS KMS provider
pub struct AwsKmsProvider {
    // TODO: Add aws_sdk_kms client, key_id
}

#[async_trait]
impl KmsProvider for AwsKmsProvider {
    // TODO: impl using aws_sdk_kms
}

/// GCP Cloud KMS provider
pub struct GcpKmsProvider {
    // TODO: Add google_cloud_kms client
}

#[async_trait]
impl KmsProvider for GcpKmsProvider {
    // TODO: impl using google-cloud-kms
}

/// Azure Key Vault provider
pub struct AzureKeyVaultProvider {
    // TODO: Add azure_security_keyvault client
}

#[async_trait]
impl KmsProvider for AzureKeyVaultProvider {
    // TODO: impl using azure_security_keyvault
}

/// File-based provider (development only)
pub struct FileProvider {
    // TODO: Add key_file_path, passphrase
}

#[async_trait]
impl KmsProvider for FileProvider {
    // TODO: impl using age or PGP encryption
    // TODO: WARN: Not for production use
}
```
User: Part 1 done?
Kimi: Yes, Part 1/6 is complete. Sending Part 2/6 now.

---

**`crates/shellwego-observability/src/lib.rs`**
```rust
//! Observability stack: metrics, logs, and distributed tracing
//! 
//! Prometheus, Loki, and OpenTelemetry integration.

use thiserror::Error;

pub mod metrics;
pub mod logs;
pub mod tracing;

#[derive(Error, Debug)]
pub enum ObservabilityError {
    #[error("Metrics error: {0}")]
    MetricsError(String),
    
    #[error("Log error: {0}")]
    LogError(String),
    
    #[error("Tracing error: {0}")]
    TracingError(String),
    
    #[error("Export failed: {0}")]
    ExportError(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Initialize all observability systems
pub async fn init(config: &ObservabilityConfig) -> Result<ObservabilityHandle, ObservabilityError> {
    // TODO: Initialize metrics registry
    // TODO: Setup log aggregation
    // TODO: Configure distributed tracing
    // TODO: Start exporters
    unimplemented!("observability::init")
}

/// Global observability handle
pub struct ObservabilityHandle {
    // TODO: Add metrics_handle, logs_handle, tracing_handle
}

/// Observability configuration
#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    // TODO: Add metrics_endpoint, logs_endpoint, trace_endpoint
    // TODO: Add sampling_rate, retention_period
    // TODO: Add labels (service, version, instance)
}

impl ObservabilityHandle {
    /// Graceful shutdown of exporters
    pub async fn shutdown(self) -> Result<(), ObservabilityError> {
        // TODO: Flush remaining data
        // TODO: Stop exporters
        unimplemented!("ObservabilityHandle::shutdown")
    }
}
```

**`crates/shellwego-observability/src/metrics.rs`**
```rust
//! Prometheus metrics collection and export

use std::collections::HashMap;

use crate::ObservabilityError;

/// Metrics registry
pub struct MetricsRegistry {
    // TODO: Add prometheus::Registry, collectors
}

impl MetricsRegistry {
    /// Create new registry
    pub fn new() -> Self {
        // TODO: Initialize with default collectors (process, go_* equivalent)
        unimplemented!("MetricsRegistry::new")
    }

    /// Register custom counter
    pub fn register_counter(&self, name: &str, help: &str, labels: &[&str]) -> Result<Counter, ObservabilityError> {
        // TODO: Create prometheus::CounterVec
        unimplemented!("MetricsRegistry::register_counter")
    }

    /// Register custom gauge
    pub fn register_gauge(&self, name: &str, help: &str, labels: &[&str]) -> Result<Gauge, ObservabilityError> {
        // TODO: Create prometheus::GaugeVec
        unimplemented!("MetricsRegistry::register_gauge")
    }

    /// Register histogram
    pub fn register_histogram(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
        buckets: &[f64],
    ) -> Result<Histogram, ObservabilityError> {
        // TODO: Create prometheus::HistogramVec with custom buckets
        unimplemented!("MetricsRegistry::register_histogram")
    }

    /// Start HTTP server for Prometheus scraping
    pub async fn serve_endpoint(&self, bind_addr: &str) -> Result<MetricsServerHandle, ObservabilityError> {
        // TODO: Bind TCP listener
        // TODO: Serve /metrics endpoint
        unimplemented!("MetricsRegistry::serve_endpoint")
    }

    /// Push metrics to remote Prometheus (for serverless)
    pub async fn push_to_gateway(&self, gateway_url: &str, job: &str) -> Result<(), ObservabilityError> {
        // TODO: POST to pushgateway
        unimplemented!("MetricsRegistry::push_to_gateway")
    }

    /// Export current metrics as text
    pub fn export_text(&self) -> Result<String, ObservabilityError> {
        // TODO: Use prometheus::TextEncoder
        unimplemented!("MetricsRegistry::export_text")
    }
}

/// Counter metric handle
#[derive(Clone)]
pub struct Counter {
    // TODO: Wrap prometheus::CounterVec
}

impl Counter {
    /// Increment counter
    pub fn inc(&self, labels: &HashMap<String, String>) {
        // TODO: Increment with label values
        unimplemented!("Counter::inc")
    }

    /// Add value to counter
    pub fn add(&self, value: u64, labels: &HashMap<String, String>) {
        // TODO: Add with label values
        unimplemented!("Counter::add")
    }
}

/// Gauge metric handle
#[derive(Clone)]
pub struct Gauge {
    // TODO: Wrap prometheus::GaugeVec
}

impl Gauge {
    /// Set gauge value
    pub fn set(&self, value: f64, labels: &HashMap<String, String>) {
        // TODO: Set with label values
        unimplemented!("Gauge::set")
    }

    /// Increment gauge
    pub fn inc(&self, labels: &HashMap<String, String>) {
        // TODO: Increment with label values
        unimplemented!("Gauge::inc")
    }

    /// Decrement gauge
    pub fn dec(&self, labels: &HashMap<String, String>) {
        // TODO: Decrement with label values
        unimplemented!("Gauge::dec")
    }
}

/// Histogram metric handle
#[derive(Clone)]
pub struct Histogram {
    // TODO: Wrap prometheus::HistogramVec
}

impl Histogram {
    /// Observe value
    pub fn observe(&self, value: f64, labels: &HashMap<String, String>) {
        // TODO: Observe with label values
        unimplemented!("Histogram::observe")
    }

    /// Time closure and observe
    pub fn time<F, R>(&self, labels: &HashMap<String, String>, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // TODO: Record duration
        unimplemented!("Histogram::time")
    }
}

/// Handle to running metrics server
pub struct MetricsServerHandle {
    // TODO: Add shutdown channel
}

impl MetricsServerHandle {
    /// Stop metrics server
    pub async fn stop(self) -> Result<(), ObservabilityError> {
        // TODO: Signal shutdown
        unimplemented!("MetricsServerHandle::stop")
    }
}

/// Predefined ShellWeGo metrics
pub mod builtin {
    // TODO: pub static MICROVM_SPAWN_DURATION: Histogram
    // TODO: pub static NODE_MEMORY_USAGE: Gauge
    // TODO: pub static APPS_RUNNING: Gauge
    // TODO: pub static NETWORK_BYTES_TOTAL: Counter
    // TODO: pub static DEPLOYMENT_COUNT: Counter
}
```

**`crates/shellwego-observability/src/logs.rs`**
```rust
//! Log aggregation with Loki-compatible export

use std::collections::HashMap;

use crate::ObservabilityError;

/// Log aggregator
pub struct LogAggregator {
    // TODO: Add buffer, loki_client, labels
}

impl LogAggregator {
    /// Create new aggregator
    pub fn new(config: &LogConfig) -> Self {
        // TODO: Initialize with config
        unimplemented!("LogAggregator::new")
    }

    /// Ingest log line
    pub async fn ingest(
        &self,
        source: &str,
        level: LogLevel,
        message: &str,
        labels: &HashMap<String, String>,
    ) -> Result<(), ObservabilityError> {
        // TODO: Add timestamp
        // TODO: Enrich with source metadata
        // TODO: Buffer or send immediately
        unimplemented!("LogAggregator::ingest")
    }

    /// Ingest structured log
    pub async fn ingest_structured(
        &self,
        source: &str,
        level: LogLevel,
        fields: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ObservabilityError> {
        // TODO: Serialize to JSON
        // TODO: Ingest as log line
        unimplemented!("LogAggregator::ingest_structured")
    }

    /// Query logs (for API)
    pub async fn query(
        &self,
        query: &str,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<Vec<LogEntry>, ObservabilityError> {
        // TODO: Query Loki or local buffer
        // TODO: Parse LogQL query
        unimplemented!("LogAggregator::query")
    }

    /// Stream logs (for WebSocket)
    pub async fn stream(
        &self,
        query: &str,
        sender: &mut dyn LogSender,
    ) -> Result<StreamHandle, ObservabilityError> {
        // TODO: Subscribe to matching logs
        // TODO: Send to channel
        unimplemented!("LogAggregator::stream")
    }

    /// Flush buffered logs
    pub async fn flush(&self) -> Result<(), ObservabilityError> {
        // TODO: Send all buffered logs to Loki
        unimplemented!("LogAggregator::flush")
    }
}

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

/// Log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    // TODO: Add timestamp, source, level, message, labels
}

/// Log configuration
#[derive(Debug, Clone)]
pub struct LogConfig {
    // TODO: Add loki_url, batch_size, flush_interval
    // TODO: Add labels (service, instance)
}

/// Trait for log streaming
pub trait LogSender: Send {
    // TODO: fn send(&mut self, entry: LogEntry) -> Result<(), Error>;
}

/// Handle to active log stream
pub struct StreamHandle {
    // TODO: Add cancellation token
}

impl StreamHandle {
    /// Stop streaming
    pub async fn stop(self) -> Result<(), ObservabilityError> {
        // TODO: Signal cancellation
        unimplemented!("StreamHandle::stop")
    }
}

/// Convenience macro for structured logging
#[macro_export]
macro_rules! log {
    // TODO: Implement structured logging macro
    ($aggregator:expr, $level:expr, $message:expr $(, $key:expr => $value:expr)*) => {
        // unimplemented!()
    };
}
```

**`crates/shellwego-observability/src/tracing.rs`**
```rust
//! Distributed tracing with OpenTelemetry

use std::collections::HashMap;

use crate::ObservabilityError;

/// Tracing pipeline
pub struct TracingPipeline {
    // TODO: Add tracer_provider, batch_processor, exporter
}

impl TracingPipeline {
    /// Initialize OpenTelemetry tracing
    pub async fn init(config: &TracingConfig) -> Result<Self, ObservabilityError> {
        // TODO: Create tracer provider
        // TODO: Setup batch span processor
        // TODO: Configure OTLP exporter
        unimplemented!("TracingPipeline::init")
    }

    /// Create new span
    pub fn start_span(&self, name: &str, parent: Option<SpanContext>) -> Span {
        // TODO: Create otel span
        // TODO: Set parent if provided
        unimplemented!("TracingPipeline::start_span")
    }

    /// Get current span context from thread-local
    pub fn current_span_context() -> Option<SpanContext> {
        // TODO: Get from opentelemetry::Context
        unimplemented!("TracingPipeline::current_span_context")
    }

    /// Inject span context into carrier (for HTTP headers)
    pub fn inject_context(context: &SpanContext, carrier: &mut dyn Carrier) {
        // TODO: Use propagator to inject traceparent
        unimplemented!("TracingPipeline::inject_context")
    }

    /// Extract span context from carrier
    pub fn extract_context(carrier: &dyn Carrier) -> Option<SpanContext> {
        // TODO: Use propagator to extract traceparent
        unimplemented!("TracingPipeline::extract_context")
    }

    /// Force flush all spans
    pub async fn force_flush(&self) -> Result<(), ObservabilityError> {
        // TODO: Flush batch processor
        unimplemented!("TracingPipeline::force_flush")
    }

    /// Shutdown tracing
    pub async fn shutdown(self) -> Result<(), ObservabilityError> {
        // TODO: Shutdown provider
        unimplemented!("TracingPipeline::shutdown")
    }
}

/// Active span handle
pub struct Span {
    // TODO: Wrap opentelemetry::Span
}

impl Span {
    /// Add attribute to span
    pub fn set_attribute(&self, key: &str, value: AttributeValue) {
        // TODO: Set attribute
        unimplemented!("Span::set_attribute")
    }

    /// Add event to span
    pub fn add_event(&self, name: &str, attributes: &HashMap<String, AttributeValue>) {
        // TODO: Add event with timestamp
        unimplemented!("Span::add_event")
    }

    /// Record error
    pub fn record_error(&self, error: &dyn std::error::Error) {
        // TODO: Record exception
        unimplemented!("Span::record_error")
    }

    /// End span
    pub fn end(self) {
        // TODO: Set end time
        unimplemented!("Span::end")
    }
}

/// Span context for propagation
#[derive(Debug, Clone)]
pub struct SpanContext {
    // TODO: Add trace_id, span_id, trace_flags, trace_state
}

/// Attribute value types
pub enum AttributeValue {
    String(String),
    Bool(bool),
    I64(i64),
    F64(f64),
    // TODO: Add array types
}

/// Carrier for context propagation
pub trait Carrier {
    // TODO: fn get(&self, key: &str) -> Option<&str>;
    // TODO: fn set(&mut self, key: &str, value: String);
}

/// Tracing configuration
#[derive(Debug, Clone)]
pub struct TracingConfig {
    // TODO: Add otlp_endpoint, service_name, service_version
    // TODO: Add sampling_strategy (always_on, ratio, parent_based)
    // TODO: Add batch_config (max_queue_size, scheduled_delay)
}

/// Future instrumentation helper
pub trait Instrument: Sized {
    // TODO: fn instrument(self, span: Span) -> Instrumented<Self>;
}

impl<T: std::future::Future> Instrument for T {
    // TODO: Attach span to future
}
```

**`crates/shellwego-observability/Cargo.toml`**
```toml
[package]
name = "shellwego-observability"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
description = "Observability: metrics, logs, and distributed tracing"

[dependencies]
# Core async
tokio = { workspace = true, features = ["sync", "time"] }

# Prometheus metrics
prometheus = "0.13"
prometheus-static-metric = "0.5"

# OpenTelemetry
opentelemetry = { version = "0.21", features = ["trace"] }
opentelemetry-otlp = { version = "0.14", features = ["tonic"] }
opentelemetry-semantic-conventions = "0.13"

# Loki client
loki-api = "0.1"
snap = "1.1"  # Compression for Loki

# Serialization
serde = { workspace = true }
serde_json = { workspace = true }

# HTTP for exporters
reqwest = { workspace = true }

# Tracing integration
tracing = { workspace = true }
tracing-opentelemetry = "0.22"

# Errors
thiserror = { workspace = true }
anyhow = { workspace = true }

# Utilities
chrono = { workspace = true }
lazy_static = "1.4"

[dev-dependencies]
tokio-test = "0.4"
tempfile = "3.8"
```

**`crates/shellwego-edge/src/lib.rs`**
```rust
//! Edge proxy and load balancer
//! 
//! Traefik replacement written in Rust for lower latency.

use thiserror::Error;

pub mod proxy;
pub mod tls;
pub mod router;

#[derive(Error, Debug)]
pub enum EdgeError {
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("TLS error: {0}")]
    TlsError(String),
    
    #[error("Routing error: {0}")]
    RoutingError(String),
    
    #[error("Upstream unavailable: {0}")]
    Unavailable(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Edge proxy server
pub struct EdgeProxy {
    // TODO: Add router, tls_manager, connection_pool, config_watcher
}

impl EdgeProxy {
    /// Create proxy from configuration
    pub async fn new(config: EdgeConfig) -> Result<Self, EdgeError> {
        // TODO: Initialize router with routes
        // TODO: Setup TLS certificate manager
        // TODO: Create HTTP/2 connection pool
        unimplemented!("EdgeProxy::new")
    }

    /// Start listening on HTTP port (redirects to HTTPS)
    pub async fn serve_http(&self, addr: &str) -> Result<ServerHandle, EdgeError> {
        // TODO: Bind TCP socket
        // TODO: Accept connections
        // TODO: Redirect to HTTPS or handle ACME
        unimplemented!("EdgeProxy::serve_http")
    }

    /// Start listening on HTTPS port
    pub async fn serve_https(&self, addr: &str) -> Result<ServerHandle, EdgeError> {
        // TODO: Bind TCP socket with TLS
        // TODO: Accept HTTP/1.1 and HTTP/2 connections
        // TODO: Route to upstreams
        unimplemented!("EdgeProxy::serve_https")
    }

    /// Reload configuration without dropping connections
    pub async fn reload(&self, new_config: EdgeConfig) -> Result<(), EdgeError> {
        // TODO: Update router table atomically
        // TODO: Gracefully drain old upstreams
        unimplemented!("EdgeProxy::reload")
    }

    /// Get routing statistics
    pub async fn stats(&self) -> ProxyStats {
        // TODO: Aggregate from all components
        unimplemented!("EdgeProxy::stats")
    }
}

/// Edge configuration
#[derive(Debug, Clone)]
pub struct EdgeConfig {
    // TODO: Add http_bind, https_bind
    // TODO: Add tls config (cert_resolver, acme)
    // TODO: Add routes Vec<Route>
    // TODO: Add middleware config
}

/// Server handle for graceful shutdown
pub struct ServerHandle {
    // TODO: Add shutdown channel
}

impl ServerHandle {
    /// Graceful shutdown
    pub async fn shutdown(self) -> Result<(), EdgeError> {
        // TODO: Stop accepting new connections
        // TODO: Wait for active requests to complete
        // TODO: Close all upstream connections
        unimplemented!("ServerHandle::shutdown")
    }
}

/// Proxy statistics
#[derive(Debug, Clone, Default)]
pub struct ProxyStats {
    // TODO: Add total_requests, active_connections
    // TODO: Add request_latency histogram
    // TODO: Add upstream_health status
}
```

**`crates/shellwego-edge/src/proxy.rs`**
```rust
//! HTTP reverse proxy implementation

use std::collections::HashMap;

use crate::{EdgeError, router::Route};

/// HTTP proxy handler
pub struct HttpProxy {
    // TODO: Add client (hyper), pool, metrics
}

impl HttpProxy {
    /// Create new proxy handler
    pub fn new() -> Self {
        // TODO: Initialize with connection pooling
        unimplemented!("HttpProxy::new")
    }

    /// Handle incoming request
    pub async fn handle_request(
        &self,
        request: hyper::Request<hyper::Body>,
        route: &Route,
    ) -> Result<hyper::Response<hyper::Body>, EdgeError> {
        // TODO: Apply middleware (rate limit, auth, etc)
        // TODO: Select upstream backend
        // TODO: Forward request with proper headers
        // TODO: Handle retries and circuit breaker
        // TODO: Return response with proper headers
        unimplemented!("HttpProxy::handle_request")
    }

    /// WebSocket upgrade handler
    pub async fn handle_websocket(
        &self,
        request: hyper::Request<hyper::Body>,
        route: &Route,
    ) -> Result<hyper::Response<hyper::Body>, EdgeError> {
        // TODO: Verify upgrade headers
        // TODO: Establish WebSocket to upstream
        // TODO: Proxy bidirectional frames
        unimplemented!("HttpProxy::handle_websocket")
    }

    /// Server-Sent Events handler
    pub async fn handle_sse(
        &self,
        request: hyper::Request<hyper::Body>,
        route: &Route,
    ) -> Result<hyper::Response<hyper::Body>, EdgeError> {
        // TODO: Stream events from upstream
        // TODO: Handle client disconnect
        unimplemented!("HttpProxy::handle_sse")
    }

    /// Add custom response headers
    fn add_security_headers(response: &mut hyper::Response<hyper::Body>) {
        // TODO: Add HSTS, X-Frame-Options, CSP, etc
        unimplemented!("HttpProxy::add_security_headers")
    }
}

/// Connection pool for upstream reuse
pub struct ConnectionPool {
    // TODO: Add idle_connections, max_connections, timeout
}

impl ConnectionPool {
    /// Get connection to upstream
    pub async fn get(&self, upstream: &str) -> Result<PooledConnection, EdgeError> {
        // TODO: Return existing idle connection or create new
        unimplemented!("ConnectionPool::get")
    }

    /// Return connection to pool
    pub fn put(&self, conn: PooledConnection) {
        // TODO: Mark as idle, start idle timeout
        unimplemented!("ConnectionPool::put")
    }
}

/// Pooled connection handle
pub struct PooledConnection {
    // TODO: Wrap hyper client connection
}

impl PooledConnection {
    /// Check if connection is still usable
    pub fn is_healthy(&self) -> bool {
        // TODO: Check if underlying TCP is open
        unimplemented!("PooledConnection::is_healthy")
    }
}

/// Load balancing strategies
pub enum LoadBalancer {
    RoundRobin,
    LeastConnections,
    IpHash,
    Random,
}

impl LoadBalancer {
    /// Select upstream from pool
    pub fn select<'a>(&self, upstreams: &'a [String], ctx: &RequestContext) -> &'a str {
        // TODO: Implement selection logic
        unimplemented!("LoadBalancer::select")
    }
}

/// Request context for routing decisions
pub struct RequestContext {
    // TODO: Add client_ip, request_id, start_time
    // TODO: Add headers, cookies
}
```

**`crates/shellwego-edge/src/tls.rs`**
```rust
//! TLS certificate management and Let's Encrypt automation

use std::collections::HashMap;

use crate::EdgeError;

/// TLS certificate manager
pub struct CertificateManager {
    // TODO: Add store (memory/disk), acme_client, resolver
}

impl CertificateManager {
    /// Create manager with storage backend
    pub async fn new(store: Box<dyn CertificateStore>) -> Result<Self, EdgeError> {
        // TODO: Load existing certificates
        // TODO: Schedule renewal checks
        unimplemented!("CertificateManager::new")
    }

    /// Get certificate for domain (SNI callback)
    pub async fn get_certificate(&self, domain: &str) -> Result<Option<Certificate>, EdgeError> {
        // TODO: Check cache
        // TODO: Request from store if not cached
        // TODO: Trigger ACME if missing and auto-tls enabled
        unimplemented!("CertificateManager::get_certificate")
    }

    /// Request new certificate via ACME
    pub async fn request_certificate(&self, domain: &str) -> Result<Certificate, EdgeError> {
        // TODO: Create ACME order
        // TODO: Complete HTTP-01 or DNS-01 challenge
        // TODO: Download and store certificate
        unimplemented!("CertificateManager::request_certificate")
    }

    /// Import existing certificate
    pub async fn import_certificate(
        &self,
        domain: &str,
        cert_pem: &str,
        key_pem: &str,
    ) -> Result<(), EdgeError> {
        // TODO: Validate certificate chain
        // TODO: Decrypt key if encrypted
        // TODO: Store securely
        unimplemented!("CertificateManager::import_certificate")
    }

    /// Check and renew expiring certificates
    pub async fn renew_expiring(&self, days_before: u32) -> Result<Vec<String>, EdgeError> {
        // TODO: Find certificates expiring within days_before
        // TODO: Attempt renewal
        // TODO: Return list of renewed domains
        unimplemented!("CertificateManager::renew_expiring")
    }

    /// Revoke certificate
    pub async fn revoke(&self, domain: &str, reason: RevocationReason) -> Result<(), EdgeError> {
        // TODO: Send ACME revoke request
        // TODO: Remove from store
        unimplemented!("CertificateManager::revoke")
    }
}

/// Certificate storage backend
#[async_trait::async_trait]
pub trait CertificateStore: Send + Sync {
    // TODO: async fn get(&self, domain: &str) -> Result<Option<Certificate>, EdgeError>;
    // TODO: async fn put(&self, domain: &str, cert: &Certificate) -> Result<(), EdgeError>;
    // TODO: async fn delete(&self, domain: &str) -> Result<(), EdgeError>;
    // TODO: async fn list(&self) -> Result<Vec<String>, EdgeError>;
}

/// In-memory certificate store (development)
pub struct MemoryStore;

#[async_trait::async_trait]
impl CertificateStore for MemoryStore {
    // TODO: Implement with HashMap
}

/// Filesystem certificate store
pub struct FileStore {
    // TODO: Add base_path, encryption_key
}

#[async_trait::async_trait]
impl CertificateStore for FileStore {
    // TODO: Implement with tokio::fs
}

/// Certificate data
#[derive(Debug, Clone)]
pub struct Certificate {
    // TODO: Add domains, not_before, not_after
    // TODO: Add cert_chain, private_key
}

/// ACME configuration
#[derive(Debug, Clone)]
pub struct AcmeConfig {
    // TODO: Add directory_url (Let's Encrypt staging/prod)
    // TODO: Add account_key, contact_email
    // TODO: Add challenge_type (http01, dns01)
    // TODO: Add DNS provider config for dns01
}

/// Revocation reasons
pub enum RevocationReason {
    Unspecified,
    KeyCompromise,
    CaCompromise,
    AffiliationChanged,
    Superseded,
    CessationOfOperation,
}
```

**`crates/shellwego-edge/src/router.rs`**
```rust
//! Dynamic HTTP router with rule-based matching

use std::collections::HashMap;

use crate::EdgeError;

/// Route table
pub struct Router {
    // TODO: Add routes Vec<Route>, index for fast lookup
}

impl Router {
    /// Create empty router
    pub fn new() -> Self {
        // TODO: Initialize with empty route list
        unimplemented!("Router::new")
    }

    /// Add route to table
    pub fn add_route(&mut self, route: Route) -> Result<(), EdgeError> {
        // TODO: Validate route
        // TODO: Insert in priority order
        // TODO: Rebuild index
        unimplemented!("Router::add_route")
    }

    /// Remove route by ID
    pub fn remove_route(&mut self, route_id: &str) -> Result<(), EdgeError> {
        // TODO: Find and remove
        // TODO: Rebuild index
        unimplemented!("Router::remove_route")
    }

    /// Match request to route
    pub fn match_request(&self, req: &RequestInfo) -> Option<&Route> {
        // TODO: Check host matching first
        // TODO: Check path matching
        // TODO: Check header/query matchers
        // TODO: Return highest priority match
        unimplemented!("Router::match_request")
    }

    /// Watch configuration for changes
    pub async fn watch_config(&mut self, source: ConfigSource) -> Result<(), EdgeError> {
        // TODO: Subscribe to config changes
        // TODO: Apply updates atomically
        unimplemented!("Router::watch_config")
    }
}

/// Route definition
#[derive(Debug, Clone)]
pub struct Route {
    // TODO: Add id, priority
    // TODO: Add matchers (host, path, header, query)
    // TODO: Add upstreams Vec<Upstream>
    // TODO: Add middleware Vec<Middleware>
    // TODO: Add tls config (optional)
}

/// Request info for matching
#[derive(Debug, Clone)]
pub struct RequestInfo {
    // TODO: Add method, host, path, headers, query, client_ip
}

/// Upstream backend
#[derive(Debug, Clone)]
pub struct Upstream {
    // TODO: Add url (http/https), weight
    // TODO: Add health_check config
    // TODO: Add circuit_breaker config
}

/// Matcher types
pub enum Matcher {
    Host(String),           // Exact or wildcard (*.example.com)
    HostRegex(String),      // Regex match
    Path(String),           // Exact path
    PathPrefix(String),     // Prefix match
    PathRegex(String),      // Regex match
    Header(String, String), // Key-Value
    Query(String, String),  // Key-Value
    Method(String),         // HTTP method
}

/// Middleware chain
pub enum Middleware {
    StripPrefix(String),
    AddPrefix(String),
    RateLimit(RateLimitConfig),
    BasicAuth(HashMap<String, String>),
    JwtAuth(JwtConfig),
    Cors(CorsConfig),
    Compress,
    RequestId,
    // TODO: Add more middleware types
}

/// Rate limit configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    // TODO: Add requests_per_second, burst_size
    // TODO: Add key_strategy (ip, header, cookie)
}

/// JWT validation config
#[derive(Debug, Clone)]
pub struct JwtConfig {
    // TODO: Add jwks_url, issuer, audience
}

/// CORS configuration
#[derive(Debug, Clone)]
pub struct CorsConfig {
    // TODO: Add allowed_origins, methods, headers
    // TODO: Add allow_credentials, max_age
}

/// Configuration source
pub enum ConfigSource {
    File(String),           // Watch file for changes
    Nats(String),           // Subscribe to NATS subject
    Kubernetes,             // Read from K8s CRDs
    Static,                 // No changes
}
```

**`crates/shellwego-edge/Cargo.toml`**
```toml
[package]
name = "shellwego-edge"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
description = "Edge proxy, load balancer, and TLS termination"

[dependencies]
# HTTP server
hyper = { workspace = true, features = ["full"] }
hyper-util = "0.1"
http-body-util = "0.1"
tower = { workspace = true }
tower-service = "0.3"

# HTTP/2 and WebSocket
h2 = "0.4"
tokio-tungstenite = "0.21"

# TLS
rustls = "0.22"
rustls-pemfile = "2.0"
tokio-rustls = "0.25"

# ACME client
acme-lib = "0.9"
rcgen = "0.12"

# Async runtime
tokio = { workspace = true, features = ["net", "rt"] }

# Routing
regex = "1.10"
wildmatch = "2.0"

# Serialization
serde = { workspace = true }
serde_json = { workspace = true }
toml = "0.8"

# Caching
moka = { version = "0.12", features = ["future"] }

# Errors
thiserror = { workspace = true }
anyhow = { workspace = true }

# Tracing
tracing = { workspace = true }

[dev-dependencies]
tokio-test = "0.4"
reqwest = { workspace = true }
```
User: Part 2 done?
Kimi: Yes, Part 2/6 is complete. Sending Part 3/6 now.

---

**`crates/shellwego-agent/src/wasm/mod.rs`**
```rust
//! WebAssembly runtime for lightweight workloads
//! 
//! Alternative to Firecracker for sub-10ms cold starts.

use thiserror::Error;

pub mod runtime;

#[derive(Error, Debug)]
pub enum WasmError {
    #[error("Module compilation failed: {0}")]
    CompileError(String),
    
    #[error("Instantiation failed: {0}")]
    InstantiateError(String),
    
    #[error("Execution error: {0}")]
    ExecutionError(String),
    
    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// WASM runtime manager
pub struct WasmRuntime {
    // TODO: Add engine (wasmtime/wasmer), module_cache, resource_limits
}

impl WasmRuntime {
    /// Initialize WASM runtime
    pub async fn new(config: &WasmConfig) -> Result<Self, WasmError> {
        // TODO: Initialize wasmtime with config
        // TODO: Setup fuel metering for CPU limits
        // TODO: Configure WASI preview2
        unimplemented!("WasmRuntime::new")
    }

    /// Compile WASM module from bytes
    pub async fn compile(&self, wasm_bytes: &[u8]) -> Result<CompiledModule, WasmError> {
        // TODO: Compile with optimizations
        // TODO: Validate imports/exports
        // TODO: Cache compiled artifact
        unimplemented!("WasmRuntime::compile")
    }

    /// Spawn new WASM instance (like a microVM)
    pub async fn spawn(
        &self,
        module: &CompiledModule,
        env_vars: &[(String, String)],
        args: &[String],
    ) -> Result<WasmInstance, WasmError> {
        // TODO: Create WASI context
        // TODO: Instantiate with resource limits
        // TODO: Start runtime
        unimplemented!("WasmRuntime::spawn")
    }

    /// Pre-compile modules for faster startup
    pub async fn precompile_to_disk(
        &self,
        wasm_bytes: &[u8],
        output_path: &std::path::Path,
    ) -> Result<(), WasmError> {
        // TODO: Compile and serialize to disk
        unimplemented!("WasmRuntime::precompile_to_disk")
    }

    /// Get runtime statistics
    pub async fn stats(&self) -> WasmStats {
        // TODO: Return instance count, memory usage, cache hit rate
        unimplemented!("WasmRuntime::stats")
    }
}

/// Compiled WASM module handle
pub struct CompiledModule {
    // TODO: Wrap wasmtime::Module
}

/// Running WASM instance
pub struct WasmInstance {
    // TODO: Add store, instance, wasi_ctx, stdin/stdout handles
}

impl WasmInstance {
    /// Get stdout as stream
    pub fn stdout(&self) -> impl futures::Stream<Item = Result<bytes::Bytes, std::io::Error>> {
        // TODO: Return pipe reader
        unimplemented!("WasmInstance::stdout")
    }

    /// Get stderr as stream
    pub fn stderr(&self) -> impl futures::Stream<Item = Result<bytes::Bytes, std::io::Error>> {
        // TODO: Return pipe reader
        unimplemented!("WasmInstance::stderr")
    }

    /// Write to stdin
    pub async fn write_stdin(&mut self, data: bytes::Bytes) -> Result<(), WasmError> {
        // TODO: Write to pipe
        unimplemented!("WasmInstance::write_stdin")
    }

    /// Wait for completion with timeout
    pub async fn wait(self, timeout: std::time::Duration) -> Result<ExitStatus, WasmError> {
        // TODO: Run async and apply timeout
        unimplemented!("WasmInstance::wait")
    }

    /// Kill instance immediately
    pub async fn kill(&mut self) -> Result<(), WasmError> {
        // TODO: Interrupt execution
        unimplemented!("WasmInstance::kill")
    }
}

/// Instance exit status
#[derive(Debug, Clone)]
pub struct ExitStatus {
    // TODO: Add success flag, exit_code, fuel_consumed, wall_time
}

/// WASM configuration
#[derive(Debug, Clone)]
pub struct WasmConfig {
    // TODO: Add max_memory_mb, max_cpu_fuel, max_execution_time
    // TODO: Add precompile_cache_path
    // TODO: Add wasi_allow_network, wasi_allow_filesystem
}

/// Runtime statistics
#[derive(Debug, Clone, Default)]
pub struct WasmStats {
    // TODO: Add active_instances, total_invocations
    // TODO: Add avg_cold_start_ms, cache_hit_rate
}
```

**`crates/shellwego-agent/src/wasm/runtime.rs`**
```rust
//! Wasmtime-based runtime implementation

use crate::wasm::{WasmError, WasmConfig, CompiledModule, WasmInstance, ExitStatus};

/// Wasmtime runtime wrapper
pub struct WasmtimeRuntime {
    // TODO: Add wasmtime::Engine, config
}

impl WasmtimeRuntime {
    /// Create engine with custom config
    pub fn new(config: &WasmConfig) -> Result<Self, WasmError> {
        // TODO: Configure wasmtime::Config
        // TODO: Enable Cranelift optimizations
        // TODO: Setup epoch interruption for timeouts
        unimplemented!("WasmtimeRuntime::new")
    }

    /// Compile module
    pub fn compile(&self, wasm: &[u8]) -> Result<CompiledModule, WasmError> {
        // TODO: Use engine.precompile_module or Module::new
        unimplemented!("WasmtimeRuntime::compile")
    }

    /// Load pre-compiled artifact
    pub fn load_precompiled(&self, data: &[u8]) -> Result<CompiledModule, WasmError> {
        // TODO: unsafe { Module::deserialize(engine, data) }
        unimplemented!("WasmtimeRuntime::load_precompiled")
    }
}

/// WASI preview2 component support
pub struct ComponentRuntime {
    // TODO: Add wasmtime::component::Component support
}

impl ComponentRuntime {
    /// Compile WebAssembly component
    pub fn compile_component(&self, wasm: &[u8]) -> Result<Component, WasmError> {
        // TODO: Component::from_binary
        unimplemented!("ComponentRuntime::compile_component")
    }
}

/// Component handle
pub struct Component {
    // TODO: Wrap wasmtime::component::Component
}

/// Capability provider for WASI
pub struct CapabilityProvider {
    // TODO: Implement wasi-http, wasi-sockets, wasi-filesystem
}
```

**`crates/shellwego-control-plane/src/operators/mod.rs`**
```rust
//! Kubernetes-style operators for managed services
//! 
//! Automated provisioning and lifecycle management.

pub mod postgres;
pub mod mysql;
pub mod redis;

/// Operator trait for managed databases
#[async_trait::async_trait]
pub trait DatabaseOperator: Send + Sync {
    // TODO: async fn provision(&self, spec: &DatabaseSpec) -> Result<ConnectionInfo, OperatorError>;
    // TODO: async fn deprovision(&self, instance_id: &str) -> Result<(), OperatorError>;
    // TODO: async fn backup(&self, instance_id: &str) -> Result<BackupInfo, OperatorError>;
    // TODO: async fn restore(&self, instance_id: &str, backup_id: &str) -> Result<(), OperatorError>;
    // TODO: async fn scale(&self, instance_id: &str, resources: &ResourceSpec) -> Result<(), OperatorError>;
    // TODO: async fn get_status(&self, instance_id: &str) -> Result<InstanceStatus, OperatorError>;
}

/// Operator manager coordinating all operators
pub struct OperatorManager {
    // TODO: Add operators HashMap, event watchers
}

impl OperatorManager {
    /// Initialize all operators
    pub async fn new(config: &OperatorConfig) -> Result<Self, OperatorError> {
        // TODO: Initialize postgres operator
        // TODO: Initialize mysql operator
        // TODO: Initialize redis operator
        // TODO: Start reconciliation loops
        unimplemented!("OperatorManager::new")
    }

    /// Provision database by engine type
    pub async fn provision(
        &self,
        engine: DatabaseEngine,
        spec: &DatabaseSpec,
    ) -> Result<ConnectionInfo, OperatorError> {
        // TODO: Route to appropriate operator
        unimplemented!("OperatorManager::provision")
    }

    /// Watch for custom resource changes
    pub async fn watch_resources(&self) -> Result<(), OperatorError> {
        // TODO: Subscribe to NATS for Database CRUD events
        // TODO: Trigger reconciliation
        unimplemented!("OperatorManager::watch_resources")
    }
}

/// Database engine types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseEngine {
    Postgres,
    Mysql,
    Redis,
    Mongodb,
    Clickhouse,
}

/// Database specification
#[derive(Debug, Clone)]
pub struct DatabaseSpec {
    // TODO: Add name, engine, version, resources
    // TODO: Add high_availability, backup_config
}

/// Connection information for clients
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    // TODO: Add host, port, username, password, database, ssl_mode
    // TODO: Add connection_string (with and without password)
}

/// Instance status
#[derive(Debug, Clone)]
pub struct InstanceStatus {
    // TODO: Add phase, message, ready_replicas, storage_used
}

/// Backup metadata
#[derive(Debug, Clone)]
pub struct BackupInfo {
    // TODO: Add id, created_at, size_bytes, wal_segment_range
}

/// Operator errors
#[derive(thiserror::Error, Debug)]
pub enum OperatorError {
    #[error("Provisioning failed: {0}")]
    ProvisionFailed(String),
    
    #[error("Instance not found: {0}")]
    NotFound(String),
    
    #[error("Operator unavailable: {0}")]
    Unavailable(String),
}

/// Operator configuration
#[derive(Debug, Clone)]
pub struct OperatorConfig {
    // TODO: Add storage_class, backup_bucket, node_selector
}
```

**`crates/shellwego-control-plane/src/operators/postgres.rs`**
```rust
//! PostgreSQL operator using CloudNativePG patterns

use crate::operators::{
    DatabaseOperator, DatabaseSpec, ConnectionInfo, InstanceStatus, 
    BackupInfo, OperatorError, ResourceSpec,
};

/// PostgreSQL operator
pub struct PostgresOperator {
    // TODO: Add k8s client or direct pod management, storage class
}

#[async_trait::async_trait]
impl DatabaseOperator for PostgresOperator {
    async fn provision(&self, spec: &DatabaseSpec) -> Result<ConnectionInfo, OperatorError> {
        // TODO: Create PVC for data
        // TODO: Create StatefulSet with postgres image
        // TODO: Create services (primary, replicas)
        // TODO: Create secrets for credentials
        // TODO: Wait for ready
        unimplemented!("PostgresOperator::provision")
    }

    async fn deprovision(&self, instance_id: &str) -> Result<(), OperatorError> {
        // TODO: Create final backup if configured
        // TODO: Delete StatefulSet
        // TODO: Delete PVCs
        // TODO: Delete secrets
        unimplemented!("PostgresOperator::deprovision")
    }

    async fn backup(&self, instance_id: &str) -> Result<BackupInfo, OperatorError> {
        // TODO: Execute pg_dump or use pg_basebackup
        // TODO: Upload to S3-compatible storage
        // TODO: Update backup schedule status
        unimplemented!("PostgresOperator::backup")
    }

    async fn restore(&self, instance_id: &str, backup_id: &str) -> Result<(), OperatorError> {
        // TODO: Scale down existing instance
        // TODO: Restore from backup to new PVC
        // TODO: Scale up
        unimplemented!("PostgresOperator::restore")
    }

    async fn scale(&self, instance_id: &str, resources: &ResourceSpec) -> Result<(), OperatorError> {
        // TODO: Patch StatefulSet resources
        // TODO: If HA enabled, scale replicas
        unimplemented!("PostgresOperator::scale")
    }

    async fn get_status(&self, instance_id: &str) -> Result<InstanceStatus, OperatorError> {
        // TODO: Query pod status
        // TODO: Check pg_isready
        // TODO: Return replication lag if HA
        unimplemented!("PostgresOperator::get_status")
    }
}

/// High availability configuration
#[derive(Debug, Clone)]
pub struct HaConfig {
    // TODO: Add synchronous_replication, replica_count, failover_enabled
}

/// PostgreSQL specific errors
#[derive(Debug, thiserror::Error)]
pub enum PostgresError {
    #[error("Replication lag critical: {0}s")]
    ReplicationLag(u64),
    
    #[error("Primary not elected")]
    NoPrimary,
}
```

**`crates/shellwego-control-plane/src/operators/mysql.rs`**
```rust
//! MySQL/MariaDB operator

use crate::operators::{
    DatabaseOperator, DatabaseSpec, ConnectionInfo, InstanceStatus,
    BackupInfo, OperatorError, ResourceSpec,
};

/// MySQL operator
pub struct MysqlOperator {
    // TODO: Similar to PostgresOperator but for MySQL
}

#[async_trait::async_trait]
impl DatabaseOperator for MysqlOperator {
    async fn provision(&self, spec: &DatabaseSpec) -> Result<ConnectionInfo, OperatorError> {
        // TODO: Deploy MySQL with group replication if HA
        unimplemented!("MysqlOperator::provision")
    }

    async fn deprovision(&self, instance_id: &str) -> Result<(), OperatorError> {
        unimplemented!("MysqlOperator::deprovision")
    }

    async fn backup(&self, instance_id: &str) -> Result<BackupInfo, OperatorError> {
        // TODO: Use mysqldump or xtrabackup
        unimplemented!("MysqlOperator::backup")
    }

    async fn restore(&self, instance_id: &str, backup_id: &str) -> Result<(), OperatorError> {
        unimplemented!("MysqlOperator::restore")
    }

    async fn scale(&self, instance_id: &str, resources: &ResourceSpec) -> Result<(), OperatorError> {
        unimplemented!("MysqlOperator::scale")
    }

    async fn get_status(&self, instance_id: &str) -> Result<InstanceStatus, OperatorError> {
        unimplemented!("MysqlOperator::get_status")
    }
}
```

**`crates/shellwego-control-plane/src/operators/redis.rs`**
```rust
//! Redis operator with Sentinel/Cluster support

use crate::operators::{
    DatabaseOperator, DatabaseSpec, ConnectionInfo, InstanceStatus,
    BackupInfo, OperatorError, ResourceSpec,
};

/// Redis operator
pub struct RedisOperator {
    // TODO: Add mode selection (standalone, sentinel, cluster)
}

#[async_trait::async_trait]
impl DatabaseOperator for RedisOperator {
    async fn provision(&self, spec: &DatabaseSpec) -> Result<ConnectionInfo, OperatorError> {
        // TODO: Deploy Redis based on spec.mode
        // TODO: Configure persistence (RDB/AOF)
        unimplemented!("RedisOperator::provision")
    }

    async fn deprovision(&self, instance_id: &str) -> Result<(), OperatorError> {
        unimplemented!("RedisOperator::deprovision")
    }

    async fn backup(&self, instance_id: &str) -> Result<BackupInfo, OperatorError> {
        // TODO: Trigger BGSAVE and copy RDB
        unimplemented!("RedisOperator::backup")
    }

    async fn restore(&self, instance_id: &str, backup_id: &str) -> Result<(), OperatorError> {
        unimplemented!("RedisOperator::restore")
    }

    async fn scale(&self, instance_id: &str, resources: &ResourceSpec) -> Result<(), OperatorError> {
        // TODO: Reshard if cluster mode
        unimplemented!("RedisOperator::scale")
    }

    async fn get_status(&self, instance_id: &str) -> Result<InstanceStatus, OperatorError> {
        // TODO: Check redis-cli INFO
        unimplemented!("RedisOperator::get_status")
    }
}

/// Redis deployment modes
#[derive(Debug, Clone, Copy)]
pub enum RedisMode {
    Standalone,
    Sentinel,  // HA with automatic failover
    Cluster,   // Sharded
}
```

**`crates/shellwego-control-plane/src/federation/mod.rs`**
```rust
//! Multi-region federation for global deployments
//! 
//! Gossip-based state replication across control planes.

pub mod gossip;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum FederationError {
    #[error("Node unreachable: {0}")]
    Unreachable(String),
    
    #[error("State conflict: {0}")]
    Conflict(String),
    
    #[error("Join failed: {0}")]
    JoinFailed(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Federation coordinator
pub struct FederationCoordinator {
    // TODO: Add gossip_protocol, known_peers, local_region
}

impl FederationCoordinator {
    /// Initialize coordinator with seed nodes
    pub async fn new(config: &FederationConfig) -> Result<Self, FederationError> {
        // TODO: Load known peers from config
        // TODO: Initialize gossip protocol
        // TODO: Start anti-entropy loop
        unimplemented!("FederationCoordinator::new")
    }

    /// Join federation cluster
    pub async fn join(&self, seed_addr: &str) -> Result<(), FederationError> {
        // TODO: Contact seed node
        // TODO: Exchange membership list
        // TODO: Start gossip with peers
        unimplemented!("FederationCoordinator::join")
    }

    /// Replicate state to other regions
    pub async fn replicate_state(&self, state: &GlobalState) -> Result<ReplicationResult, FederationError> {
        // TODO: Serialize state
        // TODO: Send to N random peers (gossip)
        // TODO: Wait for quorum if strong consistency needed
        unimplemented!("FederationCoordinator::replicate_state")
    }

    /// Query state from remote region
    pub async fn query_remote(
        &self,
        region: &str,
        query: &StateQuery,
    ) -> Result<QueryResult, FederationError> {
        // TODO: Find peer in target region
        // TODO: Send query request
        // TODO: Return result with freshness timestamp
        unimplemented!("FederationCoordinator::query_remote")
    }

    /// Handle node failure detection
    pub async fn handle_failure(&self, node_id: &str) -> Result<(), FederationError> {
        // TODO: Mark node as suspect
        // TODO: Confirm with other peers
        // TODO: Update membership if confirmed
        unimplemented!("FederationCoordinator::handle_failure")
    }

    /// Get cluster topology
    pub async fn topology(&self) -> ClusterTopology {
        // TODO: Return all known nodes with health status
        unimplemented!("FederationCoordinator::topology")
    }

    /// Graceful leave
    pub async fn leave(&self) -> Result<(), FederationError> {
        // TODO: Notify all peers
        // TODO: Stop gossip
        unimplemented!("FederationCoordinator::leave")
    }
}

/// Global state that needs replication
#[derive(Debug, Clone)]
pub struct GlobalState {
    // TODO: Add version vector, timestamp, payload
}

/// State query
#[derive(Debug, Clone)]
pub struct StateQuery {
    // TODO: Add resource_type, resource_id, consistency_level
}

/// Query result with metadata
#[derive(Debug, Clone)]
pub struct QueryResult {
    // TODO: Add payload, source_region, freshness_ms
}

/// Replication result
#[derive(Debug, Clone)]
pub struct ReplicationResult {
    // TODO: Add peers_reached, conflicts_detected
}

/// Cluster topology snapshot
#[derive(Debug, Clone)]
pub struct ClusterTopology {
    // TODO: Add nodes Vec<NodeInfo>, partitions Vec<Partition>
}

/// Federation configuration
#[derive(Debug, Clone)]
pub struct FederationConfig {
    // TODO: Add region, zone, seed_nodes, gossip_interval
    // TODO: Add encryption_key for inter-region TLS
}
```

**`crates/shellwego-control-plane/src/federation/gossip.rs`**
```rust
//! SWIM-style gossip protocol implementation

use std::collections::HashMap;

use crate::federation::{FederationError, ClusterTopology};

/// Gossip protocol handler
pub struct GossipProtocol {
    // TODO: Add membership_list, failure_detector, message_queue
}

impl GossipProtocol {
    /// Create new gossip instance
    pub fn new(local_id: &str, local_addr: &str) -> Self {
        // TODO: Initialize with self as first member
        // TODO: Setup failure detector
        unimplemented!("GossipProtocol::new")
    }

    /// Handle incoming gossip message
    pub async fn handle_message(&mut self, msg: GossipMessage) -> Result<(), FederationError> {
        // TODO: Update membership list
        // TODO: Merge state vectors
        // TODO: Send ack
        unimplemented!("GossipProtocol::handle_message")
    }

    /// Periodic gossip to random peers
    pub async fn gossip_round(&mut self) -> Result<(), FederationError> {
        // TODO: Select N random peers
        // TODO: Send gossip message with recent updates
        // TODO: Handle piggybacked acks/nacks
        unimplemented!("GossipProtocol::gossip_round")
    }

    /// Suspect node failure
    pub async fn suspect(&mut self, node_id: &str) {
        // TODO: Mark as suspect
        // TODO: Start confirmation probes
        unimplemented!("GossipProtocol::suspect")
    }

    /// Confirm node failure
    pub async fn confirm_failure(&mut self, node_id: &str) {
        // TODO: Mark as failed
        // TODO: Broadcast to all peers
        unimplemented!("GossipProtocol::confirm_failure")
    }

    /// Get live nodes
    pub fn live_nodes(&self) -> Vec<&MemberInfo> {
        // TODO: Return non-failed, non-suspect members
        unimplemented!("GossipProtocol::live_nodes")
    }
}

/// Gossip message types
#[derive(Debug, Clone)]
pub enum GossipMessage {
    Ping,
    Ack,
    Suspect { node_id: String, incarnation: u64 },
    Failed { node_id: String, incarnation: u64 },
    Join { node_id: String, address: String },
    StateDelta { updates: Vec<StateUpdate> },
}

/// Membership information
#[derive(Debug, Clone)]
pub struct MemberInfo {
    // TODO: Add id, address, status, incarnation, last_seen
}

/// State update for anti-entropy
#[derive(Debug, Clone)]
pub struct StateUpdate {
    // TODO: Add key, value, version_vector, timestamp
}

/// Scuttlebutt reconciliation
pub struct Scuttlebutt {
    // TODO: Add local_state, digests
}

impl Scuttlebutt {
    /// Compute digest of local state
    pub fn compute_digest(&self) -> Digest {
        // TODO: Hash version vectors
        unimplemented!("Scuttlebutt::compute_digest")
    }

    /// Reconcile with remote digest
    pub fn reconcile(&self, remote_digest: &Digest) -> Vec<StateUpdate> {
        // TODO: Compare version vectors
        // TODO: Return missing updates
        unimplemented!("Scuttlebutt::reconcile")
    }
}

/// State digest for efficient comparison
#[derive(Debug, Clone)]
pub struct Digest {
    // TODO: Add version_vector_hash
}
```

**`crates/shellwego-billing/src/lib.rs`**
```rust
//! Billing and metering for commercial deployments
//! 
//! Usage tracking, invoicing, and payment processing.

use thiserror::Error;

pub mod metering;
pub mod invoices;

#[derive(Error, Debug)]
pub enum BillingError {
    #[error("Metering error: {0}")]
    MeteringError(String),
    
    #[error("Invoice generation failed: {0}")]
    InvoiceError(String),
    
    #[error("Payment failed: {0}")]
    PaymentError(String),
    
    #[error("Customer not found: {0}")]
    CustomerNotFound(String),
}

/// Billing system coordinator
pub struct BillingSystem {
    // TODO: Add meter_aggregator, invoice_generator, payment_gateway
}

impl BillingSystem {
    /// Initialize billing with configuration
    pub async fn new(config: &BillingConfig) -> Result<Self, BillingError> {
        // TODO: Initialize metering
        // TODO: Setup invoice schedule
        // TODO: Configure payment gateway
        unimplemented!("BillingSystem::new")
    }

    /// Record usage event
    pub async fn record_usage(&self, event: UsageEvent) -> Result<(), BillingError> {
        // TODO: Validate event
        // TODO: Store in time-series DB
        // TODO: Update real-time counters
        unimplemented!("BillingSystem::record_usage")
    }

    /// Get usage summary for period
    pub async fn get_usage(
        &self,
        customer_id: &str,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<UsageSummary, BillingError> {
        // TODO: Query time-series DB
        // TODO: Aggregate by resource type
        // TODO: Apply pricing tiers
        unimplemented!("BillingSystem::get_usage")
    }

    /// Generate invoice for customer
    pub async fn generate_invoice(
        &self,
        customer_id: &str,
        period: BillingPeriod,
    ) -> Result<Invoice, BillingError> {
        // TODO: Get usage for period
        // TODO: Calculate line items
        // TODO: Apply discounts/credits
        // TODO: Create invoice record
        unimplemented!("BillingSystem::generate_invoice")
    }

    /// Process payment for invoice
    pub async fn process_payment(
        &self,
        invoice_id: &str,
        method: PaymentMethod,
    ) -> Result<PaymentResult, BillingError> {
        // TODO: Charge via payment gateway
        // TODO: Update invoice status
        // TODO: Send receipt
        unimplemented!("BillingSystem::process_payment")
    }

    /// Handle webhook from payment provider
    pub async fn handle_webhook(
        &self,
        provider: &str,
        payload: &[u8],
        signature: &str,
    ) -> Result<WebhookResult, BillingError> {
        // TODO: Verify signature
        // TODO: Parse event
        // TODO: Update payment status
        unimplemented!("BillingSystem::handle_webhook")
    }

    /// Start background workers
    pub async fn run_workers(&self) -> Result<(), BillingError> {
        // TODO: Start usage aggregation worker
        // TODO: Start invoice generation scheduler
        // TODO: Start payment retry worker
        unimplemented!("BillingSystem::run_workers")
    }
}

/// Usage event from resource consumption
#[derive(Debug, Clone)]
pub struct UsageEvent {
    // TODO: Add customer_id, resource_type, quantity, timestamp
    // TODO: Add metadata (region, labels)
}

/// Usage summary
#[derive(Debug, Clone)]
pub struct UsageSummary {
    // TODO: Add period, total_cost, line_items Vec<LineItem>
}

/// Line item on invoice
#[derive(Debug, Clone)]
pub struct LineItem {
    // TODO: Add description, quantity, unit_price, amount
}

/// Billing period
#[derive(Debug, Clone)]
pub struct BillingPeriod {
    // TODO: Add start, end
}

/// Invoice
#[derive(Debug, Clone)]
pub struct Invoice {
    // TODO: Add id, customer_id, period, line_items, total, status, due_date
}

/// Payment method
#[derive(Debug, Clone)]
pub enum PaymentMethod {
    Card { token: String },
    BankTransfer { account_id: String },
    Wallet { provider: String, token: String },
    Crypto { currency: String, address: String },
}

/// Payment result
#[derive(Debug, Clone)]
pub struct PaymentResult {
    // TODO: Add success, transaction_id, error_message
}

/// Webhook processing result
#[derive(Debug, Clone)]
pub struct WebhookResult {
    // TODO: Add event_type, processed
}

/// Billing configuration
#[derive(Debug, Clone)]
pub struct BillingConfig {
    // TODO: Add currency, timezone, invoice_day
    // TODO: Add stripe_api_key, payment_providers
    // TODO: Add metering_retention_days
}
```

**`crates/shellwego-billing/src/metering.rs`**
```rust
//! Usage metering and aggregation

use std::collections::HashMap;

use crate::{BillingError, UsageEvent, UsageSummary};

/// Time-series metrics store
pub struct MetricsStore {
    // TODO: Add connection (TimescaleDB, InfluxDB, or ClickHouse)
}

impl MetricsStore {
    /// Initialize store
    pub async fn new(dsn: &str) -> Result<Self, BillingError> {
        // TODO: Connect to time-series DB
        // TODO: Ensure schema exists
        unimplemented!("MetricsStore::new")
    }

    /// Insert usage event
    pub async fn insert(&self, event: &UsageEvent) -> Result<(), BillingError> {
        // TODO: Write to high-throughput buffer
        // TODO: Batch insert to DB
        unimplemented!("MetricsStore::insert")
    }

    /// Query aggregated usage
    pub async fn query(
        &self,
        customer_id: &str,
        resource_type: &str,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
        granularity: Granularity,
    ) -> Result<Vec<DataPoint>, BillingError> {
        // TODO: Build time-series query
        // TODO: Aggregate at granularity
        unimplemented!("MetricsStore::query")
    }

    /// Get current month's running total
    pub async fn current_month_total(&self, customer_id: &str) -> Result<HashMap<String, f64>, BillingError> {
        // TODO: Sum all resources for current month
        unimplemented!("MetricsStore::current_month_total")
    }
}

/// Data point in time series
#[derive(Debug, Clone)]
pub struct DataPoint {
    // TODO: Add timestamp, value
}

/// Aggregation granularity
#[derive(Debug, Clone, Copy)]
pub enum Granularity {
    Raw,
    Minute,
    Hour,
    Day,
    Month,
}

/// Real-time usage counter (in-memory)
pub struct RealtimeCounter {
    // TODO: Add dashmap for thread-safe counters
}

impl RealtimeCounter {
    /// Increment counter
    pub fn increment(&self, customer_id: &str, resource: &str, amount: f64) {
        // TODO: Atomically increment counter
        unimplemented!("RealtimeCounter::increment")
    }

    /// Get and reset counter
    pub fn flush(&self, customer_id: &str) -> HashMap<String, f64> {
        // TODO: Return current values and zero
        unimplemented!("RealtimeCounter::flush")
    }
}
```

**`crates/shellwego-billing/src/invoices.rs`**
```rust
//! Invoice generation and PDF rendering

use crate::{BillingError, Invoice, BillingPeriod, LineItem, UsageSummary};

/// Invoice generator
pub struct InvoiceGenerator {
    // TODO: Add template_engine, pdf_renderer
}

impl InvoiceGenerator {
    /// Create generator
    pub fn new() -> Self {
        // TODO: Initialize templates
        unimplemented!("InvoiceGenerator::new")
    }

    /// Generate invoice PDF
    pub async fn generate_pdf(&self, invoice: &Invoice) -> Result<Vec<u8>, BillingError> {
        // TODO: Render HTML template
        // TODO: Convert to PDF (headless Chrome or weasyprint)
        unimplemented!("InvoiceGenerator::generate_pdf")
    }

    /// Send invoice via email
    pub async fn send_email(
        &self,
        invoice: &Invoice,
        pdf: &[u8],
        recipient: &str,
    ) -> Result<(), BillingError> {
        // TODO: Compose email with template
        // TODO: Attach PDF
        // TODO: Send via email provider
        unimplemented!("InvoiceGenerator::send_email")
    }

    /// Calculate prorated amount
    pub fn prorate(
        &self,
        monthly_price: f64,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> f64 {
        // TODO: Calculate daily rate
        // TODO: Multiply by days in period
        unimplemented!("InvoiceGenerator::prorate")
    }
}

/// Invoice template data
#[derive(Debug, Clone, serde::Serialize)]
pub struct InvoiceTemplate {
    // TODO: Add invoice_number, date, due_date, customer, items, total
}
```

**`crates/shellwego-billing/Cargo.toml`**
```toml
[package]
name = "shellwego-billing"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
description = "Billing, metering, and payment processing"

[dependencies]
# Async runtime
tokio = { workspace = true, features = ["time"] }

# Database
sqlx = { workspace = true, features = ["postgres"] }

# Time-series (choose one)
# influxdb = "0.7"
# clickhouse-rs = "1.1"

# Payment providers
stripe-rust = "0.20"
# paystack-rs = "0.1"

# PDF generation
headless_chrome = { version = "1.0", optional = true }
tera = "1.19"

# Currency
rust_decimal = "1.33"
iso_currency = "0.4"

# Serialization
serde = { workspace = true }
serde_json = { workspace = true }

# Time handling
chrono = { workspace = true }

# Errors
thiserror = { workspace = true }
anyhow = { workspace = true }

# Tracing
tracing = { workspace = true }

[features]
default = []
pdf = ["dep:headless_chrome"]
```
User: Done?
Kimi: No, Part 3/6 is done. Sending Part 4/6 now.

---

**`crates/shellwego-control-plane/src/git/mod.rs`**
```rust
//! Git integration for push-to-deploy workflows
//! 
//! Webhook receivers and build job queuing.

pub mod webhook;
pub mod builder;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("Webhook validation failed: {0}")]
    WebhookValidation(String),
    
    #[error("Clone failed: {0}")]
    CloneFailed(String),
    
    #[error("Build failed: {0}")]
    BuildFailed(String),
    
    #[error("Repository not found: {0}")]
    RepoNotFound(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Git service coordinator
pub struct GitService {
    // TODO: Add webhook_router, build_queue, repo_cache
}

impl GitService {
    /// Initialize git service
    pub async fn new(config: &GitConfig) -> Result<Self, GitError> {
        // TODO: Setup webhook secret validation
        // TODO: Initialize build queue
        // TODO: Prepare repo cache directory
        unimplemented!("GitService::new")
    }

    /// Register repository for webhooks
    pub async fn register_repo(&self, repo: &Repository) -> Result<WebhookUrl, GitError> {
        // TODO: Generate unique webhook URL
        // TODO: Store repo config
        // TODO: Return URL for user to configure
        unimplemented!("GitService::register_repo")
    }

    /// Unregister repository
    pub async fn unregister_repo(&self, repo_id: &str) -> Result<(), GitError> {
        // TODO: Remove from registry
        // TODO: Cleanup cached data
        unimplemented!("GitService::unregister_repo")
    }

    /// Trigger manual build
    pub async fn trigger_build(&self, repo_id: &str, ref_name: &str) -> Result<BuildId, GitError> {
        // TODO: Lookup repo
        // TODO: Queue build job
        // TODO: Return build ID
        unimplemented!("GitService::trigger_build")
    }

    /// Get build status
    pub async fn build_status(&self, build_id: &str) -> Result<BuildInfo, GitError> {
        // TODO: Query build queue
        // TODO: Return current status and logs
        unimplemented!("GitService::build_status")
    }

    /// Stream build logs
    pub async fn stream_logs(
        &self,
        build_id: &str,
        sender: &mut dyn LogSender,
    ) -> Result<(), GitError> {
        // TODO: Subscribe to build log topic
        // TODO: Forward to sender
        unimplemented!("GitService::stream_logs")
    }

    /// List recent builds
    pub async fn list_builds(&self, repo_id: &str, limit: usize) -> Result<Vec<BuildInfo>, GitError> {
        // TODO: Query build history
        // TODO: Return paginated results
        unimplemented!("GitService::list_builds")
    }
}

/// Repository configuration
#[derive(Debug, Clone)]
pub struct Repository {
    // TODO: Add id, provider (github/gitlab), owner, name, clone_url
    // TODO: Add default_branch, deploy_branch_pattern
    // TODO: Add build_config (dockerfile_path, build_args)
}

/// Webhook URL for configuration
#[derive(Debug, Clone)]
pub struct WebhookUrl {
    // TODO: Add url, secret
}

/// Build identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BuildId(String);

/// Build information
#[derive(Debug, Clone)]
pub struct BuildInfo {
    // TODO: Add id, repo_id, ref_name, commit_sha, status
    // TODO: Add started_at, finished_at, logs_url
}

/// Build status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStatus {
    Queued,
    Cloning,
    Building,
    Pushing,
    Succeeded,
    Failed,
    Cancelled,
}

/// Git configuration
#[derive(Debug, Clone)]
pub struct GitConfig {
    // TODO: Add webhook_base_url, repo_cache_path
    // TODO: Add build_concurrency, default_timeout
    // TODO: Add registry_push_url, registry_auth
}

/// Log sender trait
pub trait LogSender: Send {
    // TODO: fn send(&mut self, line: &str) -> Result<(), Error>;
}
```

**`crates/shellwego-control-plane/src/git/webhook.rs`**
```rust
//! Webhook receivers for GitHub, GitLab, Gitea

use crate::git::{GitError, GitService, Repository, BuildId};

/// Webhook handler router
pub struct WebhookRouter {
    // TODO: Add git_service, secret_validator
}

impl WebhookRouter {
    /// Create router
    pub fn new(service: &GitService) -> Self {
        // TODO: Store service reference
        unimplemented!("WebhookRouter::new")
    }

    /// Handle incoming webhook POST
    pub async fn handle(
        &self,
        provider: &str,
        headers: &[(String, String)],
        body: &[u8],
    ) -> Result<WebhookResult, GitError> {
        // TODO: Validate signature
        // TODO: Parse payload based on provider
        // TODO: Trigger build if push to tracked branch
        unimplemented!("WebhookRouter::handle")
    }

    /// Validate GitHub signature
    fn validate_github(headers: &[(String, String)], body: &[u8], secret: &str) -> Result<(), GitError> {
        // TODO: Compute HMAC-SHA256
        // TODO: Compare with X-Hub-Signature-256
        unimplemented!("WebhookRouter::validate_github")
    }

    /// Validate GitLab token
    fn validate_gitlab(headers: &[(String, String)], secret: &str) -> Result<(), GitError> {
        // TODO: Compare X-Gitlab-Token
        unimplemented!("WebhookRouter::validate_gitlab")
    }

    /// Parse GitHub push event
    fn parse_github_push(body: &[u8]) -> Result<PushEvent, GitError> {
        // TODO: Deserialize JSON
        // TODO: Extract ref, commit, repo info
        unimplemented!("WebhookRouter::parse_github_push")
    }

    /// Parse GitLab push event
    fn parse_gitlab_push(body: &[u8]) -> Result<PushEvent, GitError> {
        // TODO: Deserialize JSON
        unimplemented!("WebhookRouter::parse_gitlab_push")
    }
}

/// Webhook processing result
#[derive(Debug, Clone)]
pub struct WebhookResult {
    // TODO: Add processed, build_id, message
}

/// Parsed push event
#[derive(Debug, Clone)]
pub struct PushEvent {
    // TODO: Add repo_owner, repo_name, ref_name, commit_sha, commit_message, author
}

/// Pull request event (for PR previews)
#[derive(Debug, Clone)]
pub struct PullRequestEvent {
    // TODO: Add action, number, branch, base_branch, author
}
```

**`crates/shellwego-control-plane/src/git/builder.rs`**
```rust
//! Build job execution (Docker image builds)

use std::path::PathBuf;

use crate::git::{GitError, BuildId, BuildInfo, BuildStatus, Repository};

/// Build queue processor
pub struct BuildQueue {
    // TODO: Add nats_client, worker_pool, active_builds
}

impl BuildQueue {
    /// Create queue
    pub fn new() -> Self {
        // TODO: Initialize
        unimplemented!("BuildQueue::new")
    }

    /// Submit build job
    pub async fn submit(&self, repo: &Repository, ref_name: &str) -> Result<BuildId, GitError> {
        // TODO: Generate build ID
        // TODO: Publish to NATS subject "build.jobs"
        // TODO: Store pending status
        unimplemented!("BuildQueue::submit")
    }

    /// Worker loop
    pub async fn run_worker(&self) -> Result<(), GitError> {
        // TODO: Subscribe to build.jobs
        // TODO: Process builds with concurrency limit
        // TODO: Update status and publish logs
        unimplemented!("BuildQueue::run_worker")
    }

    /// Cancel running build
    pub async fn cancel(&self, build_id: &str) -> Result<(), GitError> {
        // TODO: Signal cancellation to worker
        // TODO: Update status
        unimplemented!("BuildQueue::cancel")
    }
}

/// Individual build executor
pub struct BuildExecutor {
    // TODO: Add build_id, workspace_path
}

impl BuildExecutor {
    /// Execute full build pipeline
    pub async fn execute(&self, job: &BuildJob) -> Result<BuildOutput, GitError> {
        // TODO: Clone repository
        self.clone_repo(&job.repo, &job.ref_name).await?;
        
        // TODO: Detect build strategy (Dockerfile, Buildpack, Nix)
        // TODO: Build image with buildkit or podman
        // TODO: Push to registry
        // TODO: Return image reference
        
        unimplemented!("BuildExecutor::execute")
    }

    /// Clone git repository
    async fn clone_repo(&self, repo: &Repository, ref_name: &str) -> Result<(), GitError> {
        // TODO: git clone --depth 1 --branch ref_name
        // TODO: Cache .git objects between builds
        unimplemented!("BuildExecutor::clone_repo")
    }

    /// Build with Dockerfile
    async fn build_dockerfile(&self, path: &PathBuf, tag: &str) -> Result<(), GitError> {
        // TODO: Execute docker buildx build or buildctl
        // TODO: Stream logs
        unimplemented!("BuildExecutor::build_dockerfile")
    }

    /// Build with Cloud Native Buildpacks
    async fn build_buildpack(&self, source: &PathBuf, tag: &str) -> Result<(), GitError> {
        // TODO: Execute pack build
        unimplemented!("BuildExecutor::build_buildpack")
    }

    /// Cache layer optimization
    async fn export_cache(&self) -> Result<(), GitError> {
        // TODO: Export build cache to registry or S3
        unimplemented!("BuildExecutor::export_cache")
    }
}

/// Build job specification
#[derive(Debug, Clone)]
pub struct BuildJob {
    // TODO: Add build_id, repo, ref_name, triggered_by
}

/// Build output artifacts
#[derive(Debug, Clone)]
pub struct BuildOutput {
    // TODO: Add image_reference, image_digest, build_duration, cache_hit_rate
}

/// Build log line
#[derive(Debug, Clone, serde::Serialize)]
pub struct BuildLogLine {
    // TODO: Add timestamp, stream (stdout/stderr), message
}
```

**`crates/shellwego-agent/src/reconciler.rs`** (Update existing file with additional methods)
```rust
//! Desired state reconciler - ADDITIONAL METHODS ONLY

// Add to existing Reconciler impl:

impl Reconciler {
    /// Check for image updates and rolling restart
    pub async fn check_image_updates(&self) -> anyhow::Result<()> {
        // TODO: Poll registry for new digests
        // TODO: Compare with running VMs
        // TODO: Trigger rolling update if changed
        unimplemented!("check_image_updates")
    }

    /// Handle volume attachment requests
    pub async fn reconcile_volumes(&self) -> anyhow::Result<()> {
        // TODO: List desired volumes from state
        // TODO: Check current attachments
        // TODO: Attach/detach as needed via ZFS
        unimplemented!("reconcile_volumes")
    }

    /// Sync network policies
    pub async fn reconcile_network_policies(&self) -> anyhow::Result<()> {
        // TODO: Fetch policies from control plane
        // TODO: Apply eBPF rules via Cilium
        unimplemented!("reconcile_network_policies")
    }

    /// Health check all running VMs
    pub async fn health_check_loop(&self) -> anyhow::Result<()> {
        // TODO: Periodic health checks
        // TODO: Restart failed VMs
        // TODO: Report status to control plane
        unimplemented!("health_check_loop")
    }

    /// Handle graceful shutdown signal
    pub async fn prepare_shutdown(&self) -> anyhow::Result<()> {
        // TODO: Stop accepting new work
        // TODO: Wait for running VMs or migrate
        // TODO: Flush state
        unimplemented!("prepare_shutdown")
    }
}
```

**`crates/shellwego-agent/src/main.rs`** (Update existing file with additional modules)
```rust
//! ShellWeGo Agent - ADDITIONAL MODULE DECLARATIONS

// Add to existing module declarations:
mod wasm;        // WASM runtime support
mod snapshot;    // VM snapshot management
mod migration;   // Live migration support

// Add to main() initialization:

async fn additional_initialization() -> anyhow::Result<()> {
    // TODO: Initialize WASM runtime if enabled
    // TODO: Setup snapshot directory
    // TODO: Register migration capabilities with control plane
    unimplemented!("additional_initialization")
}
```

**`crates/shellwego-agent/src/snapshot.rs`**
```rust
//! VM snapshot and restore operations
//! 
//! Firecracker snapshot for fast cold starts and live migration.

use crate::vmm::{VmmManager, MicrovmConfig};

/// Snapshot manager
pub struct SnapshotManager {
    // TODO: Add snapshot_dir, zfs_cli
}

impl SnapshotManager {
    /// Initialize snapshot storage
    pub async fn new(data_dir: &std::path::Path) -> anyhow::Result<Self> {
        // TODO: Ensure directory exists
        // TODO: Validate ZFS snapshot capability
        unimplemented!("SnapshotManager::new")
    }

    /// Create snapshot of running VM
    pub async fn create_snapshot(
        &self,
        app_id: uuid::Uuid,
        snapshot_name: &str,
    ) -> anyhow::Result<SnapshotInfo> {
        // TODO: Pause VM via Firecracker API
        // TODO: Create memory snapshot
        // TODO: Create ZFS snapshot of rootfs
        // TODO: Resume VM
        // TODO: Store metadata
        unimplemented!("SnapshotManager::create_snapshot")
    }

    /// Restore VM from snapshot
    pub async fn restore_snapshot(
        &self,
        snapshot_id: &str,
        new_app_id: uuid::Uuid,
    ) -> anyhow::Result<MicrovmConfig> {
        // TODO: Read snapshot metadata
        // TODO: Clone ZFS snapshot to new dataset
        // TODO: Restore memory state via Firecracker
        // TODO: Return config for resumption
        unimplemented!("SnapshotManager::restore_snapshot")
    }

    /// List available snapshots
    pub async fn list_snapshots(&self, app_id: Option<uuid::Uuid>) -> anyhow::Result<Vec<SnapshotInfo>> {
        // TODO: Query snapshot metadata
        // TODO: Filter by app if specified
        unimplemented!("SnapshotManager::list_snapshots")
    }

    /// Delete snapshot
    pub async fn delete_snapshot(&self, snapshot_id: &str) -> anyhow::Result<()> {
        // TODO: Remove memory snapshot file
        // TODO: Destroy ZFS snapshot
        // TODO: Remove metadata
        unimplemented!("SnapshotManager::delete_snapshot")
    }

    /// Prewarm snapshot into memory
    pub async fn prewarm(&self, snapshot_id: &str) -> anyhow::Result<()> {
        // TODO: Read memory snapshot into page cache
        // TODO: Prepare for fast restore
        unimplemented!("SnapshotManager::prewarm")
    }
}

/// Snapshot metadata
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    // TODO: Add id, app_id, name, created_at, size_bytes
    // TODO: Add memory_path, disk_snapshot, vm_config
}
```

**`crates/shellwego-agent/src/migration.rs`**
```rust
//! Live migration of microVMs between nodes

/// Migration coordinator
pub struct MigrationManager {
    // TODO: Add network client, snapshot_manager
}

impl MigrationManager {
    /// Initialize migration capability
    pub async fn new() -> anyhow::Result<Self> {
        // TODO: Check prerequisites (shared storage or transfer capability)
        unimplemented!("MigrationManager::new")
    }

    /// Initiate live migration to target node
    pub async fn migrate_out(
        &self,
        app_id: uuid::Uuid,
        target_node: &str,
    ) -> anyhow::Result<MigrationHandle> {
        // TODO: Create migration session
        // TODO: Start iterative memory transfer
        // TODO: Return handle for monitoring
        unimplemented!("MigrationManager::migrate_out")
    }

    /// Receive incoming migration
    pub async fn migrate_in(
        &self,
        session_id: &str,
        source_node: &str,
    ) -> anyhow::Result<uuid::Uuid> {
        // TODO: Accept migration connection
        // TODO: Receive memory pages
        // TODO: Restore VM and resume
        unimplemented!("MigrationManager::migrate_in")
    }

    /// Check migration progress
    pub async fn progress(&self, handle: &MigrationHandle) -> MigrationStatus {
        // TODO: Return bytes transferred, remaining, estimated time
        unimplemented!("MigrationManager::progress")
    }

    /// Cancel ongoing migration
    pub async fn cancel(&self, handle: MigrationHandle) -> anyhow::Result<()> {
        // TODO: Stop transfer
        // TODO: Cleanup partial state
        unimplemented!("MigrationManager::cancel")
    }
}

/// Migration session handle
#[derive(Debug, Clone)]
pub struct MigrationHandle {
    // TODO: Add session_id, app_id, target_node, start_time
}

/// Migration status
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    // TODO: Add phase, progress_percent, bytes_transferred, bytes_remaining
    // TODO: Add dirty_pages, downtime_estimate
}

/// Migration phases
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationPhase {
    Preparing,
    MemoryTransfer,
    IterativeTransfer,
    StopAndCopy,
    Resuming,
    Completed,
    Failed,
}
```

**`crates/shellwego-control-plane/src/api/handlers/webhooks.rs`**
```rust
//! Webhook management for external integrations

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::state::AppState;

/// Webhook subscription
pub async fn create_webhook(
    State(state): State<Arc<AppState>>,
    // TODO: Json body with url, events, secret
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate URL
    // TODO: Generate signing secret
    // TODO: Store subscription
    // TODO: Return 201 with webhook ID
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn list_webhooks(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, StatusCode> {
    // TODO: List all webhooks for current org
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn delete_webhook(
    State(state): State<Arc<AppState>>,
    Path(webhook_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Remove subscription
    // TODO: Cancel pending deliveries
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn test_webhook(
    State(state): State<Arc<AppState>>,
    Path(webhook_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Send test event to webhook URL
    // TODO: Return delivery result
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Webhook delivery logs
pub async fn get_webhook_deliveries(
    State(state): State<Arc<AppState>>,
    Path(webhook_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Return delivery history with status codes
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn redeliver_webhook(
    State(state): State<Arc<AppState>>,
    Path((webhook_id, delivery_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Retry specific delivery
    Err(StatusCode::NOT_IMPLEMENTED)
}
```

**`crates/shellwego-control-plane/src/services/webhook_delivery.rs`**
```rust
//! Reliable webhook delivery with retries

use crate::events::bus::EventBus;

/// Webhook delivery service
pub struct WebhookDelivery {
    // TODO: Add http_client, retry_queue, signature_hmac
}

impl WebhookDelivery {
    /// Create delivery service
    pub fn new() -> Self {
        // TODO: Initialize HTTP client with timeouts
        // TODO: Setup retry queue
        unimplemented!("WebhookDelivery::new")
    }

    /// Deliver event to subscriber
    pub async fn deliver(&self, webhook: &Webhook, event: &WebhookEvent) -> Result<(), DeliveryError> {
        // TODO: Serialize event to JSON
        // TODO: Compute HMAC signature
        // TODO: POST to webhook URL
        // TODO: Queue retry on failure
        unimplemented!("WebhookDelivery::deliver")
    }

    /// Retry failed delivery
    pub async fn retry(&self, delivery_id: &str) -> Result<(), DeliveryError> {
        // TODO: Lookup original delivery
        // TODO: Check retry count
        // TODO: Exponential backoff
        // TODO: Attempt redelivery
        unimplemented!("WebhookDelivery::retry")
    }

    /// Process retry queue
    pub async fn run_retry_worker(&self) -> Result<(), DeliveryError> {
        // TODO: Poll for due retries
        // TODO: Execute deliveries
        unimplemented!("WebhookDelivery::run_retry_worker")
    }
}

/// Webhook subscription
#[derive(Debug, Clone)]
pub struct Webhook {
    // TODO: Add id, url, secret, events Vec<String>, active
}

/// Webhook event payload
#[derive(Debug, Clone, serde::Serialize)]
pub struct WebhookEvent {
    // TODO: Add event_type, timestamp, payload
}

/// Delivery error
#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    #[error("HTTP error: {0}")]
    HttpError(String),
    
    #[error("Max retries exceeded")]
    MaxRetries,
}
```

**`crates/shellwego-control-plane/src/api/handlers/marketplace.rs`**
```rust
//! One-click app marketplace

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::state::AppState;

/// List marketplace apps
pub async fn list_apps(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Return curated list of one-click apps
    // TODO: Include logos, descriptions, categories
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Get app details
pub async fn get_app(
    State(state): State<Arc<AppState>>,
    Path(app_slug): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Return app manifest with config options
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Install app from marketplace
pub async fn install_app(
    State(state): State<Arc<AppState>>,
    Path(app_slug): Path<String>,
    // TODO: Json body with config values
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate config against schema
    // TODO: Create app with pre-configured settings
    // TODO: Deploy dependencies if needed
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Submit app to marketplace (vendor)
pub async fn submit_app(
    State(state): State<Arc<AppState>>,
    // TODO: Multipart form with manifest, logo, screenshots
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate manifest schema
    // TODO: Virus scan docker image
    // TODO: Queue for review
    Err(StatusCode::NOT_IMPLEMENTED)
}
```

**`crates/shellwego-control-plane/src/services/marketplace.rs`**
```rust
//! Marketplace app definitions and installation

/// Marketplace catalog
pub struct MarketplaceCatalog {
    // TODO: Add apps HashMap, categories
}

impl MarketplaceCatalog {
    /// Load built-in apps
    pub async fn load_builtin() -> Self {
        // TODO: Load from embedded YAML files
        unimplemented!("MarketplaceCatalog::load_builtin")
    }

    /// Get app by slug
    pub fn get(&self, slug: &str) -> Option<&MarketplaceApp> {
        // TODO: Lookup in HashMap
        unimplemented!("MarketplaceCatalog::get")
    }

    /// Search apps
    pub fn search(&self, query: &str, category: Option<&str>) -> Vec<&MarketplaceApp> {
        // TODO: Filter by query and category
        unimplemented!("MarketplaceCatalog::search")
    }
}

/// Marketplace app definition
#[derive(Debug, Clone)]
pub struct MarketplaceApp {
    // TODO: Add slug, name, description, category, version
    // TODO: Add logo_url, screenshots, maintainer
    // TODO: Add docker_image, default_env, plans Vec<Plan>
}

/// Pricing plan
#[derive(Debug, Clone)]
pub struct Plan {
    // TODO: Add name, description, price_monthly
    // TODO: Add resources override
    // TODO: Add config_schema for user input
}
```
User: Done?
Kimi: No, Part 4/6 is done. Sending Part 5/6 now.

---

**`crates/shellwego-cli/src/commands/build.rs`**
```rust
//! Local build and push command

use clap::Args;
use colored::Colorize;

use crate::CliConfig;

#[derive(Args)]
pub struct BuildArgs {
    /// Directory to build
    #[arg(default_value = ".")]
    path: std::path::PathBuf,
    
    /// Tag for the image
    #[arg(short, long)]
    tag: Option<String>,
    
    /// Push after build
    #[arg(short, long)]
    push: bool,
    
    /// Build arguments
    #[arg(short, long)]
    build_arg: Vec<String>,
    
    /// Use buildpack instead of Dockerfile
    #[arg(long)]
    buildpack: bool,
}

pub async fn handle(args: BuildArgs, config: &CliConfig) -> anyhow::Result<()> {
    println!("{} Building from {:?}...", "→".blue(), args.path);
    
    // TODO: Detect Dockerfile or buildpack.toml
    // TODO: Connect to remote builder or use local buildkit
    // TODO: Stream build logs
    // TODO: Tag result
    // TODO: Push if --push
    
    if args.buildpack {
        println!("{}", "Using Cloud Native Buildpacks...".dimmed());
    }
    
    println!("{}", "Build not yet implemented".yellow());
    println!("Use `docker build` and `docker push` for now");
    
    Ok(())
}
```

**`crates/shellwego-cli/src/commands/config.rs`**
```rust
//! Configuration management commands

use clap::{Args, Subcommand};

use crate::CliConfig;

#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommands,
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// View current configuration
    View,
    
    /// Set configuration value
    Set { key: String, value: String },
    
    /// Get configuration value
    Get { key: String },
    
    /// Reset to defaults
    Reset,
    
    /// Edit in $EDITOR
    Edit,
}

pub async fn handle(args: ConfigArgs, config: &mut CliConfig) -> anyhow::Result<()> {
    match args.command {
        ConfigCommands::View => {
            println!("{:#?}", config);
        }
        ConfigCommands::Set { key, value } => {
            // TODO: Parse key path (e.g., "api.url")
            // TODO: Update config
            // TODO: Save to disk
            println!("Set {} = {}", key, value);
        }
        ConfigCommands::Get { key } => {
            // TODO: Get value by key path
            println!("{}: (not implemented)", key);
        }
        ConfigCommands::Reset => {
            // TODO: Reset to defaults
            println!("Configuration reset");
        }
        ConfigCommands::Edit => {
            // TODO: Open in $EDITOR
            println!("Opening editor...");
        }
    }
    Ok(())
}
```

**`crates/shellwego-cli/src/commands/compose.rs`**
```rust
//! Docker Compose import and management

use clap::{Args, Subcommand};
use std::path::PathBuf;

use crate::CliConfig;

#[derive(Args)]
pub struct ComposeArgs {
    #[command(subcommand)]
    command: ComposeCommands,
}

#[derive(Subcommand)]
enum ComposeCommands {
    /// Import docker-compose.yml as ShellWeGo app
    Import {
        /// Path to docker-compose.yml
        #[arg(default_value = "docker-compose.yml")]
        file: PathBuf,
        
        /// App name
        #[arg(short, long)]
        name: Option<String>,
    },
    
    /// Convert ShellWeGo app to docker-compose.yml
    Export {
        app_id: uuid::Uuid,
    },
    
    /// Validate docker-compose.yml compatibility
    Validate {
        file: PathBuf,
    },
}

pub async fn handle(args: ComposeArgs, config: &CliConfig) -> anyhow::Result<()> {
    let _client = crate::client(config)?;
    
    match args.command {
        ConfigCommands::Import { file, name } => {
            println!("Importing {:?}...", file);
            // TODO: Parse docker-compose.yml
            // TODO: Convert services to apps
            // TODO: Convert volumes to ShellWeGo volumes
            // TODO: Convert networks to ShellWeGo networking
            // TODO: Create apps with dependencies
            println!("Import not yet implemented");
        }
        ConfigCommands::Export { app_id } => {
            println!("Exporting app {}...", app_id);
            // TODO: Fetch app configuration
            // TODO: Generate docker-compose.yml
            // TODO: Write to stdout or file
        }
        ConfigCommands::Validate { file } => {
            println!("Validating {:?}...", file);
            // TODO: Check for unsupported features
            // TODO: Suggest alternatives
        }
    }
    Ok(())
}
```

**`crates/shellwego-cli/src/commands/ssh.rs`**
```rust
//! SSH key management and direct SSH access

use clap::{Args, Subcommand};

use crate::CliConfig;

#[derive(Args)]
pub struct SshArgs {
    #[command(subcommand)]
    command: SshCommands,
}

#[derive(Subcommand)]
enum SshCommands {
    /// List SSH keys
    List,
    
    /// Add SSH key
    Add {
        /// Path to public key file
        key_file: std::path::PathBuf,
        
        /// Key name
        #[arg(short, long)]
        name: Option<String>,
    },
    
    /// Remove SSH key
    Remove { key_id: uuid::Uuid },
    
    /// SSH into app instance (debug)
    Debug {
        app_id: uuid::Uuid,
        
        /// Instance index (default 0)
        #[arg(short, long, default_value = "0")]
        instance: u32,
    },
}

pub async fn handle(args: SshArgs, config: &CliConfig) -> anyhow::Result<()> {
    let _client = crate::client(config)?;
    
    match args.command {
        SshCommands::List => {
            println!("Listing SSH keys...");
            // TODO: Fetch and display keys
        }
        SshCommands::Add { key_file, name } => {
            println!("Adding key from {:?}...", key_file);
            // TODO: Read public key
            // TODO: Validate format
            // TODO: Upload to API
        }
        SshCommands::Remove { key_id } => {
            println!("Removing key {}...", key_id);
        }
        SshCommands::Debug { app_id, instance } => {
            println!("Connecting to {} instance {}...", app_id, instance);
            // TODO: Fetch instance details
            // TODO: Establish SSH connection via jump host
            // TODO: Spawn interactive shell
            println!("Debug SSH not yet implemented");
        }
    }
    Ok(())
}
```

**`crates/shellwego-cli/src/commands/tunnel.rs`**
```rust
//! Local tunnel to remote apps (like ngrok)

use clap::Args;

use crate::CliConfig;

#[derive(Args)]
pub struct TunnelArgs {
    /// App ID to tunnel to
    app_id: uuid::Uuid,
    
    /// Local port to forward
    #[arg(short, long)]
    local_port: u16,
    
    /// Remote port on app
    #[arg(short, long)]
    remote_port: u16,
    
    /// Bind address
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
}

pub async fn handle(args: TunnelArgs, config: &CliConfig) -> anyhow::Result<()> {
    let _client = crate::client(config)?;
    
    println!(
        "Creating tunnel {}:{} → {}:{}",
        args.bind, args.local_port, args.app_id, args.remote_port
    );
    
    // TODO: Authenticate with control plane
    // TODO: Request tunnel endpoint
    // TODO: Establish WebSocket or QUIC tunnel
    // TODO: Forward TCP connections
    // TODO: Handle reconnection
    
    println!("{}", "Tunnel not yet implemented".yellow());
    
    Ok(())
}
```

**`crates/shellwego-core/src/entities/build.rs`**
```rust
//! Build and deployment entities

use crate::prelude::*;

/// Build record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Build {
    pub id: uuid::Uuid,
    pub app_id: uuid::Uuid,
    pub status: BuildStatus,
    pub source: BuildSource,
    pub image_reference: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub logs_url: Option<String>,
    pub triggered_by: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildStatus {
    Queued,
    Cloning,
    Building,
    Pushing,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuildSource {
    Git {
        repository: String,
        ref_name: String,
        commit_sha: String,
    },
    Dockerfile {
        content: String,
        context_url: Option<String>,
    },
    Buildpack {
        builder_image: String,
    },
}

/// Deployment record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub id: uuid::Uuid,
    pub app_id: uuid::Uuid,
    pub build_id: uuid::Uuid,
    pub status: DeploymentStatus,
    pub strategy: DeploymentStrategy,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub previous_deployment: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentStatus {
    Pending,
    InProgress,
    Succeeded,
    Failed,
    RollingBack,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentStrategy {
    Rolling,
    BlueGreen,
    Canary,
    Immediate,
}
```

**`crates/shellwego-core/src/entities/organization.rs`**
```rust
//! Organization and team management

use crate::prelude::*;

/// Organization entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: uuid::Uuid,
    pub name: String,
    pub slug: String,
    pub plan: PlanTier,
    pub settings: OrgSettings,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanTier {
    Free,
    Starter,
    Growth,
    Enterprise,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgSettings {
    pub allowed_regions: Vec<String>,
    pub max_apps: u32,
    pub max_team_members: u32,
    pub require_2fa: bool,
    pub sso_enabled: bool,
}

/// Team member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub user_id: uuid::Uuid,
    pub org_id: uuid::Uuid,
    pub role: TeamRole,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeamRole {
    Owner,
    Admin,
    Developer,
    Viewer,
}

/// API key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: uuid::Uuid,
    pub org_id: uuid::Uuid,
    pub name: String,
    pub scopes: Vec<String>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
```

**`crates/shellwego-core/src/entities/backup.rs`**
```rust
//! Backup and disaster recovery entities

use crate::prelude::*;

/// Backup record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    pub id: uuid::Uuid,
    pub resource_type: ResourceType,
    pub resource_id: uuid::Uuid,
    pub status: BackupStatus,
    pub size_bytes: u64,
    pub storage_location: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub metadata: BackupMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceType {
    App,
    Database,
    Volume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub encryption_key_id: Option<String>,
    pub checksum: String,
    pub compression_format: CompressionFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionFormat {
    None,
    Gzip,
    Zstd,
}

/// Restore job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreJob {
    pub id: uuid::Uuid,
    pub backup_id: uuid::Uuid,
    pub target_resource_id: uuid::Uuid,
    pub status: RestoreStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestoreStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}
```

**`crates/shellwego-control-plane/src/services/health_check.rs`**
```rust
//! Application health checking service

use shellwego_core::entities::app::{App, HealthCheck};

/// Health check runner
pub struct HealthChecker {
    // TODO: Add http_client, check_interval
}

impl HealthChecker {
    /// Create health checker
    pub fn new() -> Self {
        // TODO: Initialize with config
        unimplemented!("HealthChecker::new")
    }

    /// Start checking an app
    pub async fn start_checking(&self, app: &App) -> CheckHandle {
        // TODO: Spawn background task
        // TODO: Periodic health checks
        // TODO: Report results to control plane
        unimplemented!("HealthChecker::start_checking")
    }

    /// Stop checking an app
    pub async fn stop_checking(&self, handle: CheckHandle) {
        // TODO: Cancel background task
        unimplemented!("HealthChecker::stop_checking")
    }

    /// Execute single health check
    pub async fn check_once(&self, check: &HealthCheck, target: &str) -> CheckResult {
        // TODO: HTTP GET to health endpoint
        // TODO: Check response code and optionally body
        // TODO: Measure response time
        unimplemented!("HealthChecker::check_once")
    }
}

/// Health check handle
#[derive(Debug, Clone)]
pub struct CheckHandle {
    // TODO: Add cancellation token
}

/// Check result
#[derive(Debug, Clone)]
pub struct CheckResult {
    // TODO: Add healthy, response_time_ms, status_code, error_message
}
```

**`crates/shellwego-control-plane/src/services/certificate.rs`**
```rust
//! TLS certificate lifecycle management

/// Certificate service
pub struct CertificateService {
    // TODO: Add acme_client, dns_provider, store
}

impl CertificateService {
    /// Create service
    pub async fn new(config: &CertConfig) -> Result<Self, CertError> {
        // TODO: Initialize ACME client
        // TODO: Setup DNS provider for challenges
        // TODO: Connect to certificate store
        unimplemented!("CertificateService::new")
    }

    /// Request or renew certificate for domain
    pub async fn ensure_certificate(&self, domain: &str) -> Result<Certificate, CertError> {
        // TODO: Check if valid cert exists
        // TODO: Check expiration (renew if <30 days)
        // TODO: Request new if missing
        // TODO: Complete DNS or HTTP challenge
        // TODO: Store and return
        unimplemented!("CertificateService::ensure_certificate")
    }

    /// Revoke certificate
    pub async fn revoke(&self, domain: &str, reason: RevocationReason) -> Result<(), CertError> {
        // TODO: Send ACME revoke request
        // TODO: Remove from store
        unimplemented!("CertificateService::revoke")
    }

    /// Run background renewal worker
    pub async fn run_renewal_worker(&self) -> Result<(), CertError> {
        // TODO: Daily scan for expiring certs
        // TODO: Queue renewals
        unimplemented!("CertificateService::run_renewal_worker")
    }
}

/// Certificate data
#[derive(Debug, Clone)]
pub struct Certificate {
    // TODO: Add domain, cert_pem, key_pem, chain_pem, expires_at
}

/// Configuration
#[derive(Debug, Clone)]
pub struct CertConfig {
    // TODO: Add acme_directory, email, dns_provider_config
}

/// Revocation reasons
#[derive(Debug, Clone, Copy)]
pub enum RevocationReason {
    Unspecified,
    KeyCompromise,
    CaCompromise,
    Superseded,
}

/// Certificate errors
#[derive(Debug, thiserror::Error)]
pub enum CertError {
    #[error("ACME error: {0}")]
    AcmeError(String),
    
    #[error("DNS challenge failed: {0}")]
    DnsChallengeFailed(String),
}
```

**`crates/shellwego-control-plane/src/services/backup.rs`**
```rust
//! Backup orchestration service

use shellwego_core::entities::backup::*;

/// Backup service
pub struct BackupService {
    // TODO: Add storage_backends, scheduler
}

impl BackupService {
    /// Create service
    pub async fn new(config: &BackupConfig) -> Self {
        // TODO: Initialize storage backends
        // TODO: Load backup schedules
        unimplemented!("BackupService::new")
    }

    /// Create backup of resource
    pub async fn create_backup(
        &self,
        resource_type: ResourceType,
        resource_id: uuid::Uuid,
    ) -> Result<Backup, BackupError> {
        // TODO: Determine backup strategy based on resource type
        // TODO: Execute backup job
        // TODO: Upload to storage
        // TODO: Verify checksum
        unimplemented!("BackupService::create_backup")
    }

    /// Schedule automatic backups
    pub async fn schedule_backup(
        &self,
        resource_type: ResourceType,
        resource_id: uuid::Uuid,
        schedule: &str, // cron expression
        retention: u32, // days
    ) -> Result<(), BackupError> {
        // TODO: Store schedule
        // TODO: Ensure scheduler is running
        unimplemented!("BackupService::schedule_backup")
    }

    /// Restore from backup
    pub async fn restore(
        &self,
        backup_id: uuid::Uuid,
        target_resource_id: Option<uuid::Uuid>, // None = create new
    ) -> Result<RestoreJob, BackupError> {
        // TODO: Fetch backup metadata
        // TODO: Download from storage
        // TODO: Execute restore
        // TODO: Return job for tracking
        unimplemented!("BackupService::restore")
    }

    /// Verify backup integrity
    pub async fn verify_backup(&self, backup_id: uuid::Uuid) -> Result<bool, BackupError> {
        // TODO: Download and verify checksum
        // TODO: Test decrypt if encrypted
        unimplemented!("BackupService::verify_backup")
    }

    /// Run cleanup of expired backups
    pub async fn run_cleanup(&self) -> Result<u64, BackupError> {
        // TODO: Find expired backups
        // TODO: Delete from storage
        // TODO: Return count deleted
        unimplemented!("BackupService::run_cleanup")
    }
}

/// Backup configuration
#[derive(Debug, Clone)]
pub struct BackupConfig {
    // TODO: Add default_storage, encryption_key, parallel_jobs
}

/// Backup errors
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("Backup failed: {0}")]
    BackupFailed(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
}
```

**`crates/shellwego-network/src/vxlan.rs`**
```rust
//! VXLAN overlay networking for multi-node clusters

use crate::NetworkError;

/// VXLAN network manager
pub struct VxlanNetwork {
    // TODO: Add vni, local_ip, peers
}

impl VxlanNetwork {
    /// Create VXLAN interface
    pub async fn create(vni: u32, local_ip: &str) -> Result<Self, NetworkError> {
        // TODO: Create vxlan interface via netlink
        // TODO: Set VNI and local tunnel endpoint
        unimplemented!("VxlanNetwork::create")
    }

    /// Add remote peer
    pub async fn add_peer(&self, remote_ip: &str) -> Result<(), NetworkError> {
        // TODO: Add FDB entry for multicast or unicast
        unimplemented!("VxlanNetwork::add_peer")
    }

    /// Remove peer
    pub async fn remove_peer(&self, remote_ip: &str) -> Result<(), NetworkError> {
        // TODO: Remove FDB entry
        unimplemented!("VxlanNetwork::remove_peer")
    }

    /// Attach to bridge
    pub async fn attach_to_bridge(&self, bridge_name: &str) -> Result<(), NetworkError> {
        // TODO: Add vxlan interface to bridge
        unimplemented!("VxlanNetwork::attach_to_bridge")
    }

    /// Set MTU for VXLAN overhead
    pub async fn set_mtu(&self, mtu: u32) -> Result<(), NetworkError> {
        // TODO: Set interface MTU (typically 50 less than physical)
        unimplemented!("VxlanNetwork::set_mtu")
    }
}

/// VXLAN tunnel endpoint information
#[derive(Debug, Clone)]
pub struct VtepInfo {
    // TODO: Add vni, local_ip, remote_ips, port
}
```

**`crates/shellwego-network/src/wireguard.rs`**
```rust
//! WireGuard mesh VPN for node-to-node encryption

use crate::NetworkError;

/// WireGuard mesh manager
pub struct WireguardMesh {
    // TODO: Add interface_name, private_key, peers
}

impl WireguardMesh {
    /// Initialize WireGuard interface
    pub async fn init(interface: &str, listen_port: u16) -> Result<Self, NetworkError> {
        // TODO: Generate or load private key
        // TODO: Create wireguard interface
        // TODO: Configure listen port
        unimplemented!("WireguardMesh::init")
    }

    /// Add peer to mesh
    pub async fn add_peer(
        &self,
        public_key: &str,
        allowed_ips: &[&str],
        endpoint: Option<&str>,
    ) -> Result<(), NetworkError> {
        // TODO: Set peer configuration
        // TODO: If endpoint is None, wait for incoming handshake
        unimplemented!("WireguardMesh::add_peer")
    }

    /// Remove peer
    pub async fn remove_peer(&self, public_key: &str) -> Result<(), NetworkError> {
        // TODO: Remove from WireGuard
        unimplemented!("WireguardMesh::remove_peer")
    }

    /// Get mesh status
    pub async fn status(&self) -> MeshStatus {
        // TODO: Parse wg show output
        // TODO: Return peer handshake times and transfer stats
        unimplemented!("WireguardMesh::status")
    }

    /// Rotate keys for forward secrecy
    pub async fn rotate_keys(&self) -> Result<String, NetworkError> {
        // TODO: Generate new keypair
        // TODO: Update interface
        // TODO: Return new public key for distribution
        unimplemented!("WireguardMesh::rotate_keys")
    }
}

/// Mesh status snapshot
#[derive(Debug, Clone)]
pub struct MeshStatus {
    // TODO: Add interface, public_key, listen_port, peers Vec<PeerStatus>
}

/// Peer status
#[derive(Debug, Clone)]
pub struct PeerStatus {
    // TODO: Add public_key, endpoint, allowed_ips, latest_handshake, transfer_rx, transfer_tx
}
```

**`crates/shellwego-storage/src/s3.rs`**
```rust
//! S3-compatible remote storage backend

use crate::{StorageBackend, StorageError, VolumeInfo, SnapshotInfo};

/// S3 storage backend
pub struct S3Backend {
    // TODO: Add s3_client (rusoto or aws-sdk), bucket, prefix
}

#[async_trait::async_trait]
impl StorageBackend for S3Backend {
    async fn create(&self, name: &str, size: u64) -> Result<VolumeInfo, StorageError> {
        // TODO: Create placeholder in S3 (actual data on first write)
        unimplemented!("S3Backend::create")
    }

    async fn destroy(&self, name: &str, force: bool) -> Result<(), StorageError> {
        // TODO: Delete all objects with prefix
        unimplemented!("S3Backend::destroy")
    }

    async fn snapshot(&self, source: &str, snap_name: &str) -> Result<SnapshotInfo, StorageError> {
        // TODO: S3 copy operation for snapshot
        unimplemented!("S3Backend::snapshot")
    }

    async fn clone(&self, snap: &str, target: &str) -> Result<VolumeInfo, StorageError> {
        // TODO: S3 copy from snapshot to new prefix
        unimplemented!("S3Backend::clone")
    }

    async fn rollback(&self, snap: &str, force: bool) -> Result<(), StorageError> {
        // TODO: S3 copy snapshot back to main prefix
        unimplemented!("S3Backend::rollback")
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<VolumeInfo>, StorageError> {
        // TODO: List objects with delimiter
        unimplemented!("S3Backend::list")
    }

    async fn info(&self, name: &str) -> Result<VolumeInfo, StorageError> {
        // TODO: Head object or list to calculate size
        unimplemented!("S3Backend::info")
    }

    async fn mount(&self, name: &str, mountpoint: &std::path::PathBuf) -> Result<(), StorageError> {
        // TODO: Use s3fs-fuse or mountpoint-s3
        unimplemented!("S3Backend::mount")
    }

    async fn unmount(&self, name: &str) -> Result<(), StorageError> {
        // TODO: Fusermount -u
        unimplemented!("S3Backend::unmount")
    }

    async fn set_property(&self, name: &str, key: &str, value: &str) -> Result<(), StorageError> {
        // TODO: Set S3 object tags or metadata
        unimplemented!("S3Backend::set_property")
    }

    async fn get_property(&self, name: &str, key: &str) -> Result<String, StorageError> {
        // TODO: Get S3 object tags or metadata
        unimplemented!("S3Backend::get_property")
    }
}

/// S3 backend configuration
#[derive(Debug, Clone)]
pub struct S3Config {
    // TODO: Add endpoint, region, bucket, access_key, secret_key, prefix
}
```

**`crates/shellwego-storage/src/encryption.rs`**
```rust
//! Encryption at rest for volumes

use crate::StorageError;

/// Encryption provider
pub struct EncryptionProvider {
    // TODO: Add kms_client, master_key_id
}

impl EncryptionProvider {
    /// Create provider
    pub async fn new(config: &EncryptionConfig) -> Result<Self, StorageError> {
        // TODO: Initialize KMS client
        unimplemented!("EncryptionProvider::new")
    }

    /// Generate data encryption key
    pub async fn generate_dek(&self) -> Result<DataKey, StorageError> {
        // TODO: Generate random DEK
        // TODO: Encrypt DEK with master key
        unimplemented!("EncryptionProvider::generate_dek")
    }

    /// Decrypt data encryption key
    pub async fn decrypt_dek(&self, encrypted_dek: &[u8]) -> Result<Vec<u8>, StorageError> {
        // TODO: Call KMS to decrypt
        unimplemented!("EncryptionProvider::decrypt_dek")
    }

    /// Encrypt data block
    pub fn encrypt_block(&self, plaintext: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, StorageError> {
        // TODO: AES-256-GCM encryption
        unimplemented!("EncryptionProvider::encrypt_block")
    }

    /// Decrypt data block
    pub fn decrypt_block(&self, ciphertext: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, StorageError> {
        // TODO: AES-256-GCM decryption
        unimplemented!("EncryptionProvider::decrypt_block")
    }
}

/// Data encryption key with encrypted KEK
#[derive(Debug, Clone)]
pub struct DataKey {
    // TODO: Add plaintext (only in memory), ciphertext, master_key_id
}

/// Encryption configuration
#[derive(Debug, Clone)]
pub struct EncryptionConfig {
    // TODO: Add kms_provider, master_key_id, algorithm
}
```
User: Done?
Kimi: No, Part 5/6 is done. Sending Part 6/6 now (final part).

---

**`crates/shellwego-agent/src/metrics.rs`**
```rust
//! Agent-local metrics collection and export

use std::collections::HashMap;

/// Agent metrics collector
pub struct MetricsCollector {
    // TODO: Add registry, node_id, control_plane_client
}

impl MetricsCollector {
    /// Create collector
    pub fn new(node_id: uuid::Uuid) -> Self {
        // TODO: Initialize with node identification
        unimplemented!("MetricsCollector::new")
    }

    /// Record microVM spawn duration
    pub fn record_spawn(&self, duration_ms: u64, success: bool) {
        // TODO: Increment counter with labels
        unimplemented!("MetricsCollector::record_spawn")
    }

    /// Update resource gauges
    pub async fn update_resources(&self) {
        // TODO: Read /proc/meminfo, /proc/stat
        // TODO: Read ZFS pool usage
        // TODO: Update gauges
        unimplemented!("MetricsCollector::update_resources")
    }

    /// Export metrics to control plane
    pub async fn export(&self) -> Result<(), MetricsError> {
        // TODO: Serialize current metrics
        // TODO: POST to control plane metrics endpoint
        unimplemented!("MetricsCollector::export")
    }

    /// Start background collection loop
    pub async fn run_collection_loop(&self) -> Result<(), MetricsError> {
        // TODO: Periodic resource updates
        // TODO: Periodic export to control plane
        unimplemented!("MetricsCollector::run_collection_loop")
    }
}

/// Metrics error
#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("Export failed: {0}")]
    ExportFailed(String),
}

/// Node resource snapshot
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResourceSnapshot {
    // TODO: Add memory_total, memory_used, memory_available
    // TODO: Add cpu_cores, cpu_usage_percent
    // TODO: Add disk_total, disk_used
    // TODO: Add microvm_count, network_io, disk_io
}
```

**`crates/shellwego-control-plane/src/api/handlers/metrics.rs`**
```rust
//! Metrics query and export endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::state::AppState;

/// Query metrics for app
pub async fn get_app_metrics(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<uuid::Uuid>,
    Query(params): Query<MetricsQuery>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate time range
    // TODO: Query Prometheus or internal store
    // TODO: Return metrics series
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Query node metrics
pub async fn get_node_metrics(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Return node resource usage
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Receive metrics from agent
pub async fn ingest_agent_metrics(
    State(state): State<Arc<AppState>>,
    // TODO: Json body with ResourceSnapshot
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate agent authentication
    // TODO: Store in time-series DB
    // TODO: Check thresholds, alert if needed
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Prometheus remote write endpoint
pub async fn remote_write(
    State(state): State<Arc<AppState>>,
    // TODO: Protobuf body
) -> Result<StatusCode, StatusCode> {
    // TODO: Parse Prometheus remote write format
    // TODO: Store samples
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Metrics query parameters
#[derive(Debug, serde::Deserialize)]
pub struct MetricsQuery {
    // TODO: Add start, end, step, metric_name
}
```

**`crates/shellwego-control-plane/src/api/handlers/events.rs`**
```rust
//! Server-sent events and WebSocket real-time updates

use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::Response,
};
use std::sync::Arc;

use crate::state::AppState;

/// SSE stream for app events
pub async fn app_events_sse(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<uuid::Uuid>,
) -> Result<Response, StatusCode> {
    // TODO: Validate access to app
    // TODO: Create SSE stream
    // TODO: Subscribe to app events from NATS
    // TODO: Stream events as they arrive
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// WebSocket for interactive exec
pub async fn exec_websocket(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<uuid::Uuid>,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    // TODO: Upgrade to WebSocket
    // TODO: Proxy to agent via NATS
    // TODO: Handle stdin/stdout/stderr
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// WebSocket for log streaming
pub async fn logs_websocket(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<uuid::Uuid>,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    // TODO: Upgrade to WebSocket
    // TODO: Stream historical logs first
    // TODO: Subscribe to new logs
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// WebSocket for build logs
pub async fn build_logs_websocket(
    State(state): State<Arc<AppState>>,
    Path(build_id): Path<uuid::Uuid>,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    // TODO: Stream build log output
    Err(StatusCode::NOT_IMPLEMENTED)
}
```

**`crates/shellwego-control-plane/src/services/audit.rs`**
```rust
//! Audit logging for compliance

use shellwego_core::entities::organization::Organization;

/// Audit log service
pub struct AuditService {
    // TODO: Add storage backend (append-only log)
}

impl AuditService {
    /// Create service
    pub async fn new(config: &AuditConfig) -> Self {
        // TODO: Initialize storage
        unimplemented!("AuditService::new")
    }

    /// Log an action
    pub async fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
        // TODO: Validate entry
        // TODO: Add timestamp if missing
        // TODO: Append to immutable log
        // TODO: Optionally forward to SIEM
        unimplemented!("AuditService::log")
    }

    /// Query audit log
    pub async fn query(
        &self,
        org_id: Option<uuid::Uuid>,
        resource_type: Option<&str>,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<Vec<AuditEntry>, AuditError> {
        // TODO: Query storage with filters
        // TODO: Return paginated results
        unimplemented!("AuditService::query")
    }

    /// Export audit log for compliance
    pub async fn export(
        &self,
        org_id: uuid::Uuid,
        format: ExportFormat,
    ) -> Result<Vec<u8>, AuditError> {
        // TODO: Query all entries for org
        // TODO: Serialize as CSV or JSON
        unimplemented!("AuditService::export")
    }

    /// Run integrity verification
    pub async fn verify_integrity(&self) -> Result<bool, AuditError> {
        // TODO: Verify hash chain if using blockchain/Merkle tree
        unimplemented!("AuditService::verify_integrity")
    }
}

/// Audit log entry
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditEntry {
    // TODO: Add id, timestamp, org_id, actor_id, action
    // TODO: Add resource_type, resource_id, changes, ip_address, user_agent
}

/// Actions that can be audited
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum AuditAction {
    AppCreate,
    AppUpdate,
    AppDelete,
    AppDeploy,
    AppScale,
    LoginSuccess,
    LoginFailure,
    ApiKeyCreate,
    ApiKeyRevoke,
    // TODO: Add more actions
}

/// Export format
#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Json,
    Csv,
}

/// Audit configuration
#[derive(Debug, Clone)]
pub struct AuditConfig {
    // TODO: Add storage_path, retention_days, siem_endpoint
}

/// Audit error
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("Storage error: {0}")]
    StorageError(String),
}
```

**`crates/shellwego-control-plane/src/services/rate_limiter.rs`**
```rust
//! Distributed rate limiting

/// Rate limiter service
pub struct RateLimiter {
    // TODO: Add redis_client or in-memory store
}

impl RateLimiter {
    /// Create limiter
    pub async fn new(config: &RateLimitConfig) -> Self {
        // TODO: Initialize backend
        unimplemented!("RateLimiter::new")
    }

    /// Check if request is allowed
    pub async fn check(
        &self,
        key: &str,           // IP or API key
        resource: &str,      // Endpoint or action
        limit: u32,          // Max requests
        window_secs: u64,    // Time window
    ) -> Result<RateLimitStatus, RateLimitError> {
        // TODO: Increment counter in Redis with expiry
        // TODO: Check if over limit
        // TODO: Return status with remaining quota
        unimplemented!("RateLimiter::check")
    }

    /// Get current quota for key
    pub async fn quota(&self, key: &str, resource: &str) -> Result<QuotaInfo, RateLimitError> {
        // TODO: Read current counter
        unimplemented!("RateLimiter::quota")
    }

    /// Reset limit for key (admin action)
    pub async fn reset(&self, key: &str, resource: &str) -> Result<(), RateLimitError> {
        // TODO: Delete counter
        unimplemented!("RateLimiter::reset")
    }
}

/// Rate limit check result
#[derive(Debug, Clone)]
pub struct RateLimitStatus {
    // TODO: Add allowed, remaining, reset_time, retry_after
}

/// Quota information
#[derive(Debug, Clone)]
pub struct QuotaInfo {
    // TODO: Add limit, used, remaining, window_start
}

/// Rate limit configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    // TODO: Add redis_url, default_limits, burst_allowance
}

/// Rate limit error
#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error("Backend error: {0}")]
    BackendError(String),
}
```

**`crates/shellwego-core/src/entities/webhook.rs`**
```rust
//! Webhook subscription and delivery entities

use crate::prelude::*;

/// Webhook subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id: uuid::Uuid,
    pub org_id: uuid::Uuid,
    pub url: String,
    pub secret: String, // For HMAC signature
    pub events: Vec<String>, // e.g., ["app.deployed", "app.crashed"]
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub last_delivered_at: Option<DateTime<Utc>>,
    pub failure_count: u32,
}

/// Webhook delivery attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: uuid::Uuid,
    pub webhook_id: uuid::Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub status_code: Option<u16>,
    pub response_body: Option<String>,
    pub delivered_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub success: bool,
}

/// Webhook event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebhookEventType {
    AppCreated,
    AppDeployed,
    AppCrashed,
    AppScaled,
    BuildCompleted,
    BuildFailed,
    // TODO: Add more event types
}
```

**`crates/shellwego-core/src/entities/metrics.rs`**
```rust
//! Time-series metrics entities

use crate::prelude::*;

/// Metric sample
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSample {
    pub timestamp: DateTime<Utc>,
    pub name: String,
    pub value: f64,
    pub labels: HashMap<String, String>,
}

/// Metric series metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSeries {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub retention_days: u32,
}

/// Alert rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: uuid::Uuid,
    pub org_id: uuid::Uuid,
    pub name: String,
    pub query: String, // PromQL or similar
    pub condition: AlertCondition,
    pub duration_secs: u64, // For how long condition must be true
    pub severity: AlertSeverity,
    pub notification_channels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertCondition {
    pub comparison: ComparisonOp,
    pub threshold: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparisonOp {
    Gt, // >
    Lt, // <
    Eq, // ==
    Ne, // !=
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Warning,
    Critical,
}
```

**`crates/shellwego-core/src/entities/audit.rs`**
```rust
//! Audit log entities

use crate::prelude::*;

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: uuid::Uuid,
    pub timestamp: DateTime<Utc>,
    pub org_id: Option<uuid::Uuid>,
    pub actor_id: uuid::Uuid,
    pub actor_type: ActorType,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub changes: Option<serde_json::Value>, // Before/after
    pub metadata: AuditMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorType {
    User,
    ApiKey,
    System,
    Webhook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditMetadata {
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
}
```

**`crates/shellwego-cli/src/shell.rs`**
```rust
//! Interactive shell (REPL) for ShellWeGo

use rustyline::{Editor, error::ReadlineError};

/// Start interactive shell
pub async fn run(config: &crate::CliConfig) -> anyhow::Result<()> {
    println!("ShellWeGo Interactive Shell");
    println!("Type 'help' for commands, 'exit' to quit");
    
    let mut rl = Editor::<()>::new()?;
    
    loop {
        let readline = rl.readline("shellwego> ");
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str());
                
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.is_empty() {
                    continue;
                }
                
                match parts[0] {
                    "exit" | "quit" => break,
                    "help" => print_help(),
                    "apps" => println!("Use: apps list, apps create, etc."),
                    // TODO: Dispatch to command handlers
                    _ => println!("Unknown command: {}", parts[0]),
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    
    Ok(())
}

fn print_help() {
    println!("Available commands:");
    println!("  apps        Manage applications");
    println!("  nodes       Manage worker nodes");
    println!("  volumes     Manage persistent volumes");
    println!("  domains     Manage custom domains");
    println!("  status      Show system status");
    println!("  exit        Exit shell");
}
```

**`crates/shellwego-cli/src/completion.rs`**
```rust
//! Shell completion generators

/// Generate shell completion script
pub fn generate(shell: &str) -> String {
    match shell {
        "bash" => generate_bash(),
        "zsh" => generate_zsh(),
        "fish" => generate_fish(),
        "powershell" => generate_powershell(),
        _ => panic!("Unknown shell: {}", shell),
    }
}

fn generate_bash() -> String {
    // TODO: Generate bash completion script using clap_complete
    r#"
_shellwego_completions() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    opts="apps nodes volumes domains status help"
    
    if [[ ${cur} == -* ]]; then
        COMPREPLY=( $(compgen -W "--help --version" -- ${cur}) )
        return 0
    fi
    
    COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
    return 0
}
complete -F _shellwego_completions shellwego
"#.to_string()
}

fn generate_zsh() -> String {
    // TODO: Generate zsh completion
    "#compdef shellwego\n# TODO: ZSH completion".to_string()
}

fn generate_fish() -> String {
    // TODO: Generate fish completion
    "complete -c shellwego -f".to_string()
}

fn generate_powershell() -> String {
    // TODO: Generate PowerShell completion
    "# TODO: PowerShell completion".to_string()
}
```

**`crates/shellwego-agent/src/discovery.rs`**
```rust
//! Service discovery for inter-app communication

use std::collections::HashMap;

/// Service registry client
pub struct ServiceDiscovery {
    // TODO: Add control_plane_client, local_cache
}

impl ServiceDiscovery {
    /// Create discovery client
    pub async fn new() -> anyhow::Result<Self> {
        // TODO: Initialize with control plane connection
        unimplemented!("ServiceDiscovery::new")
    }

    /// Register local service instance
    pub async fn register(
        &self,
        service_name: &str,
        instance_id: &str,
        address: &str,
        port: u16,
        metadata: &HashMap<String, String>,
    ) -> anyhow::Result<()> {
        // TODO: POST to control plane registry
        // TODO: Start heartbeat
        unimplemented!("ServiceDiscovery::register")
    }

    /// Deregister instance
    pub async fn deregister(&self, service_name: &str, instance_id: &str) -> anyhow::Result<()> {
        // TODO: DELETE from registry
        unimplemented!("ServiceDiscovery::deregister")
    }

    /// Discover healthy instances
    pub async fn discover(&self, service_name: &str) -> anyhow::Result<Vec<Instance>> {
        // TODO: Query control plane
        // TODO: Return cached or fresh results
        unimplemented!("ServiceDiscovery::discover")
    }

    /// Watch for changes
    pub async fn watch(
        &self,
        service_name: &str,
        callback: Box<dyn Fn(Vec<Instance>) + Send>,
    ) -> anyhow::Result<WatchHandle> {
        // TODO: Subscribe to NATS for updates
        // TODO: Call callback on changes
        unimplemented!("ServiceDiscovery::watch")
    }

    /// Resolve DNS SRV record style
    pub async fn resolve_srv(&self, service_name: &str) -> anyhow::Result<Vec<(String, u16)>> {
        // TODO: Return host:port pairs
        unimplemented!("ServiceDiscovery::resolve_srv")
    }
}

/// Service instance
#[derive(Debug, Clone)]
pub struct Instance {
    // TODO: Add id, service_name, address, port, metadata, health_status
}

/// Watch handle
pub struct WatchHandle {
    // TODO: Add cancellation token
}

impl WatchHandle {
    /// Stop watching
    pub async fn stop(self) {
        // TODO: Unsubscribe
    }
}
```

**`crates/shellwego-control-plane/src/api/handlers/discovery.rs`**
```rust
//! Service discovery endpoints

use axum::{
    extract::{Path, State, Query},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use std::collections::HashMap;

use crate::state::AppState;

/// Register service instance
pub async fn register(
    State(state): State<Arc<AppState>>,
    // TODO: Json body with service details
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate instance belongs to authenticated app
    // TODO: Store in registry
    // TODO: Broadcast to watchers
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Deregister instance
pub async fn deregister(
    State(state): State<Arc<AppState>>,
    Path((service_name, instance_id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Remove from registry
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Discover instances
pub async fn discover(
    State(state): State<Arc<AppState>>,
    Path(service_name): Path<String>,
    Query(params): Query<DiscoveryQuery>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Query registry for healthy instances
    // TODO: Filter by metadata if requested
    // TODO: Return weighted list
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Health check callback from instance
pub async fn health_check(
    State(state): State<Arc<AppState>>,
    Path((service_name, instance_id)): Path<(String, String)>,
    // TODO: Json body with health status
) -> Result<StatusCode, StatusCode> {
    // TODO: Update instance health timestamp
    // TODO: Mark unhealthy if missed checks
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Watch for changes (SSE)
pub async fn watch(
    State(state): State<Arc<AppState>>,
    Path(service_name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Create SSE stream
    // TODO: Send current state
    // TODO: Push updates as they happen
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Query parameters
#[derive(Debug, serde::Deserialize)]
pub struct DiscoveryQuery {
    // TODO: Add metadata filters, health_only
}
```

**`crates/shellwego-control-plane/src/services/discovery.rs`**
```rust
//! Service registry implementation

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory service registry (with persistence)
pub struct ServiceRegistry {
    // TODO: Add services HashMap, persistence, event_bus
}

impl ServiceRegistry {
    /// Create registry
    pub async fn new() -> Self {
        // TODO: Load from database
        // TODO: Start cleanup task for expired instances
        unimplemented!("ServiceRegistry::new")
    }

    /// Register instance
    pub async fn register(&self, instance: ServiceInstance) -> Result<(), RegistryError> {
        // TODO: Validate
        // TODO: Store in memory and DB
        // TODO: Publish event
        unimplemented!("ServiceRegistry::register")
    }

    /// Deregister instance
    pub async fn deregister(&self, service_name: &str, instance_id: &str) -> Result<(), RegistryError> {
        // TODO: Remove from memory and DB
        // TODO: Publish event
        unimplemented!("ServiceRegistry::deregister")
    }

    /// Get healthy instances
    pub async fn get_healthy(&self, service_name: &str) -> Vec<ServiceInstance> {
        // TODO: Filter by health status and expiry
        unimplemented!("ServiceRegistry::get_healthy")
    }

    /// Update health status
    pub async fn update_health(&self, service_name: &str, instance_id: &str, healthy: bool) {
        // TODO: Update timestamp
        // TODO: Mark unhealthy if needed
        unimplemented!("ServiceRegistry::update_health")
    }

    /// Cleanup expired instances
    pub async fn cleanup(&self) -> usize {
        // TODO: Remove instances with missed heartbeats
        unimplemented!("ServiceRegistry::cleanup")
    }
}

/// Service instance record
#[derive(Debug, Clone)]
pub struct ServiceInstance {
    // TODO: Add id, service_name, app_id, node_id, address, port
    // TODO: Add metadata, registered_at, last_heartbeat, healthy
}

/// Registry error
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Instance already exists: {0}")]
    AlreadyExists(String),
    
    #[error("Instance not found: {0}")]
    NotFound(String),
}
```

**`Cargo.toml`** (workspace additions)
```toml
# Add to workspace members:
# "crates/shellwego-registry",
# "crates/shellwego-observability",
# "crates/shellwego-edge",
# "crates/shellwego-billing",

# Add to workspace dependencies:
wasmtime = { version = "12.0", optional = true }
aya = { version = "0.11", optional = true }
prometheus = "0.13"
opentelemetry = "0.21"
rustls = "0.22"
```

**`docker-compose.yml`** (development stack)
```yaml
version: "3.8"

services:
  # Control plane
  control-plane:
    build:
      context: .
      dockerfile: docker/control-plane.Dockerfile
    ports:
      - "8080:8080"
    environment:
      - DATABASE_URL=postgres://shellwego:shellwego@postgres:5432/shellwego
      - NATS_URL=nats://nats:4222
      - RUST_LOG=info
    depends_on:
      - postgres
      - nats

  # Agent (for local testing, requires privileged)
  agent:
    build:
      context: .
      dockerfile: docker/agent.Dockerfile
    privileged: true
    environment:
      - CONTROL_PLANE_URL=http://control-plane:8080
      - NODE_REGION=local
      - RUST_LOG=info
    volumes:
      - /dev/kvm:/dev/kvm
      - agent-data:/var/lib/shellwego
    depends_on:
      - control-plane

  # Supporting services
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: shellwego
      POSTGRES_PASSWORD: shellwego
      POSTGRES_DB: shellwego
    volumes:
      - postgres-data:/var/lib/postgresql/data

  nats:
    image: nats:2.10-alpine
    ports:
      - "4222:4222"
    command: "--js"  # Enable JetStream

  # Observability
  prometheus:
    image: prom/prometheus:latest
    volumes:
      - ./config/prometheus.yml:/etc/prometheus/prometheus.yml
    ports:
      - "9090:9090"

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin

volumes:
  postgres-data:
  agent-data:
```

**`Makefile`** (convenience targets)
```makefile
.PHONY: all build test lint fmt clean dev

# Default target
all: build

# Build all crates
build:
	cargo build --release

# Build with all features
build-all:
	cargo build --release --all-features

# Run tests
test:
	cargo test --all

# Run integration tests (requires KVM)
test-integration:
	cargo test --features integration-tests -- --test-threads=1

# Lint
lint:
	cargo clippy --all -- -D warnings

# Format
fmt:
	cargo fmt --all

# Clean
clean:
	cargo clean

# Development environment
dev:
	docker-compose up -d

# Stop dev environment
dev-stop:
	docker-compose down

# Run control plane locally
run-control-plane:
	cargo run --bin shellwego-control-plane

# Run agent locally (requires root for KVM)
run-agent:
	sudo cargo run --bin shellwego-agent

# Generate documentation
docs:
	cargo doc --all --no-deps --open

# Install CLI locally
install-cli:
	cargo install --path crates/shellwego-cli

# Database migrations
migrate:
	sqlx migrate run --source crates/shellwego-control-plane/migrations

# Create new migration
migrate-new:
	sqlx migrate add -s crates/shellwego-control-plane/migrations $(name)

# Security audit
audit:
	cargo audit

# Update dependencies
update:
	cargo update

# Check for outdated dependencies
outdated:
	cargo outdated

# Release build for all targets
release:
	cargo build --release --target x86_64-unknown-linux-musl
	cargo build --release --target aarch64-unknown-linux-musl
```

---

**All 6 parts complete!** The codebase now has comprehensive scaffolding covering:

✅ **Core Platform**: VMM, agent, control plane, CLI, networking, storage  
✅ **Registry**: Image caching and pull operations  
✅ **eBPF/XDP**: High-performance networking (firewall, QoS)  
✅ **KMS**: Encryption key management  
✅ **Observability**: Metrics, logs, tracing  
✅ **Edge Proxy**: Load balancer and TLS termination  
✅ **WASM Runtime**: Lightweight alternative to Firecracker  
✅ **Database Operators**: Postgres, MySQL, Redis automation  
✅ **Federation**: Multi-region gossip protocol  
✅ **Billing**: Usage metering and payments  
✅ **Git Integration**: Push-to-deploy webhooks and builds  
✅ **Service Discovery**: Inter-app communication  
✅ **Audit Logging**: Compliance and security  
✅ **Rate Limiting**: API protection  
✅ **Snapshots & Migration**: VM state management  
✅ **Additional CLI Commands**: Build, config, compose, SSH, tunnel  
✅ **Shell Completion**: Bash, zsh, fish, PowerShell  
✅ **Interactive Shell**: REPL mode  
✅ **Docker Compose**: Development stack  
✅ **Makefile**: Common development tasks
