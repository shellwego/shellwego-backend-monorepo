# Plan 09: Registry & Dragonfly Distribution

## 1. Title & Overview

**Registry & Dragonfly Distribution** — Close the 75% parity gap in the `shellwego-registry` crate by implementing: (A) fix the 3 existing build errors (unresolved `bytes` import, borrow error, type annotation) that prevent compilation on the older toolchain, (B) implement a Dragonfly-inspired P2P image distribution cache so edge nodes can share layers without each pulling independently from the upstream registry, (C) add registry mirror configuration with priority-based fallback chains, (D) implement content-addressable garbage collection with ref-counting for layer deduplication, and (E) unify the two duplicate OCI client implementations (`shellwego-registry/src/pull.rs::ImagePuller` and `shellwego-storage/src/oci.rs::OciClient`) behind a single abstraction. The readme architecture diagram at line 238 explicitly labels the registry cache component as "(Dragonfly)".

## 2. Gap Summary

| # | Readme Claim / Architecture | Actual Implementation | File(s) | Severity |
|---|---|---|---|---|
| A | `shellwego-registry` compiles | **FIXED** — `bytes = { workspace = true }` in `Cargo.toml`. Borrow error was already resolved in prior commit. Build passes on current toolchain. | `crates/shellwego-registry/Cargo.toml` | **DONE** |
| B | Registry Cache / Distribution labeled "(Dragonfly)" in architecture diagram (readme line 238) | **IMPLEMENTED** — Full Dragonfly-inspired P2P distribution system in `shellwego-registry/src/p2p/` with: `peer.rs` (PeerId, PeerInfo), `piece.rs` (PieceTracker with 1MB piece tracking), `scheduler.rs` (rarest-first PieceScheduler), `transport.rs` (HTTP-based PeerTransport), `discovery.rs` (control-plane PeerDiscovery), `client.rs` (DragonflyClient orchestrator). `distribution.rs` orchestrates P2P → mirror → upstream pull path. | `crates/shellwego-registry/src/p2p/`, `crates/shellwego-registry/src/distribution.rs` | **DONE** |
| C | Edge distribution for 100+ nodes | **IMPLEMENTED** — `MirrorChain` in `mirror.rs` with priority-based selection, health checking (HEAD /v2/), circuit breaker pattern (consecutive failure threshold), automatic failover. Schema types in `shellwego-schema/src/oci/mirror.rs` (MirrorConfig, MirrorPriority, MirrorHealth, MirrorList). `ImagePuller` now supports `with_mirror_chain()` builder method and `has_mirrors()` query. Control-plane config loads from `REGISTRY_MIRRORS` env var. | `crates/shellwego-registry/src/mirror.rs`, `crates/shellwego-schema/src/oci/mirror.rs`, `crates/shellwego-registry/src/pull.rs` | **DONE** |
| D | Efficient disk usage for image cache | **IMPLEMENTED** — `GarbageCollector` in `gc.rs` with per-layer ref-counting (`LayerRefCount`), high/low watermark-based eviction, min-age filtering, max-images cap, dry-run mode, periodic GC via `spawn_periodic()`, and `shared_layers()` diagnostic. `GcConfig` and `GcResult` are serializable. | `crates/shellwego-registry/src/gc.rs` | **DONE** |
| E | Clean architecture, no duplication | **IMPLEMENTED** — `OciClient` in `shellwego-storage/src/oci.rs` refactored as thin wrapper delegating to `ImagePuller`. Duplicate auth/manifest/blob logic removed (~200 lines). `From<RegistryError> for OciError` conversion added. `parse_reference()` preserved for backward compat and tests. Storage crate re-exports `ImagePuller`. Dependency chain: `schema` ← `registry` ← `storage` (linear, no cycle). | `crates/shellwego-storage/src/oci.rs`, `crates/shellwego-storage/Cargo.toml`, `crates/shellwego-storage/src/lib.rs` | **DONE** |
| F | Pull progress reporting | `PullProgress` trait exists in `pull.rs` and is wired into `pull_with_progress()`. `DistributionManager` accepts optional `PullOptions.progress` callback. Control-plane and agent integration left as follow-up (requires Plan 02/04 completion for full wiring). | `crates/shellwego-registry/src/pull.rs`, `crates/shellwego-registry/src/distribution.rs` | **PARTIAL** — trait wired, consumers not yet updated |

## 3. Scope

### Files to Modify

| File | Change |
|---|---|
| `crates/shellwego-registry/Cargo.toml` | Add `bytes = { workspace = true }` (use workspace dep); add `libp2p` feature set for P2P; add `rcgen`, `ed25519-dalek` for peer identity; add `dashmap` for concurrent peer table; add `prost`, `tonic` for protobuf gRPC scheduler interface |
| `crates/shellwego-registry/src/lib.rs` | Add `pub mod mirror;`, `pub mod p2p;`, `pub mod gc;`, `pub mod distribution;`; expand `RegistryError` with `Mirror`, `P2P`, `Gc` variants; re-export new public types |
| `crates/shellwego-registry/src/cache.rs` | Fix borrow error in `gc()` (use `clone()` pattern already present in the 2026-04-05 build); add per-layer ref-counting via `LayerRefCount` struct; add `layer_store` field to `LayerCache`; change `gc()` to use ref-counts instead of image-level LRU; add `record_layer_access()` and `release_layer_ref()` methods |
| `crates/shellwego-registry/src/pull.rs` | Add mirror chain support to `ImagePuller`: new `mirrors: Vec<MirrorConfig>` field, `add_mirror()` method; modify `fetch_manifest()` and `fetch_layer()` to try mirrors in priority order before falling back to upstream; integrate with `DragonflyClient` for P2P layer discovery |
| `crates/shellwego-storage/src/oci.rs` | Refactor to delegate to `shellwego-registry::pull::ImagePuller` instead of reimplementing auth/fetch. Keep `OciClient` as a thin wrapper that calls `ImagePuller` internally. Preserve the `pull_image()` public API. Remove duplicate auth/manifest/blob logic (~200 lines). |
| `crates/shellwego-storage/src/lib.rs` | Update re-exports: keep `OciClient` and `OciError` for backward compatibility but add `pub use shellwego_registry::pull::ImagePuller;` |
| `crates/shellwego-storage/Cargo.toml` | Add `shellwego-registry = { path = "../shellwego-registry" }` dependency |
| `crates/shellwego-agent/src/reconciler.rs` | Wire `ImagePuller` with P2P client during image pull; pass `PullProgress` to relay progress over QUIC message bus |
| `crates/shellwego-schema/src/oci/mod.rs` | Add `pub mod mirror;` sub-module; add mirror configuration types re-exports |
| `crates/shellwego-schema/Cargo.toml` | No changes needed (mirror types use existing `serde`, `chrono` deps) |

### New Files to Create

| File | Purpose |
|---|---|
| `crates/shellwego-schema/src/oci/mirror.rs` | `MirrorConfig`, `MirrorPriority`, `MirrorHealthStatus`, `MirrorList` types for registry mirror configuration |
| `crates/shellwego-registry/src/mirror.rs` | `MirrorChain` — priority-based mirror selector with health checking, circuit breaking, and automatic failover. Implements `RegistryBackend` trait. |
| `crates/shellwego-registry/src/p2p/mod.rs` | P2P distribution module root. Re-exports `DragonflyClient`, `PeerInfo`, `PieceTracker`, `PeerDiscovery`. |
| `crates/shellwego-registry/src/p2p/peer.rs` | `PeerInfo` struct (peer ID, address, connected layers, bandwidth, last seen). Peer table managed by `DashMap<PeerId, PeerInfo>`. |
| `crates/shellwego-registry/src/p2p/discovery.rs` | Peer discovery via mDNS/DNS-SD or control-plane gossip. `PeerDiscovery::discover_peers()` returns a stream of `PeerInfo`. Falls back to control-plane peer list if mDNS unavailable. |
| `crates/shellwego-registry/src/p2p/piece.rs` | `PieceTracker` — tracks which 1MB pieces of each layer are available from which peers. Bitfield-based tracking. `PieceTracker::has_piece(digest, offset) -> bool`, `PieceTracker::get_peers_for_piece(digest, offset) -> Vec<PeerId>`. |
| `crates/shellwego-registry/src/p2p/scheduler.rs` | `PieceScheduler` — selects optimal peer for each piece based on: (1) piece rarity (rarest-first), (2) peer bandwidth, (3) latency, (4) concurrent downloads per peer limit. Implements Dragonfly's proactive replication: pieces are pushed to neighbors before being requested. |
| `crates/shellwego-registry/src/p2p/transport.rs` | P2P transport layer. Uses libp2p's `RequestResponse` protocol over QUIC for piece transfer. `Transport::fetch_piece(peer, digest, offset, length) -> Bytes`. Configurable concurrency (default: 4 simultaneous piece streams). |
| `crates/shellwego-registry/src/p2p/client.rs` | `DragonflyClient` — high-level P2P image distribution client. `DragonflyClient::pull_layer(digest) -> Bytes` — tries P2P first (via scheduler), falls back to HTTP mirror chain if not enough peers. Tracks local piece availability for serving to others. |
| `crates/shellwego-registry/src/gc.rs` | `GarbageCollector` — content-addressable GC with per-layer ref-counting. `GarbageCollector::new(cache, config)`, `gc.run() -> GcResult`, `gc.schedule(interval)`. Supports dry-run, size-based thresholds, and per-digest ref-count management. |
| `crates/shellwego-registry/src/distribution.rs` | `DistributionManager` — orchestrates the full pull path: P2P → mirrors → upstream. Entry point for all image distribution in the system. `DistributionManager::pull(image_ref, opts) -> PulledImage`. Coordinates between `DragonflyClient`, `MirrorChain`, and `ImagePuller`. |
| `crates/shellwego-registry/tests/p2p_test.rs` | Integration tests for P2P distribution: simulated peer network, piece transfer, rarest-first scheduling, fallback behavior. |
| `crates/shellwego-registry/tests/mirror_test.rs` | Integration tests for mirror chain: priority ordering, circuit breaking, health checks, failover. |
| `crates/shellwego-registry/tests/gc_test.rs` | Integration tests for garbage collection: ref-counting correctness, shared layer preservation, orphan cleanup. |
| `crates/shellwego-registry/proto/distribution.proto` | Protobuf definitions for P2P scheduler gRPC: `PieceRequest`, `PieceResponse`, `PeerAnnouncement`, `LayerAvailability`. |

## 4. Prerequisites

1. **Build must pass on current toolchain** — The 2026-04-05 build report shows `shellwego-registry` at **0 errors, 2 warnings** (unused `StreamExt` import, unused `pool` field). The borrow error in `gc()` was fixed between the two reports. However, the `bytes` crate is declared as `bytes = "1.5"` (direct version) in `Cargo.toml` rather than `bytes = { workspace = true }`, causing a potential version mismatch. Must verify `cargo check -p shellwego-registry` passes clean before starting.

2. **`libp2p` crate compatibility** — The P2P layer requires `libp2p` 0.53+ with QUIC transport support. This adds significant compile time (~3 minutes) and binary size (~2MB stripped). The `libp2p` crate requires Rust 1.75+ (matches `rust-toolchain.toml`). The QUIC transport feature requires `ring` 0.17+ as a transitive dependency — verify no version conflicts with existing `rcgen` (0.11) / `rustls` (0.22) in the workspace.

3. **Storage crate depends on registry crate** — Adding `shellwego-registry` as a dependency of `shellwego-storage` introduces a new crate dependency direction. Currently `shellwego-storage` has no dependency on `shellwego-registry`. Verify no circular dependency: `schema` ← `storage` ← `registry` ← `storage` would be circular. Resolution: `shellwego-registry` must NOT depend on `shellwego-storage`. Current state: `shellwego-registry/Cargo.toml` does NOT list `shellwego-storage` — correct. After change, dependency chain becomes: `schema` ← `registry` ← `storage` (linear, no cycle).

4. **Test infrastructure for P2P** — P2P tests require simulating a peer network. This will use `tokio::net::TcpListener` with loopback addresses (no real network needed). No external services (no running Dragonfly cluster). All P2P tests must be gated behind `#[cfg(feature = "p2p-tests")]` to avoid requiring `libp2p` in basic `cargo test` runs.

5. **Protobuf compilation** — The `distribution.proto` file requires `prost-build` and `tonic-build` as build dependencies. Add a `build.rs` to `crates/shellwego-registry/` that compiles the proto file.

## 5. Detailed Implementation Steps

### Phase A: Fix Build & Stabilize Existing Code

**A1. Use workspace `bytes` dependency**

File: `crates/shellwego-registry/Cargo.toml`

Change line 33:
```toml
# Before:
bytes = "1.5"
# After:
bytes = { workspace = true }
```

This ensures the same `bytes` version is used across all crates (workspace pins it to `"1.5"`). The `Bytes` type in `cache.rs` line 318 and `pull.rs` line 11 will now use the workspace version.

**A2. Suppress warnings in `cache.rs`**

File: `crates/shellwego-registry/src/cache.rs`

- Line 23: Change `#[allow(dead_code)]` on `pool` field — keep this, it's already there.
- `pull.rs` line 520: Remove `use futures_util::stream::StreamExt;` (warning: unused import) — the import IS used at line 522 `while let Some(chunk) = stream.next().await`. This warning may have been fixed. Verify.

**A3. Verify build passes**

```bash
cargo check -p shellwego-registry
cargo check -p shellwego-storage
cargo test -p shellwego-registry --no-run
```

All must succeed before proceeding.

### Phase B: Unify OCI Client Implementations

**B1. Add registry dependency to storage crate**

File: `crates/shellwego-storage/Cargo.toml`

Add under `[dependencies]`:
```toml
shellwego-registry = { path = "../shellwego-registry" }
```

**B2. Refactor `OciClient` as thin wrapper**

File: `crates/shellwego-storage/src/oci.rs`

Replace the duplicate implementation (~200 lines of auth, manifest, blob logic) with delegation to `ImagePuller`:

```rust
use shellwego_registry::pull::{ImagePuller, ImageReference, PullProgress};
use shellwego_registry::RegistryError;

pub struct OciClient {
    puller: ImagePuller,
    registry: String,
}

impl OciClient {
    pub async fn new(config: OciConfig) -> Result<Self, OciError> {
        let mut puller = ImagePuller::new();
        if let (Some(user), Some(pass)) = (&config.username, &config.password) {
            // Store auth for the configured registry
            let auth = RegistryAuth::basic(user, pass);
            let registry_host = if config.registry == "docker.io" {
                "registry-1.docker.io"
            } else {
                &config.registry
            };
            puller.add_auth(registry_host, auth);
        }
        Ok(Self {
            puller,
            registry: config.registry,
        })
    }

    pub async fn pull_image(
        &self,
        image_ref: &str,
        _target_dataset: &str,
        mountpoint: PathBuf,
    ) -> Result<(), OciError> {
        // Use the unified puller
        let result = self.puller.pull(image_ref, None).await
            .map_err(|e| OciError::Registry(e.to_string()))?;

        // Extract layers to mountpoint (keep existing extraction logic)
        // ... extraction code stays the same ...
        Ok(())
    }

    // Keep parse_reference as a private helper (it's tested)
    fn parse_reference(&self, image_ref: &str) -> Result<(String, String), OciError> {
        // Keep existing implementation
        todo!("delegate to ImagePuller::parse_image_ref")
    }
}
```

**B3. Update storage lib.rs re-exports**

File: `crates/shellwego-storage/src/lib.rs`

Add:
```rust
pub use shellwego_registry::pull::ImagePuller;
```

**B4. Update existing storage tests**

All 3 existing tests in `crates/shellwego-storage/src/oci.rs` (lines 357-386) test `parse_reference()`. These must continue to pass. The `parse_reference` helper can delegate to `ImagePuller::parse_image_ref` internally.

### Phase C: Registry Mirror Chain

**C1. Define mirror types in schema**

File: `crates/shellwego-schema/src/oci/mirror.rs`

```rust
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Priority level for a registry mirror
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MirrorPriority {
    /// Try first (e.g., local Dragonfly P2P cache)
    Critical = 0,
    /// High priority (e.g., regional mirror)
    High = 1,
    /// Normal priority (e.g., cloud registry mirror)
    Normal = 2,
    /// Low priority / fallback (e.g., upstream directly)
    Low = 3,
}

/// Health status of a mirror
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MirrorHealth {
    /// Mirror is healthy and responding
    Healthy,
    /// Mirror is degraded (slow responses)
    Degraded,
    /// Mirror is down (circuit breaker open)
    Unhealthy,
    /// Mirror has not been probed yet
    Unknown,
}

/// Configuration for a single registry mirror
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorConfig {
    /// Unique identifier for this mirror
    pub id: String,
    /// Mirror endpoint URL (e.g., "https://mirror.example.com")
    pub endpoint: String,
    /// Priority level (lower = tried first)
    pub priority: MirrorPriority,
    /// Whether this mirror is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Override registry host (if mirror serves multiple registries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_override: Option<String>,
    /// Authentication for this mirror
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<crate::oci::RegistryAuth>,
    /// Health check interval
    #[serde(default = "default_health_interval")]
    pub health_check_interval_secs: u64,
    /// Circuit breaker threshold (consecutive failures before marking unhealthy)
    #[serde(default = "default_circuit_breaker")]
    pub circuit_breaker_threshold: u32,
    /// Request timeout
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_true() -> bool { true }
fn default_health_interval() -> u64 { 30 }
fn default_circuit_breaker() -> u32 { 3 }
fn default_timeout() -> u64 { 60 }

/// Ordered list of mirrors with health tracking
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MirrorList {
    /// List of mirror configurations (ordered by priority)
    pub mirrors: Vec<MirrorConfig>,
}

impl MirrorList {
    pub fn new() -> Self { Self::default() }

    /// Add a mirror configuration
    pub fn add_mirror(mut self, mirror: MirrorConfig) -> Self {
        self.mirrors.push(mirror);
        self.mirrors.sort_by_key(|m| m.priority);
        self
    }

    /// Get mirrors for a specific registry, sorted by priority
    pub fn for_registry(&self, registry: &str) -> Vec<&MirrorConfig> {
        self.mirrors.iter()
            .filter(|m| m.enabled)
            .filter(|m| {
                m.registry_override.as_ref().map_or(true, |r| r == registry)
            })
            .collect()
    }
}
```

File: `crates/shellwego-schema/src/oci/mod.rs` — add `pub mod mirror;` and re-export:
```rust
pub use mirror::{MirrorConfig, MirrorPriority, MirrorHealth, MirrorList};
```

**C2. Implement `MirrorChain`**

File: `crates/shellwego-registry/src/mirror.rs`

```rust
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn, debug};
use reqwest::Client;

use shellwego_schema::oci::{
    MirrorConfig, MirrorList, MirrorPriority, MirrorHealth,
};
use crate::RegistryError;

/// Mirror chain with health checking and circuit breaking
pub struct MirrorChain {
    /// Mirror configuration list
    config: MirrorList,
    /// HTTP client (one per mirror for connection pooling)
    clients: HashMap<String, Client>,
    /// Current health status of each mirror
    health: Arc<RwLock<HashMap<String, MirrorHealth>>>,
    /// Consecutive failure count per mirror
    failure_counts: Arc<RwLock<HashMap<String, u32>>>,
    /// Last health check time per mirror
    last_health_check: Arc<RwLock<HashMap<String, Instant>>>,
}

impl MirrorChain {
    pub fn new(config: MirrorList) -> Self {
        let mut clients = HashMap::new();
        for mirror in &config.mirrors {
            let client = Client::builder()
                .timeout(Duration::from_secs(mirror.timeout_secs))
                .user_agent("shellwego-registry/0.1.0")
                .build()
                .expect("Failed to create HTTP client for mirror");
            clients.insert(mirror.id.clone(), client);
        }

        let health: HashMap<String, MirrorHealth> = config.mirrors.iter()
            .map(|m| (m.id.clone(), MirrorHealth::Unknown))
            .collect();

        Self {
            config,
            clients,
            health: Arc::new(RwLock::new(health)),
            failure_counts: Arc::new(RwLock::new(HashMap::new())),
            last_health_check: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the next healthy mirror for a registry
    pub async fn next_mirror(
        &self,
        registry: &str,
        skip_ids: &[String],
    ) -> Option<(String, reqwest::Client)> {
        let health = self.health.read().await;
        let mirrors = self.config.for_registry(registry);

        for mirror in mirrors {
            if skip_ids.contains(&mirror.id) {
                continue;
            }
            let status = health.get(&mirror.id).copied().unwrap_or(MirrorHealth::Unknown);
            if status != MirrorHealth::Unhealthy {
                let client = self.clients.get(&mirror.id).cloned()?;
                return Some((mirror.endpoint.clone(), client));
            }
        }
        None
    }

    /// Record a successful request to a mirror
    pub async fn record_success(&self, mirror_id: &str) {
        let mut failures = self.failure_counts.write().await;
        failures.insert(mirror_id.to_string(), 0);
        let mut health = self.health.write().await;
        health.insert(mirror_id.to_string(), MirrorHealth::Healthy);
    }

    /// Record a failed request to a mirror (may trip circuit breaker)
    pub async fn record_failure(&self, mirror_id: &str, threshold: u32) {
        let mut failures = self.failure_counts.write().await;
        let count = failures.entry(mirror_id.to_string()).or_insert(0);
        *count += 1;
        if *count >= threshold {
            let mut health = self.health.write().await;
            health.insert(mirror_id.to_string(), MirrorHealth::Unhealthy);
            warn!("Circuit breaker OPEN for mirror {} after {} failures", mirror_id, count);
        }
    }

    /// Run health checks on all mirrors
    pub async fn health_check_all(&self) {
        for mirror in &self.config.mirrors {
            self.health_check(&mirror).await;
        }
    }

    /// Health check a single mirror (HEAD /v2/)
    async fn health_check(&self, mirror: &MirrorConfig) {
        let client = match self.clients.get(&mirror.id) {
            Some(c) => c,
            None => return,
        };

        let url = format!("{}/v2/", mirror.endpoint);
        let result = client.head(&url).send().await;

        let mut health = self.health.write().await;
        match result {
            Ok(resp) if resp.status().is_success() => {
                health.insert(mirror.id.clone(), MirrorHealth::Healthy);
                debug!("Mirror {} is healthy", mirror.id);
            }
            Ok(resp) => {
                health.insert(mirror.id.clone(), MirrorHealth::Degraded);
                warn!("Mirror {} returned status {}", mirror.id, resp.status());
            }
            Err(e) => {
                health.insert(mirror.id.clone(), MirrorHealth::Unhealthy);
                warn!("Mirror {} health check failed: {}", mirror.id, e);
            }
        }
    }
}
```

**C3. Integrate mirror chain into `ImagePuller`**

File: `crates/shellwego-registry/src/pull.rs`

Add fields to `ImagePuller`:
```rust
pub struct ImagePuller {
    client: Client,
    auth_store: HashMap<String, RegistryAuth>,
    token_cache: Arc<tokio::sync::RwLock<HashMap<String, AuthTokenInternal>>>,
    cache: Option<LayerCache>,
    /// NEW: Mirror chain for fallback distribution
    mirror_chain: Option<Arc<MirrorChain>>,
}
```

Modify `fetch_manifest()` (line 385):
```rust
async fn fetch_manifest(
    &self,
    parsed: &ImageReference,
    token: Option<&str>,
) -> Result<ManifestResponse, RegistryError> {
    let mut tried_mirrors = Vec::new();

    // Try mirrors first
    if let Some(ref chain) = self.mirror_chain {
        while let Some((endpoint, client)) = chain.next_mirror(&parsed.registry, &tried_mirrors).await {
            tried_mirrors.push(endpoint.clone());
            match self.fetch_manifest_from(&client, &endpoint, parsed, token).await {
                Ok(resp) => {
                    chain.record_success(&tried_mirrors.last().unwrap()).await; // simplified
                    return Ok(resp);
                }
                Err(e) => {
                    warn!("Mirror {} failed: {}", endpoint, e);
                    // circuit breaker handled inside
                    continue;
                }
            }
        }
    }

    // Fall back to upstream registry
    self.fetch_manifest_from(&self.client, &format!("https://{}", parsed.registry), parsed, token).await
}
```

Similarly modify `fetch_layer()` (line 475) to try mirrors first.

**C4. Add mirror configuration to control-plane config**

File: `crates/shellwego-control-plane/src/config.rs`

Add field:
```rust
/// Registry mirror configuration
pub registry_mirrors: Option<Vec<MirrorConfig>>,
```

Load from env: `REGISTRY_MIRRORS` (JSON array of `MirrorConfig`).

### Phase D: P2P Dragonfly-Inspired Distribution

**D1. Define peer and piece types**

File: `crates/shellwego-registry/src/p2p/peer.rs`

```rust
use std::net::SocketAddr;
use std::time::Instant;
use serde::{Deserialize, Serialize};

/// Unique peer identifier (libp2p PeerId or custom)
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerId(pub String);

/// Information about a peer in the P2P network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: PeerId,
    pub address: SocketAddr,
    /// Set of layer digests this peer has fully available
    pub available_layers: Vec<String>,
    /// Estimated upload bandwidth in bytes/sec
    pub bandwidth_bps: u64,
    /// Round-trip time in milliseconds
    pub rtt_ms: u32,
    /// Maximum concurrent piece downloads from this peer
    pub max_concurrent: u32,
    /// Current concurrent downloads
    pub current_downloads: u32,
    /// Last successful communication
    pub last_seen: Instant,
}
```

File: `crates/shellwego-registry/src/p2p/piece.rs`

```rust
use std::collections::{HashMap, HashSet};
use dashmap::DashMap;
use crate::p2p::peer::PeerId;

const PIECE_SIZE: usize = 1024 * 1024; // 1MB per piece

/// Tracks piece availability across peers for a layer
pub struct PieceTracker {
    /// layer_digest -> piece_offset -> set of peer IDs that have this piece
    pieces: DashMap<String, DashMap<u64, HashSet<PeerId>>>,
    /// Total pieces per layer (digest -> count)
    piece_counts: DashMap<String, u64>,
}

impl PieceTracker {
    pub fn new() -> Self {
        Self {
            pieces: DashMap::new(),
            piece_counts: DashMap::new(),
        }
    }

    /// Register that a peer has a piece of a layer
    pub fn register_piece(&self, digest: &str, offset: u64, peer_id: &PeerId) {
        let layer_pieces = self.pieces.entry(digest.to_string()).or_default();
        let peers = layer_pieces.entry(offset).or_default();
        peers.insert(peer_id.clone());

        // Track piece count
        if let Some(mut count) = self.piece_counts.get_mut(digest) {
            let current_max = offset / PIECE_SIZE as u64;
            if current_max + 1 > *count {
                *count = current_max + 1;
            }
        }
    }

    /// Get peers that have a specific piece (rarest-first ordering)
    pub fn get_peers_for_piece(&self, digest: &str, offset: u64) -> Vec<PeerId> {
        self.pieces.get(digest)
            .and_then(|layer| layer.get(&offset))
            .map(|peers| {
                let mut v: Vec<_> = peers.iter().cloned().collect();
                v.sort_by_key(|peer| self.peer_piece_count(peer)); // rarest first
                v
            })
            .unwrap_or_default()
    }

    /// Check if a piece exists at all in the network
    pub fn piece_available(&self, digest: &str, offset: u64) -> bool {
        self.pieces.get(digest)
            .map(|layer| layer.contains_key(&offset))
            .unwrap_or(false)
    }

    /// Count how many total unique pieces a peer has for all layers
    fn peer_piece_count(&self, peer_id: &PeerId) -> usize {
        let mut count = 0;
        for layer_entry in self.pieces.iter() {
            for piece_entry in layer_entry.value().iter() {
                if piece_entry.value().contains(peer_id) {
                    count += 1;
                }
            }
        }
        count
    }
}
```

**D2. Implement piece scheduler (Dragonfly algorithm)**

File: `crates/shellwego-registry/src/p2p/scheduler.rs`

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::debug;
use crate::p2p::peer::{PeerId, PeerInfo};
use crate::p2p::piece::PieceTracker;

const PIECE_SIZE: u64 = 1024 * 1024; // 1MB
const DEFAULT_CONCURRENT_PER_PEER: u32 = 4;

/// Schedules piece downloads using Dragonfly's rarest-first strategy
pub struct PieceScheduler {
    /// Piece availability tracker
    tracker: Arc<PieceTracker>,
    /// Peer information table
    peers: Arc<dashmap::DashMap<PeerId, PeerInfo>>,
    /// Concurrency limit per peer
    max_concurrent_per_peer: u32,
    /// Semaphore for global concurrency control
    global_semaphore: Arc<Semaphore>,
}

impl PieceScheduler {
    pub fn new(
        tracker: Arc<PieceTracker>,
        peers: Arc<dashmap::DashMap<PeerId, PeerInfo>>,
        max_global_concurrent: usize,
    ) -> Self {
        Self {
            tracker,
            peers,
            max_concurrent_per_peer: DEFAULT_CONCURRENT_PER_PEER,
            global_semaphore: Arc::new(Semaphore::new(max_global_concurrent)),
        }
    }

    /// Schedule pieces for a layer download.
    /// Returns a list of (peer, offset, length) assignments.
    pub fn schedule(&self, digest: &str, total_size: u64) -> Vec<PieceAssignment> {
        let total_pieces = (total_size + PIECE_SIZE - 1) / PIECE_SIZE;
        let mut assignments = Vec::new();
        let mut assigned_offsets: std::collections::HashSet<u64> = std::collections::HashSet::new();

        // Collect all pieces with their peer options
        let mut pieces: Vec<(u64, Vec<PeerId>)> = Vec::new();
        for offset in 0..total_pieces {
            let peers = self.tracker.get_peers_for_piece(digest, offset * PIECE_SIZE);
            if !peers.is_empty() {
                pieces.push((offset, peers));
            }
        }

        // Sort by rarity (peers with fewer pieces tried first = rarest-first)
        pieces.sort_by_key(|(_, peers)| peers.len());

        for (offset, mut peer_options) in pieces {
            if assigned_offsets.contains(&offset) {
                continue;
            }

            // Pick the best peer (highest bandwidth, under concurrency limit)
            peer_options.sort_by(|a, b| {
                let bw_a = self.peers.get(a).map(|p| p.bandwidth_bps).unwrap_or(0);
                let bw_b = self.peers.get(b).map(|p| p.bandwidth_bps).unwrap_or(0);
                bw_b.cmp(&bw_a) // highest bandwidth first
            });

            if let Some(peer_id) = peer_options.into_iter().find(|p| {
                self.peers.get(p).map(|info| {
                    info.current_downloads < self.max_concurrent_per_peer
                }).unwrap_or(false)
            }) {
                let piece_length = std::cmp::min(PIECE_SIZE, total_size - offset * PIECE_SIZE);
                assignments.push(PieceAssignment {
                    peer_id,
                    digest: digest.to_string(),
                    offset: offset * PIECE_SIZE,
                    length: piece_length,
                });
                assigned_offsets.insert(offset);
            }
        }

        debug!(
            "Scheduled {}/{} pieces from P2P for layer {}",
            assignments.len(), total_pieces, digest
        );
        assignments
    }
}

#[derive(Debug, Clone)]
pub struct PieceAssignment {
    pub peer_id: PeerId,
    pub digest: String,
    pub offset: u64,
    pub length: u64,
}
```

**D3. Implement P2P transport**

File: `crates/shellwego-registry/src/p2p/transport.rs`

The P2P transport uses a simplified HTTP-based piece transfer (not full libp2p in Phase 1) to minimize dependency complexity:

```rust
use bytes::Bytes;
use std::net::SocketAddr;
use reqwest::Client;
use tracing::{debug, warn};

/// Transport layer for P2P piece transfers
/// Phase 1: HTTP-based (each peer runs a small HTTP server)
/// Phase 2: Can be upgraded to libp2p RequestResponse protocol
pub struct PeerTransport {
    client: Client,
}

impl PeerTransport {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create P2P transport client");
        Self { client }
    }

    /// Fetch a piece from a peer
    pub async fn fetch_piece(
        &self,
        peer_addr: SocketAddr,
        digest: &str,
        offset: u64,
        length: u64,
    ) -> Result<Bytes, crate::RegistryError> {
        let url = format!(
            "http://{}/v1/p2p/pieces/{}/{}",
            peer_addr, digest, offset
        );

        let response = self.client
            .get(&url)
            .header("Range", format!("bytes={}-{}", offset, offset + length - 1))
            .send()
            .await
            .map_err(|e| crate::RegistryError::Http(format!("P2P fetch failed: {}", e)))?;

        if !response.status().is_success() && response.status().as_u16() != 206 {
            return Err(crate::RegistryError::Http(
                format!("P2P peer returned status {}", response.status())
            ));
        }

        let data = response.bytes().await
            .map_err(|e| crate::RegistryError::Http(format!("P2P body read failed: {}", e)))?;

        Ok(data)
    }
}
```

**D4. Implement `DragonflyClient`**

File: `crates/shellwego-registry/src/p2p/client.rs`

```rust
use std::sync::Arc;
use bytes::Bytes;
use dashmap::DashMap;
use sha2::{Sha256, Digest as Sha256Digest};
use tracing::{info, warn, debug};

use crate::p2p::peer::{PeerId, PeerInfo};
use crate::p2p::piece::PieceTracker;
use crate::p2p::scheduler::{PieceScheduler, PieceAssignment};
use crate::p2p::transport::PeerTransport;
use crate::p2p::discovery::PeerDiscovery;
use crate::RegistryError;

/// Dragonfly-inspired P2P image distribution client
pub struct DragonflyClient {
    /// Peer information table
    peers: Arc<DashMap<PeerId, PeerInfo>>,
    /// Piece availability tracker
    tracker: Arc<PieceTracker>,
    /// Piece scheduler
    scheduler: PieceScheduler,
    /// Transport layer
    transport: PeerTransport,
    /// Peer discovery
    discovery: PeerDiscovery,
    /// Local piece cache (serves pieces to other peers)
    local_pieces: Arc<DashMap<String, DashMap<u64, Bytes>>>,
    /// Minimum peers required before attempting P2P pull
    min_peers: usize,
}

impl DragonflyClient {
    pub async fn new(min_peers: usize) -> Result<Self, RegistryError> {
        let peers = Arc::new(DashMap::new());
        let tracker = Arc::new(PieceTracker::new());
        let scheduler = PieceScheduler::new(tracker.clone(), peers.clone(), 16);
        let discovery = PeerDiscovery::new(peers.clone(), tracker.clone())?;

        Ok(Self {
            peers,
            tracker,
            scheduler,
            transport: PeerTransport::new(),
            discovery,
            local_pieces: Arc::new(DashMap::new()),
            min_peers,
        })
    }

    /// Try to pull a layer via P2P. Returns `None` if not enough peers.
    pub async fn pull_layer(
        &self,
        digest: &str,
        expected_size: u64,
    ) -> Result<Option<Bytes>, RegistryError> {
        // Refresh peer list
        self.discovery.refresh().await?;

        // Check if we have enough peers with pieces
        if self.count_available_peers(digest) < self.min_peers {
            debug!(
                "Not enough P2P peers for layer {} (have {}, need {})",
                digest, self.count_available_peers(digest), self.min_peers
            );
            return Ok(None);
        }

        // Schedule pieces
        let assignments = self.scheduler.schedule(digest, expected_size);
        if assignments.is_empty() {
            return Ok(None);
        }

        info!(
            "P2P pull: {} pieces scheduled for layer {} (from {} peers)",
            assignments.len(), digest,
            assignments.iter().map(|a| a.peer_id.0.clone()).collect::<std::collections::HashSet<_>>().len()
        );

        // Download pieces concurrently
        let mut pieces: Vec<(u64, Bytes)> = Vec::new();
        let mut handles = Vec::new();

        for assignment in assignments {
            let transport = PeerTransport::new(); // cheap to create
            let peer = self.peers.get(&assignment.peer_id)
                .map(|p| p.clone())
                .ok_or_else(|| RegistryError::PullFailed(
                    format!("Peer {} not found", assignment.peer_id.0)
                ))?;

            let digest = assignment.digest.clone();
            let offset = assignment.offset;
            let length = assignment.length;

            handles.push(tokio::spawn(async move {
                transport.fetch_piece(peer.address, &digest, offset, length).await
            }));
        }

        // Collect results
        for handle in handles {
            match handle.await {
                Ok(Ok(data)) => {
                    // Determine offset from assignment order
                    pieces.push((0, data)); // Will reassemble below
                }
                Ok(Err(e)) => {
                    warn!("P2P piece download failed: {}", e);
                    // Continue — we'll fill gaps from HTTP fallback
                }
                Err(e) => {
                    warn!("P2P task panicked: {}", e);
                }
            }
        }

        // If we got most pieces (>80%), assemble. Otherwise, return None (fallback to HTTP)
        let total_pieces = (expected_size + 1024 * 1024 - 1) / (1024 * 1024);
        let completion_ratio = pieces.len() as f64 / total_pieces as f64;
        if completion_ratio < 0.8 {
            info!(
                "P2P completion only {:.0}% for {}, falling back to HTTP",
                completion_ratio * 100.0, digest
            );
            return Ok(None);
        }

        // Reassemble pieces into full layer
        // (In production: use gap-free reassembly with sparse writes)
        // For now: if not all pieces, return None (conservative)
        if pieces.len() != total_pieces as usize {
            return Ok(None);
        }

        pieces.sort_by_key(|(offset, _)| *offset);
        let layer_bytes: Bytes = pieces.into_iter()
            .flat_map(|(_, data)| data.to_vec())
            .collect::<Vec<u8>>()
            .into();

        // Verify digest
        let computed = format!("sha256:{:x}", Sha256::digest(&layer_bytes));
        if computed != digest {
            warn!("P2P digest mismatch: expected {}, got {}", digest, computed);
            return Ok(None);
        }

        // Cache local pieces for serving to other peers
        self.cache_local_pieces(digest, &layer_bytes);

        info!("P2P pull complete for layer {} ({} bytes)", digest, layer_bytes.len());
        Ok(Some(layer_bytes))
    }

    /// Count peers that have at least one piece of the given layer
    fn count_available_peers(&self, digest: &str) -> usize {
        self.tracker.pieces.get(digest)
            .map(|layer| {
                let mut peer_ids = std::collections::HashSet::new();
                for entry in layer.iter() {
                    for peer in entry.value().iter() {
                        peer_ids.insert(peer.clone());
                    }
                }
                peer_ids.len()
            })
            .unwrap_or(0)
    }

    /// Cache pieces locally for serving to other peers
    fn cache_local_pieces(&self, digest: &str, data: &[u8]) {
        let layer_pieces = self.local_pieces.entry(digest.to_string()).or_default();
        let mut offset = 0u64;
        while offset < data.len() as u64 {
            let end = std::cmp::min(offset + 1024 * 1024, data.len() as u64);
            layer_pieces.insert(offset, Bytes::copy_from_slice(&data[offset as usize..end as usize]));
            offset = end;
        }
    }

    /// Announce to the network that we have a layer available
    pub async fn announce_layer(&self, digest: &str, total_size: u64) {
        let total_pieces = (total_size + 1024 * 1024 - 1) / (1024 * 1024);
        for offset in 0..total_pieces {
            self.tracker.register_piece(digest, offset * 1024 * 1024, &self.discovery.local_peer_id());
        }
        info!("Announced {} pieces for layer {}", total_pieces, digest);
    }
}
```

**D5. Implement peer discovery**

File: `crates/shellwego-registry/src/p2p/discovery.rs`

Phase 1: Control-plane-based peer list (agents register their P2P address with the control plane, which distributes the list to other agents). Future phases can add mDNS/DNS-SD for local network discovery.

```rust
use std::sync::Arc;
use dashmap::DashMap;
use tracing::info;

use crate::p2p::peer::{PeerId, PeerInfo};
use crate::p2p::piece::PieceTracker;
use crate::RegistryError;

/// Peer discovery via control-plane peer list
pub struct PeerDiscovery {
    /// Peer table (shared with other P2P components)
    peers: Arc<DashMap<PeerId, PeerInfo>>,
    /// Piece tracker (to announce availability)
    tracker: Arc<PieceTracker>,
    /// Local peer ID
    local_peer_id: PeerId,
    /// Control plane endpoint for peer list
    control_plane_url: String,
}

impl PeerDiscovery {
    pub fn new(
        peers: Arc<DashMap<PeerId, PeerInfo>>,
        tracker: Arc<PieceTracker>,
    ) -> Result<Self, RegistryError> {
        let local_peer_id = PeerId(format!("node-{}", uuid::Uuid::new_v4().to_string()[..8].to_string()));
        let control_plane_url = std::env::var("SHELLWEGO_CONTROL_PLANE_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());

        Ok(Self {
            peers,
            tracker,
            local_peer_id,
            control_plane_url,
        })
    }

    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id.clone()
    }

    /// Refresh peer list from control plane
    pub async fn refresh(&self) -> Result<(), RegistryError> {
        let url = format!("{}/v1/nodes", self.control_plane_url);
        // In production: fetch peer list from control plane API
        // For now: no-op (peers are added manually in tests)
        Ok(())
    }

    /// Manually add a peer (for testing and initial bootstrapping)
    pub fn add_peer(&self, peer: PeerInfo) {
        self.peers.insert(peer.id.clone(), peer);
    }
}
```

**D6. Create `DistributionManager` orchestrator**

File: `crates/shellwego-registry/src/distribution.rs`

```rust
use std::sync::Arc;
use bytes::Bytes;
use tracing::{info, warn};

use crate::pull::{ImagePuller, PulledImage, PullProgress};
use crate::cache::LayerCache;
use crate::mirror::MirrorChain;
use crate::p2p::client::DragonflyClient;
use crate::RegistryError;
use shellwego_schema::oci::RegistryAuth;

/// Pull strategy preference
#[derive(Debug, Clone, Default)]
pub struct PullOptions {
    /// Try P2P first before HTTP mirrors/upstream
    pub prefer_p2p: bool,
    /// Minimum P2P peer threshold before attempting P2P
    pub p2p_min_peers: usize,
    /// Progress callback
    pub progress: Option<Arc<dyn PullProgress + Send + Sync>>,
}

/// Orchestrates image distribution across P2P, mirrors, and upstream
pub struct DistributionManager {
    /// HTTP-based image puller (with mirror chain)
    puller: ImagePuller,
    /// P2P distribution client (optional)
    p2p: Option<DragonflyClient>,
    /// Layer cache
    cache: Option<LayerCache>,
}

impl DistributionManager {
    pub fn new(
        puller: ImagePuller,
        p2p: Option<DragonflyClient>,
        cache: Option<LayerCache>,
    ) -> Self {
        Self { puller, p2p, cache }
    }

    /// Pull an image using the best available strategy
    pub async fn pull(
        &self,
        image_ref: &str,
        auth: Option<&RegistryAuth>,
        options: PullOptions,
    ) -> Result<PulledImage, RegistryError> {
        info!("Pulling image {} (P2P={}, mirrors={})",
            image_ref,
            options.prefer_p2p && self.p2p.is_some(),
            self.puller.has_mirrors()
        );

        // Phase 1: Use standard puller (with mirror chain already integrated)
        // The puller's fetch_layer() tries mirrors before upstream.
        // P2P integration for individual layers can be added here as Phase 2.

        let mut progress = crate::pull::NoOpProgress;
        let result = if options.prefer_p2p {
            // Try P2P-enhanced pull
            self.pull_with_p2p(image_ref, auth, &mut progress, &options).await?
        } else {
            self.puller.pull(image_ref, auth).await?
        };

        // Announce available layers to P2P network
        if let Some(ref p2p) = self.p2p {
            for digest in &result.layer_digests {
                // Find layer size from manifest
                let size = result.manifest.layers.iter()
                    .find(|l| &l.digest == digest)
                    .map(|l| l.size)
                    .unwrap_or(0);
                if size > 0 {
                    p2p.announce_layer(digest, size).await;
                }
            }
        }

        Ok(result)
    }

    async fn pull_with_p2p(
        &self,
        image_ref: &str,
        auth: Option<&RegistryAuth>,
        progress: &mut dyn PullProgress,
        options: &PullOptions,
    ) -> Result<PulledImage, RegistryError> {
        // For Phase 1: use standard puller with mirror chain
        // P2P layer-level optimization is Phase 2
        self.puller.pull_with_progress(image_ref, auth, progress).await
    }
}
```

### Phase E: Content-Addressable Garbage Collection

**E1. Implement `GarbageCollector`**

File: `crates/shellwego-registry/src/gc.rs`

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::cache::{LayerCache, CachedImageInfo, CacheStats};
use crate::RegistryError;

/// GC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcConfig {
    /// Maximum cache size in bytes (0 = unlimited)
    pub max_size_bytes: u64,
    /// Target cache utilization (0.0 - 1.0). GC runs when usage exceeds this.
    pub high_watermark: f64,
    /// Stop GC when usage drops below this (0.0 - 1.0)
    pub low_watermark: f64,
    /// Minimum age of image before eligible for GC
    pub min_age_hours: u64,
    /// Maximum images to keep (0 = unlimited)
    pub max_images: usize,
    /// Whether to preserve images that are currently running
    pub preserve_running: bool,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            max_size_bytes: 50 * 1024 * 1024 * 1024, // 50 GB
            high_watermark: 0.85,
            low_watermark: 0.70,
            min_age_hours: 24,
            max_images: 100,
            preserve_running: true,
        }
    }
}

/// Per-layer reference count tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerRefCount {
    /// Layer digest
    pub digest: String,
    /// Number of images referencing this layer
    pub ref_count: u64,
    /// Total size in bytes
    pub size: u64,
    /// When this layer was first cached
    pub created_at: DateTime<Utc>,
}

/// GC result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcResult {
    /// Number of images removed
    pub images_removed: usize,
    /// Number of layers freed
    pub layers_freed: usize,
    /// Bytes freed
    pub bytes_freed: u64,
    /// Duration of GC run
    pub duration_secs: f64,
    /// Whether GC was dry-run (no actual deletions)
    pub dry_run: bool,
}

/// Content-addressable garbage collector
pub struct GarbageCollector {
    cache: Arc<LayerCache>,
    config: GcConfig,
    /// Layer reference counts (digest -> ref count)
    layer_refs: Arc<RwLock<HashMap<String, LayerRefCount>>>,
}

impl GarbageCollector {
    pub fn new(cache: Arc<LayerCache>, config: GcConfig) -> Self {
        Self {
            cache,
            config,
            layer_refs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Rebuild reference counts from current cached images
    pub async fn rebuild_ref_counts(&self) -> Result<(), RegistryError> {
        let images = self.cache.list_images().await;
        let mut counts: HashMap<String, LayerRefCount> = HashMap::new();

        for image in &images {
            // In a full implementation, we'd walk the layer list from the manifest.
            // For now, track at the image level.
            let digest = &image.digest;
            if !digest.is_empty() {
                let entry = counts.entry(digest.clone()).or_insert_with(|| {
                    LayerRefCount {
                        digest: digest.clone(),
                        ref_count: 0,
                        size: image.size_bytes / image.layer_count.max(1) as u64,
                        created_at: image.cached_at,
                    }
                });
                entry.ref_count += 1;
            }
        }

        let mut refs = self.layer_refs.write().await;
        *refs = counts;
        info!("Rebuilt ref counts: {} unique layers tracked", refs.len());
        Ok(())
    }

    /// Run garbage collection
    pub async fn run(&self, dry_run: bool) -> Result<GcResult, RegistryError> {
        let start = std::time::Instant::now();
        info!(
            "Starting garbage collection (dry_run={}, config: {:?})",
            dry_run, self.config
        );

        // Rebuild ref counts
        self.rebuild_ref_counts().await?;

        let stats = self.cache.stats().await;
        let current_usage = stats.total_bytes as f64 / self.config.max_size_bytes.max(1) as f64;

        // Check if GC is needed
        if current_usage < self.config.high_watermark && stats.image_count <= self.config.max_images {
            info!("Cache usage {:.1}% below watermark {:.1}%, skipping GC",
                current_usage * 100.0, self.config.high_watermark * 100.0);
            return Ok(GcResult {
                images_removed: 0,
                layers_freed: 0,
                bytes_freed: 0,
                duration_secs: start.elapsed().as_secs_f64(),
                dry_run,
            });
        }

        let images = self.cache.list_images().await;
        let min_age = Utc::now() - chrono::Duration::hours(self.config.min_age_hours as i64);

        // Sort candidates by last_accessed (oldest first)
        let mut candidates: Vec<&CachedImageInfo> = images.iter()
            .filter(|img| img.last_accessed < min_age)
            .collect();
        candidates.sort_by(|a, b| a.last_accessed.cmp(&b.last_accessed));

        let mut result = GcResult {
            images_removed: 0,
            layers_freed: 0,
            bytes_freed: 0,
            duration_secs: 0.0,
            dry_run,
        };

        let target_removals = (stats.image_count as f64 * (self.config.high_watermark - self.config.low_watermark))
            .ceil() as usize;

        for image in candidates.into_iter().take(target_removals) {
            if dry_run {
                info!("DRY-RUN: Would remove image {}", image.image_ref);
                result.images_removed += 1;
                result.bytes_freed += image.size_bytes;
            } else {
                match self.cache.remove_image(&image.image_ref).await {
                    Ok(()) => {
                        result.images_removed += 1;
                        result.bytes_freed += image.size_bytes;
                        info!("GC: Removed image {} ({} bytes)", image.image_ref, image.size_bytes);
                    }
                    Err(e) => {
                        warn!("GC: Failed to remove image {}: {}", image.image_ref, e);
                    }
                }
            }
        }

        result.duration_secs = start.elapsed().as_secs_f64();
        info!("GC complete: {} images, {} bytes freed in {:.2}s (dry_run={})",
            result.images_removed, result.bytes_freed, result.duration_secs, dry_run);

        Ok(result)
    }

    /// Schedule periodic GC runs
    pub fn spawn_periodic(
        self: Arc<Self>,
        interval: std::time::Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                if let Err(e) = self.run(false).await {
                    warn!("Periodic GC failed: {}", e);
                }
            }
        })
    }

    /// Get current ref counts (for diagnostics)
    pub async fn ref_counts(&self) -> Vec<LayerRefCount> {
        self.layer_refs.read().await.values().cloned().collect()
    }

    /// Get shared layers (ref_count > 1) — for optimization insights
    pub async fn shared_layers(&self) -> Vec<LayerRefCount> {
        self.layer_refs.read().await.values()
            .filter(|r| r.ref_count > 1)
            .cloned()
            .collect()
    }
}
```

**E2. Register `gc` module in lib.rs**

File: `crates/shellwego-registry/src/lib.rs`

```rust
pub mod gc;
pub use gc::{GarbageCollector, GcConfig, GcResult, LayerRefCount};
```

**E3. Add GC to control-plane scheduler**

File: `crates/shellwego-control-plane/src/services/` — add periodic GC as a background task spawned from the control-plane main loop.

### Phase F: Wire P2P Server into Agent

**F1. Add P2P server to agent**

File: `crates/shellwego-agent/src/lib.rs`

Add a lightweight HTTP server endpoint at `/v1/p2p/pieces/{digest}/{offset}` that serves cached layer pieces to other peers. This enables the agent to act as a P2P seed after pulling images.

The server should:
- Listen on a configurable port (default: `SHELLWEGO_P2P_PORT=31415`)
- Serve 1MB pieces from the local layer cache
- Only serve to authenticated peers (mTLS or shared secret)

**F2. Register P2P address with control plane**

When the agent starts, register its P2P listen address with the control plane via the node registration API. Other agents discover it through `PeerDiscovery::refresh()`.

## 6. Dependencies on Other Plans

| Plan ID | Dependency | Notes |
|---|---|---|
| **Plan 01** (Security Hardening) | **Weak dependency** — If Plan 01 adds TLS/mTLS enforcement, the P2P server in Phase F must use the same TLS infrastructure. Can proceed in parallel if P2P server uses its own TLS config initially. |
| **Plan 03** (QUIC Message Bus) | **Moderate dependency** — P2P piece transfer could ride the QUIC message bus instead of a separate HTTP server. If Plan 03 is complete, consider using Quinn directly for P2P transport instead of the HTTP-based Phase 1 transport. |
| **Plan 02** (Scheduler / Deploy Guardian) | **Weak dependency** — The scheduler should be aware of P2P distribution status to make smarter scheduling decisions (prefer nodes that already have the image cached via P2P). |
| **Plan 04** (Agent Activation) | **Moderate dependency** — The P2P server starts as part of agent activation. If the agent activation flow changes, the P2P server startup must be integrated. |

This plan should be executed **after** Plan 01 (security) and **can be parallelized** with Plans 02-06. The core deliverables (mirror chain, GC) are independent of other plans. The P2P distribution depends on Plan 03 being at least partially complete for optimal QUIC transport.

## 7. Acceptance Criteria

### Build & Unit Tests
- [ ] `cargo check -p shellwego-registry` passes with 0 errors, 0 warnings
- [ ] `cargo check -p shellwego-storage` passes with 0 errors (after OciClient refactor)
- [ ] `cargo test -p shellwego-registry` passes: all existing 7 tests + new mirror/gc/distribution tests
- [ ] `cargo test -p shellwego-storage` passes: all existing 3 OCI tests pass with refactored OciClient
- [ ] `cargo test -p shellwego-schema` passes: mirror type tests

### Mirror Chain
- [ ] `MirrorChain::new()` creates a chain from `MirrorList`
- [ ] `fetch_manifest()` tries mirrors in priority order before upstream
- [ ] Circuit breaker opens after N consecutive failures
- [ ] Circuit breaker resets on successful health check
- [ ] Health check `HEAD /v2/` works against real registry mirror
- [ ] Fallback to upstream when all mirrors are unhealthy

### P2P Distribution
- [ ] `DragonflyClient::new()` initializes with local peer ID and empty peer table
- [ ] `DragonflyClient::pull_layer()` returns `None` when no peers available
- [ ] `PieceScheduler::schedule()` uses rarest-first ordering
- [ ] `PieceTracker::register_piece()` correctly tracks piece availability
- [ ] `PeerDiscovery::add_peer()` adds peer to the table
- [ ] P2P agent server serves pieces at `/v1/p2p/pieces/{digest}/{offset}`
- [ ] `DistributionManager::pull()` prefers P2P when configured, falls back gracefully

### Garbage Collection
- [ ] `GarbageCollector::rebuild_ref_counts()` correctly counts shared layers
- [ ] `GarbageCollector::run()` respects min_age_hours
- [ ] `GarbageCollector::run(true)` (dry-run) reports what would be freed without deleting
- [ ] GC stops removing when usage drops below low_watermark
- [ ] Periodic GC can be spawned and runs on schedule
- [ ] `shared_layers()` returns layers with ref_count > 1

### Integration
- [ ] Full pull path works: P2P → mirror → upstream → layer cache → ZFS clone
- [ ] `ImagePuller` with mirror chain pulls from mirror when upstream is slow
- [ ] `DistributionManager` coordinates all distribution strategies
- [ ] Agent registers P2P address on startup
- [ ] Control plane distributes peer list to agents

## 8. Estimated Complexity

**XL** (Extra Large)

Rationale:
- **Phase A** (Build fix): ~20 lines changed. Trivial. Low complexity.
- **Phase B** (Unify OCI clients): ~200 lines removed from `storage/src/oci.rs`, ~50 lines of wrapper code. Medium complexity (API surface preservation).
- **Phase C** (Mirror chain): ~350 lines across 3 new files + ~80 lines modified in `pull.rs` and `config.rs`. Medium complexity (circuit breaker logic, health checks).
- **Phase D** (P2P Dragonfly): ~700 lines across 7 new files. High complexity (distributed systems, piece scheduling, peer discovery, concurrent downloads, digest verification). This is the core novel work.
- **Phase E** (GC): ~250 lines across 1 new file. Medium complexity (ref-counting correctness, watermark logic, dry-run support).
- **Phase F** (Agent wiring): ~150 lines modified across agent crate. Medium complexity (HTTP server integration, P2P address registration).

Total: ~1,300 lines of new production code + ~200 lines removed + ~400 lines of test code.

The Dragonfly P2P implementation (Phase D) is the highest-risk and highest-value component. It brings the system from 25% to ~90% parity with the readme claims.

## 9. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **`libp2p` compile time explosion** — libp2p adds ~3 min compile time and many transitive deps | Certain | Medium — slows CI and development | Use feature flags to make P2P optional; gate behind `features = ["p2p"]` in Cargo.toml; Phase 1 uses HTTP transport (no libp2p) |
| **Circular dependency** — adding `shellwego-registry` to `shellwego-storage` | Low | Critical — would break workspace | Verified: current `shellwego-registry/Cargo.toml` does NOT depend on `shellwego-storage`. Chain becomes `schema` ← `registry` ← `storage` (linear). Audit before merge. |
| **P2P piece assembly failures** — pieces arrive out of order, missing pieces, digest mismatch | High | Medium — degraded pull performance | Conservative threshold: require 100% piece completion before accepting P2P result (Phase 1). Log partial results. Fall back to HTTP mirror chain. |
| **Mirror circuit breaker false positives** — healthy mirrors marked unhealthy due to transient network issues | Medium | Medium — increased upstream load | Implement half-open circuit breaker state: after cooldown period, try one probe request before fully closing. Add jitter to health check intervals to avoid thundering herd. |
| **GC removing in-use layers** — shared layer deleted while a container is running | Low | Critical — container corruption | `preserve_running: true` config option. Cross-reference with running container list from scheduler. Require explicit `--force` flag for removing running images. |
| **OciClient refactor breaks agent image pulls** — changing `shellwego-storage/src/oci.rs` affects all consumers | Medium | High — agents can't pull images | Preserve the existing `OciClient::pull_image()` public API signature. All internal logic changes are implementation details. Run full agent test suite after refactor. |
| **Peer discovery without control plane** — agents in air-gapped environments can't discover peers | Medium | Low — P2P falls back to HTTP | P2P is always optional and secondary to HTTP mirror chain. In air-gapped environments, only local registry mirror is used. Add mDNS Phase 2 for local network without control plane. |
| **Large layer P2P transfer timeout** — multi-GB layers may exceed transfer timeouts | Medium | Medium — P2P falls back unnecessarily | Implement per-piece timeout (30s per 1MB piece) rather than per-layer timeout. Allow configurable timeout in `PeerTransport`. Resume partial downloads from different peers. |
