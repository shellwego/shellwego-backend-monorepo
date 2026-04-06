# Plan 05: Edge Proxy Enhancements

## 1. Title & Overview

**Edge Proxy Enhancements** — Bring the `shellwego-edge` crate from 75% parity to production-grade by fixing all 72 build errors, wiring up the six major feature gaps identified in the gap analysis: (A) fix WeightedRoundRobin pattern matching and rcgen API compatibility so the crate compiles, (B) implement access logging (the `access_logging: bool` config field exists but no log writer), (C) implement proper health checks (config structs exist but the checker loop is not wired), (D) implement circuit breaker and retry logic (config struct exists but no runtime state machine), (E) implement hot-reload from filesystem (the `watch_config` method is a stub), and (F) add gRPC/HTTP2 proxying support. WebSocket proxying is already scaffolded with `handle_websocket()` and `forward_frames()` in `proxy.rs`.

---

## 2. Gap Summary

| # | Gap | Severity | Evidence | Location |
|---|---|---|---|---|
| **0** | **72 build errors** — crate does not compile | **BLOCKER** | `E0282` type annotations, `E0599` rcgen API (`generate_for` / `from_params` removed in 0.12+), `E0004` `WeightedRoundRobin` not covered in match arms, `E0277` `Instant` not `Default` | `proxy.rs`, `router.rs`, `tls.rs` |
| **A** | **Access logging not implemented** | HIGH | `EdgeConfig.access_logging: bool` (line 69 of `lib.rs`) is accepted but never read; no log file, no middleware, no structured access-log output | `lib.rs:69`, `proxy.rs` |
| **B** | **Health check loop not wired** | HIGH | `HealthCheckConfig` struct in `router.rs:411-422` is defined and attached to `Upstream.circuit_breaker`, but no background task actually polls upstreams or flips `upstream.healthy` | `router.rs:411-422`, `proxy.rs` |
| **C** | **Circuit breaker is a config-only skeleton** | MEDIUM | `CircuitBreakerConfig` struct exists at `router.rs:425-433`, stored on `Upstream` but never read at runtime. No open/half-open/closed state machine, no failure counting | `router.rs:425-433` |
| **D** | **Retry logic absent** | MEDIUM | No retry on 5xx or connection-refused from upstream. A single failure immediately returns `EdgeError::Unavailable` to the client | `proxy.rs:214-219` |
| **E** | **Config hot-reload from filesystem is a stub** | MEDIUM | `Router::watch_config()` at `router.rs:247-267` logs a debug message and returns `Ok(())`. No `notify` crate, no inotify, no actual file watching. Config reload only via `EdgeProxy::reload()` API call | `router.rs:247-267` |
| **F** | **gRPC/HTTP2 proxying not supported** | LOW | The proxy uses `hyper::client::conn::http1::handshake()` (line 313 of `proxy.rs`). gRPC requires HTTP/2 multiplexing. No `h2` crate dependency. | `proxy.rs:313`, `Cargo.toml` |
| **G** | **Connection pool never pruned** | LOW | `ConnectionPool::prune_expired()` exists but is never called. No background pruning task spawned in `EdgeProxy::new()` or `serve_https()` | `proxy.rs:969-974` |

---

## 3. Scope

### Files to Modify

| File | Change |
|---|---|
| `crates/shellwego-edge/Cargo.toml` | Add deps: `notify` (file watch), `h2` (gRPC), `tokio-rustls` (already present), `tracing-subscriber` (for access log formatting). Pin `rcgen` to compatible API or update code. |
| `crates/shellwego-edge/src/tls.rs` | Fix `generate_self_signed()`: replace deprecated `KeyPair::generate()` → `KeyPair::generate(&alg)`, `Certificate::from_params()` → `Certificate::from_params()` or rcgen 0.12 API. Fix `Instant: Default` issue in CSR helper. Add type annotations where needed. |
| `crates/shellwego-edge/src/proxy.rs` | (0) Fix any remaining type annotation errors. (A) Add access logging wrapper. (C) Add circuit breaker state machine. (D) Add retry loop around `forward_request()`. (F) Add HTTP/2 connection path for gRPC. |
| `crates/shellwego-edge/src/router.rs` | (B) Fix `WeightedRoundRobin` match arm in `select_upstream()` (already present in proxy.rs but flagged as missing by compiler). (E) Implement real `watch_config()` using `notify` crate. |
| `crates/shellwego-edge/src/lib.rs` | Wire health check loop in `EdgeProxy::new()`. Wire connection pool pruning task. Wire config file watcher if path provided. Add `access_logging` configuration to `EdgeConfig` and pass to proxy. |

### New Files to Create

| File | Purpose |
|---|---|
| `crates/shellwego-edge/src/access_log.rs` | `AccessLogger` struct: configurable output (file or stdout), structured format (JSON or Combined Log Format), per-request logging middleware. |
| `crates/shellwego-edge/src/health.rs` | `HealthChecker` struct: background task polling upstreams on intervals defined by `HealthCheckConfig`, flipping `upstream.healthy` field, emitting metrics. |
| `crates/shellwego-edge/src/circuit_breaker.rs` | `CircuitBreaker` struct: open/half-open/closed state machine per upstream, failure/success counting, configurable thresholds from `CircuitBreakerConfig`. |
| `crates/shellwego-edge/src/retry.rs` | `RetryPolicy` struct: configurable max retries, backoff strategy (fixed/exponential), retryable status codes. Integrated into `HttpProxy::forward_request()`. |

---

## 4. Prerequisites

### 4.1 Build Must Pass (BLOCKER — Phase 0)

The `shellwego-edge` crate currently fails with **72 errors**. These must be resolved before any feature work. The specific error classes and their fixes are documented in Phase 0 below.

#### 4.1.1 rcgen API Compatibility (tls.rs)

The `generate_self_signed()` method at `tls.rs:461-495` uses rcgen APIs that changed between versions:

| Current (broken) | Fix for rcgen 0.12+ |
|---|---|
| `KeyPair::generate(&PKCS_ECDSA_P256_SHA256)` | `KeyPair::generate()` — takes `&SignatureAlgorithm` reference |
| `params.alg = &PKCS_ECDSA_P256_SHA256` | `params.alg = Some(&PKCS_ECDSA_P256_SHA256)` — now `Option<&SignatureAlgorithm>` |
| `Certificate::from_params(params)` | `Certificate::from_params(params)` — still exists but signature may differ; verify |
| `cert.params` | Removed — use `Certificate::params_owned()` or store params before building |

**Action:** Read rcgen 0.12 changelog. Likely fix:
```rust
let key_pair = KeyPair::generate(&PKCS_ECDSA_P256_SHA256)
    .map_err(|e| CertError::GenerationError(format!("Failed to generate key: {}", e)))?;
let key_pem = key_pair.serialize_pem();
params.key_pair = Some(key_pair);
let cert = Certificate::from_params(params)
    .map_err(|e| CertError::GenerationError(format!("Failed to create cert: {}", e)))?;
```

#### 4.1.2 Type Annotation Errors (tls.rs — CSR generation helpers)

The manual CSR DER builder at the bottom of `tls.rs` likely has type inference issues. Add explicit `Vec<u8>` type annotations and `as u8` casts on byte literals.

#### 4.1.3 `Instant` Does Not Implement `Default` (lib.rs)

`ProxyStats::default()` at `lib.rs:135-146` has `start_time: std::time::Instant` — `Instant` has no `Default`. Fix: use `std::time::Instant::now()` explicitly:
```rust
start_time: std::time::Instant::now(),
```

#### 4.1.4 WeightedRoundRobin Pattern Match (proxy.rs)

The `select_upstream()` method at `proxy.rs:240-269` already has a `WeightedRoundRobin` arm (lines 264-268), but the build report flags it as missing. This may be a false positive from an earlier version, or there's a duplicate match in another location. **Verify** that all match arms on `LoadBalancerStrategy` are exhaustive. If the compiler still complains, add the arm.

#### 4.1.5 Other Type Annotations

Add explicit type annotations where the compiler requests them — particularly around:
- Iterator chain results in the CSR builder
- `map_err` closures on error types
- Generic function return types

### 4.2 No External Service Dependencies

All feature additions in this plan are pure code changes:
- Access logging writes to filesystem or stdout
- Health checks make HTTP requests to upstreams (already reachable by the proxy)
- Circuit breaker is an in-memory state machine
- File watching uses the `notify` crate (filesystem events only)

### 4.3 Test Infrastructure

Existing unit tests in `proxy.rs` (7 tests), `router.rs` (4 tests), and `lib.rs` (4 tests) must continue to pass. New tests must be added for each new module.

---

## 5. Detailed Implementation Steps

### Phase 0: Fix Build Errors (BLOCKER)

**Estimated effort:** 2-4 hours

**0.1 Fix `ProxyStats::default()` — `Instant` not `Default`**

File: `crates/shellwego-edge/src/lib.rs` line 135-146

Replace:
```rust
impl Default for ProxyStats {
    fn default() -> Self {
        Self {
            // ...
            start_time: std::time::Instant,  // BROKEN — no Default
        }
    }
}
```
With:
```rust
impl Default for ProxyStats {
    fn default() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
            requests_per_second: AtomicU64::new(0),
            avg_latency_us: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            start_time: std::time::Instant::now(),
        }
    }
}
```

**0.2 Fix `generate_self_signed()` rcgen API**

File: `crates/shellwego-edge/src/tls.rs` lines 461-495

Update to rcgen 0.12 API:
```rust
pub fn generate_self_signed(domain: &str) -> Result<Certificate, CertError> {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_ECDSA_P256_SHA256};

    let mut params = CertificateParams::default();
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, domain);
    params.alg = Some(&PKCS_ECDSA_P256_SHA256);
    params.subject_alt_names = vec![rcgen::SanType::DnsName(domain.to_string())];

    let key_pair = KeyPair::generate()
        .map_err(|e| CertError::GenerationError(format!("Failed to generate key: {}", e)))?;

    let key_pem = key_pair.serialize_pem();
    params.key_pair = Some(key_pair);
    let cert = Certificate::from_params(params)
        .map_err(|e| CertError::GenerationError(format!("Failed to create cert: {}", e)))?;

    let cert_pem = cert.serialize_pem()
        .map_err(|e| CertError::GenerationError(format!("Failed to serialize cert: {}", e)))?;

    // ... rest unchanged
}
```

> **Note:** Run `cargo doc --package rcgen --open` to verify exact API signatures for the installed version. The above is based on rcgen 0.12 release notes. If the crate version in `Cargo.toml` differs, adjust accordingly.

**0.3 Fix type annotations in CSR builder**

File: `crates/shellwego-edge/src/tls.rs` (bottom of file, `generate_ecdsa_csr` function and related helpers)

- Add `: Vec<u8>` type annotations to intermediate variables
- Ensure all byte array indexing returns `u8`
- Fix `map_err` closures that don't match expected error types

**0.4 Verify WeightedRoundRobin exhaustiveness**

File: `crates/shellwego-edge/src/proxy.rs` lines 240-269

The match already has a `WeightedRoundRobin` arm. If the compiler still errors, check for:
- A second match on `LoadBalancerStrategy` elsewhere in the crate
- A `match` in `router.rs` that's missing the variant

Search pattern: `match.*load_balancer` and `match.*LoadBalancerStrategy`

**0.5 Fix remaining type annotations**

Run `cargo check -p shellwego-edge 2>&1` and address each remaining error. Common fixes:
- Add turbofish annotations: `collect::<Vec<_>>()`
- Add explicit return types on closures: `|x: Foo| -> Bar { ... }`
- Add `#[allow(type_alias_bounds)]` if needed for complex trait bounds

**0.6 Verify zero errors**

```bash
cargo check -p shellwego-edge 2>&1 | tail -1
# Expected: "Finished" with 0 errors (warnings acceptable)
```

---

### Phase A: Access Logging

**Estimated effort:** 4-6 hours

**A1. Create `access_log.rs`**

File: `crates/shellwego-edge/src/access_log.rs` (new)

```rust
use std::io::Write;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::BufWriter;
use tracing::info;

/// Access log format
#[derive(Debug, Clone, Copy)]
pub enum AccessLogFormat {
    /// Apache Combined Log Format
    Combined,
    /// JSON structured logging
    Json,
}

/// Access logger
pub struct AccessLogger {
    writer: Arc<tokio::sync::Mutex<Option<BufWriter<File>>>>,
    format: AccessLogFormat,
}

impl AccessLogger {
    /// Create logger that writes to stdout
    pub fn stdout(format: AccessLogFormat) -> Self {
        Self {
            writer: Arc::new(tokio::sync::Mutex::new(None)),
            format,
        }
    }

    /// Create logger that writes to a file
    pub async fn file(path: &str, format: AccessLogFormat) -> Result<Self, std::io::Error> {
        let file = File::create(path).await?;
        let writer = BufWriter::new(file);
        Ok(Self {
            writer: Arc::new(tokio::sync::Mutex::new(Some(writer))),
            format,
        })
    }

    /// Log a single request
    pub async fn log(&self, entry: &AccessLogEntry) {
        let line = match self.format {
            AccessLogFormat::Combined => format_combined(entry),
            AccessLogFormat::Json => format_json(entry),
        };

        let mut guard = self.writer.lock().await;
        if let Some(ref mut writer) = *guard {
            let _ = writeln!(writer, "{}", line);
            let _ = writer.flush();
        } else {
            // Fallback: log via tracing
            info!("{}", line.trim());
        }
    }
}

/// Single access log entry
pub struct AccessLogEntry {
    pub client_ip: String,
    pub method: String,
    pub path: String,
    pub protocol: String,
    pub status: u16,
    pub response_size: u64,
    pub latency_ms: u64,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub upstream_url: Option<String>,
}

fn format_combined(e: &AccessLogEntry) -> String {
    let ua = e.user_agent.as_deref().unwrap_or("-");
    format!(
        "{} - - [{}] \"{} {} {}\" {} {} \"{}\" \"{}\"",
        e.client_ip,
        chrono::Utc::now().format("%d/%b/%Y:%H:%M:%S +0000"),
        e.method, e.path, e.protocol,
        e.status, e.response_size,
        "-",  // referer
        ua,
    )
}

fn format_json(e: &AccessLogEntry) -> String {
    serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "client_ip": e.client_ip,
        "method": e.method,
        "path": e.path,
        "protocol": e.protocol,
        "status": e.status,
        "response_size": e.response_size,
        "latency_ms": e.latency_ms,
        "user_agent": e.user_agent,
        "request_id": e.request_id,
        "upstream_url": e.upstream_url,
    }).to_string()
}
```

**A2. Register module in `lib.rs`**

File: `crates/shellwego-edge/src/lib.rs`

Add: `pub mod access_log;`

**A3. Add `AccessLogger` to `EdgeProxy`**

File: `crates/shellwego-edge/src/lib.rs`

Add field to `EdgeProxy`:
```rust
/// Access logger (if enabled)
access_logger: Option<Arc<access_log::AccessLogger>>,
```

In `EdgeProxy::new()`:
```rust
let access_logger = if config.access_logging {
    match &config.access_log_path {
        Some(path) => Some(Arc::new(access_log::AccessLogger::file(
            path,
            config.access_log_format.unwrap_or(access_log::AccessLogFormat::Combined),
        ).await?)),
        None => Some(Arc::new(access_log::AccessLogger::stdout(
            config.access_log_format.unwrap_or(access_log::AccessLogFormat::Combined),
        ))),
    }
} else {
    None
};
```

**A4. Add config fields to `EdgeConfig`**

File: `crates/shellwego-edge/src/lib.rs`

```rust
/// Access log file path (None = stdout)
pub access_log_path: Option<String>,
/// Access log format
pub access_log_format: Option<access_log::AccessLogFormat>,
```

**A5. Wire access logging into `handle_https_request()`**

File: `crates/shellwego-edge/src/lib.rs`

After the response is returned from `proxy.handle_request()`, log the access entry:
```rust
// After getting result from proxy.handle_request()
if let Some(ref logger) = access_logger {
    let entry = access_log::AccessLogEntry {
        client_ip: request_info.client_ip.clone(),
        method: request_info.method.clone(),
        path: request_info.path.clone(),
        protocol: "HTTP/1.1".to_string(),
        status: result.as_ref().map(|r| r.status().as_u16()).unwrap_or(502),
        response_size: 0, // Would need body size tracking
        latency_ms: start.elapsed().as_millis() as u64,
        user_agent: request_info.headers.get("user-agent").cloned(),
        request_id: None, // From middleware X-Request-Id if present
        upstream_url: Some(route.upstreams[0].url.clone()),
    };
    logger.log(&entry).await;
}
```

**A6. Tests**

- Unit test: `format_combined()` produces valid CLF output
- Unit test: `format_json()` produces valid JSON with all fields
- Integration test: start proxy with `access_logging: true`, make request, verify log output appears

---

### Phase B: Health Checking

**Estimated effort:** 4-6 hours

**B1. Create `health.rs`**

File: `crates/shellwego-edge/src/health.rs` (new)

```rust
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::router::{HealthCheckConfig, Route};

/// Background health checker that polls upstreams
pub struct HealthChecker {
    router: Arc<RwLock<Vec<Route>>>,
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl HealthChecker {
    pub fn new(router: Arc<RwLock<Vec<Route>>>) -> Self {
        Self {
            router,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Start the health check loop. Spawns a background task.
    pub fn start(self: Arc<Self>) {
        self.running.store(true, std::sync::atomic::Ordering::Relaxed);
        tokio::spawn(async move {
            self.run().await;
        });
    }

    /// Stop the health check loop
    pub fn stop(&self) {
        self.running.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    async fn run(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(10)); // Check every 10s

        loop {
            interval.tick().await;

            if !self.running.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            self.check_all_upstreams().await;
        }
    }

    async fn check_all_upstreams(&self) {
        let routes = self.router.read().await;

        for route in routes.iter() {
            for upstream in &route.upstreams {
                if let Some(ref config) = upstream.health_check {
                    let healthy = self.check_upstream(&upstream.url, config).await;
                    // Note: we need interior mutability on the Route/Upstream
                    // to flip the healthy flag. This requires changing `healthy`
                    // to `Arc<AtomicBool>` or using a separate health state map.
                    debug!(
                        "Health check {} → upstream {}: {}",
                        route.id, upstream.url,
                        if healthy { "healthy" } else { "unhealthy" }
                    );
                }
            }
        }
    }

    async fn check_upstream(
        &self,
        url: &str,
        config: &HealthCheckConfig,
    ) -> bool {
        let check_url = format!("{}{}", url.trim_end_matches('/'), config.path);

        let result = tokio::time::timeout(
            Duration::from_secs(config.timeout_secs),
            reqwest::get(&check_url),
        )
        .await;

        match result {
            Ok(Ok(resp)) => resp.status().is_success(),
            Ok(Err(e)) => {
                warn!("Health check failed for {}: {}", url, e);
                false
            }
            Err(_) => {
                warn!("Health check timeout for {}", url);
                false
            }
        }
    }
}
```

**B2. Make `Upstream::healthy` mutable at runtime**

The current `Upstream` struct has `pub healthy: bool` which is not atomically mutable. Two options:

**Option A (preferred):** Change to `pub healthy: Arc<AtomicBool>`:

File: `crates/shellwego-edge/src/router.rs`

```rust
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct Upstream {
    pub url: String,
    pub weight: u32,
    pub healthy: Arc<AtomicBool>,
    pub health_check: Option<HealthCheckConfig>,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
}

impl Default for Upstream {
    fn default() -> Self {
        Self {
            url: String::new(),
            weight: 1,
            healthy: Arc::new(AtomicBool::new(true)),
            health_check: None,
            circuit_breaker: None,
        }
    }
}
```

Update `select_upstream()` in `proxy.rs` line 232:
```rust
let healthy_upstreams: Vec<_> = route.upstreams.iter()
    .filter(|u| u.healthy.load(std::sync::atomic::Ordering::Relaxed))
    .collect();
```

Update serialization — `Arc<AtomicBool>` is not `Serialize`/`Deserialize`. Add a custom serde helper:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upstream {
    pub url: String,
    pub weight: u32,
    #[serde(default = "default_healthy")]
    pub healthy: Arc<AtomicBool>,
    pub health_check: Option<HealthCheckConfig>,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
}

fn default_healthy() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(true))
}
```

This requires `Arc<AtomicBool>` to implement `Serialize`/`Deserialize` via a custom wrapper or newtype:
```rust
#[derive(Debug, Clone)]
pub struct AtomicBoolWrapper(Arc<AtomicBool>);

impl Serialize for AtomicBoolWrapper { /* ... */ }
impl<'de> Deserialize<'de> for AtomicBoolWrapper { /* ... */ }
```

**Option B (simpler):** Maintain a separate `HashMap<String, AtomicBool>` in `EdgeProxy` keyed by upstream URL. The health checker updates this map. `select_upstream()` checks both the map and the route's `healthy` field.

**B3. Wire health checker into `EdgeProxy::new()`**

File: `crates/shellwego-edge/src/lib.rs`

```rust
// After creating router
let routes_ref = /* shared reference to route list */;
let health_checker = Arc::new(HealthChecker::new(routes_ref));
health_checker.clone().start();
```

Store `health_checker` on `EdgeProxy` for graceful shutdown:
```rust
/// Health checker handle
health_checker: Option<Arc<HealthChecker>>,
```

In `ServerHandle::shutdown()`, call `health_checker.stop()`.

**B4. Tests**

- Unit test: `HealthChecker::check_upstream()` returns true for a mock HTTP server returning 200
- Unit test: `HealthChecker::check_upstream()` returns false for 500 or timeout
- Integration test: start proxy with health-check config, kill upstream, verify upstream marked unhealthy, verify 503 returned

---

### Phase C: Circuit Breaker

**Estimated effort:** 6-8 hours

**C1. Create `circuit_breaker.rs`**

File: `crates/shellwego-edge/src/circuit_breaker.rs` (new)

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use tracing::{info, warn};

use crate::router::CircuitBreakerConfig;

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — requests pass through
    Closed,
    /// Failures exceeded threshold — requests are rejected
    Open,
    /// Probing — allow limited requests to test recovery
    HalfOpen,
}

/// Per-upstream circuit breaker
struct Circuit {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
    config: CircuitBreakerConfig,
}

impl Circuit {
    fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            config,
        }
    }

    fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.config.success_threshold {
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                    info!("Circuit breaker: CLOSED (recovered)");
                }
            }
            CircuitState::Open => {
                // Should not happen — check_timeout should transition to HalfOpen first
            }
        }
    }

    fn record_failure(&mut self) {
        self.last_failure_time = Some(Instant::now());

        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.config.failure_threshold {
                    self.state = CircuitState::Open;
                    warn!(
                        "Circuit breaker: OPEN ({} failures)",
                        self.failure_count
                    );
                }
            }
            CircuitState::HalfOpen => {
                self.state = CircuitState::Open;
                warn!("Circuit breaker: OPEN (probe failed)");
            }
            CircuitState::Open => {
                // Already open
            }
        }
    }

    fn check_timeout(&mut self) {
        if self.state == CircuitState::Open {
            if let Some(last_fail) = self.last_failure_time {
                if last_fail.elapsed() >= Duration::from_secs(self.config.timeout_secs) {
                    self.state = CircuitState::HalfOpen;
                    self.success_count = 0;
                    info!("Circuit breaker: HALF-OPEN (timeout elapsed)");
                }
            }
        }
    }

    fn is_request_allowed(&mut self) -> bool {
        self.check_timeout();
        match self.state {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => true, // Allow probe
            CircuitState::Open => false,
        }
    }
}

/// Circuit breaker registry — one breaker per upstream URL
pub struct CircuitBreakerRegistry {
    breakers: RwLock<HashMap<String, Circuit>>,
}

impl CircuitBreakerRegistry {
    pub fn new() -> Self {
        Self {
            breakers: RwLock::new(HashMap::new()),
        }
    }

    /// Ensure a breaker exists for the given upstream URL.
    /// Called when routes are loaded.
    pub fn register(&self, upstream_url: &str, config: CircuitBreakerConfig) {
        let mut breakers = self.breakers.write();
        breakers.entry(upstream_url.to_string())
            .or_insert_with(|| Circuit::new(config));
    }

    /// Check if a request is allowed through the circuit breaker
    pub fn is_request_allowed(&self, upstream_url: &str) -> bool {
        let mut breakers = self.breakers.write();
        if let Some(circuit) = breakers.get_mut(upstream_url) {
            circuit.is_request_allowed()
        } else {
            true // No breaker configured — allow
        }
    }

    /// Record a successful request
    pub fn record_success(&self, upstream_url: &str) {
        let mut breakers = self.breakers.write();
        if let Some(circuit) = breakers.get_mut(upstream_url) {
            circuit.record_success();
        }
    }

    /// Record a failed request
    pub fn record_failure(&self, upstream_url: &str) {
        let mut breakers = self.breakers.write();
        if let Some(circuit) = breakers.get_mut(upstream_url) {
            circuit.record_failure();
        }
    }

    /// Get the current state for an upstream
    pub fn state(&self, upstream_url: &str) -> Option<CircuitState> {
        let mut breakers = self.breakers.write();
        breakers.get_mut(upstream_url).map(|c| {
            c.check_timeout();
            c.state
        })
    }
}
```

**C2. Add `CircuitBreakerRegistry` to `HttpProxy`**

File: `crates/shellwego-edge/src/proxy.rs`

```rust
pub struct HttpProxy {
    pool: ConnectionPool,
    metrics: ProxyMetrics,
    request_timeout: Duration,
    connect_timeout: Duration,
    circuit_breakers: Arc<CircuitBreakerRegistry>,  // NEW
}
```

**C3. Check circuit breaker before forwarding**

File: `crates/shellwego-edge/src/proxy.rs` in `handle_request_inner()`

Before calling `self.forward_request()`:
```rust
if !self.circuit_breakers.is_request_allowed(upstream_url) {
    self.circuit_breakers.record_failure(upstream_url);
    return Err(EdgeError::Unavailable(
        "Circuit breaker open for upstream".into(),
    ));
}
```

After successful response:
```rust
self.circuit_breakers.record_success(upstream_url);
```

On error:
```rust
self.circuit_breakers.record_failure(upstream_url);
```

**C4. Register breakers on route load**

File: `crates/shellwego-edge/src/lib.rs`

When routes are loaded (in `EdgeProxy::new()` and `reload()`):
```rust
for route in &config.routes {
    for upstream in &route.upstreams {
        if let Some(ref cb_config) = upstream.circuit_breaker {
            proxy.circuit_breakers().register(&upstream.url, cb_config.clone());
        }
    }
}
```

**C5. Tests**

- Unit test: circuit transitions Closed → Open after N failures
- Unit test: circuit transitions Open → HalfOpen after timeout
- Unit test: circuit transitions HalfOpen → Closed after N successes
- Unit test: circuit stays Open and rejects requests
- Integration test: configure route with `failure_threshold: 3`, kill upstream, verify 3 failures → 503

---

### Phase D: Retry Logic

**Estimated effort:** 3-4 hours

**D1. Create `retry.rs`**

File: `crates/shellwego-edge/src/retry.rs` (new)

```rust
use std::time::Duration;
use tracing::{debug, warn};

/// Backoff strategy
#[derive(Debug, Clone, Copy)]
pub enum BackoffStrategy {
    /// Fixed delay between retries
    Fixed { delay_ms: u64 },
    /// Exponential backoff: base * 2^attempt
    Exponential { base_ms: u64, max_ms: u64 },
}

impl Default for BackoffStrategy {
    fn default() -> Self {
        BackoffStrategy::Exponential {
            base_ms: 100,
            max_ms: 5000,
        }
    }
}

/// Retry policy configuration
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (0 = no retries)
    pub max_retries: u32,
    /// Backoff strategy
    pub backoff: BackoffStrategy,
    /// HTTP status codes that trigger retry
    pub retryable_status_codes: Vec<u16>,
    /// Whether to retry on connection errors
    pub retry_on_connection_error: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            backoff: BackoffStrategy::default(),
            retryable_status_codes: vec![502, 503, 504, 429],
            retry_on_connection_error: true,
        }
    }
}

impl RetryPolicy {
    /// No-retry policy
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            backoff: BackoffStrategy::Fixed { delay_ms: 0 },
            retryable_status_codes: vec![],
            retry_on_connection_error: false,
        }
    }

    /// Calculate delay for the given attempt (0-indexed)
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        match self.backoff {
            BackoffStrategy::Fixed { delay_ms } => Duration::from_millis(delay_ms),
            BackoffStrategy::Exponential { base_ms, max_ms } => {
                let delay = base_ms * 2u64.saturating_pow(attempt);
                Duration::from_millis(delay.min(max_ms))
            }
        }
    }

    /// Check if a response status code should be retried
    pub fn is_retryable_status(&self, status: u16) -> bool {
        self.retryable_status_codes.contains(&status)
    }
}
```

**D2. Integrate retry into `HttpProxy::forward_request()`**

File: `crates/shellwego-edge/src/proxy.rs`

Wrap `self.create_connection_and_send()` in a retry loop:
```rust
async fn forward_request_with_retry(
    &self,
    request: Request<Body>,
    upstream_url: &str,
    policy: &RetryPolicy,
) -> Result<Response<Body>, EdgeError> {
    let mut last_error = None;

    for attempt in 0..=policy.max_retries {
        if attempt > 0 {
            let delay = policy.delay_for_attempt(attempt - 1);
            debug!(
                "Retry attempt {}/{} for {} (waiting {}ms)",
                attempt, policy.max_retries, upstream_url, delay.as_millis()
            );
            tokio::time::sleep(delay).await;
        }

        match self.create_connection_and_send(upstream_url, request_clone(&request)?).await {
            Ok(response) => {
                if policy.is_retryable_status(response.status().as_u16()) {
                    debug!(
                        "Retryable status {} from {} (attempt {}/{})",
                        response.status(), upstream_url, attempt, policy.max_retries
                    );
                    last_error = Some(EdgeError::Unavailable(
                        format!("Upstream returned {}", response.status()),
                    ));
                    continue;
                }
                return Ok(response);
            }
            Err(e) => {
                if policy.retry_on_connection_error {
                    warn!(
                        "Connection error to {} (attempt {}/{}): {}",
                        upstream_url, attempt, policy.max_retries, e
                    );
                    last_error = Some(e);
                    continue;
                }
                return Err(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        EdgeError::Unavailable("Max retries exceeded".into())
    }))
}
```

> **Note:** Request cloning is needed for retries since `Request<Body>` is consumed on send. Use `http_body_util::BodyExt` or reconstruct the request. For bodies that are not replayable (streaming), the first attempt's body must be buffered.

**D3. Add `RetryPolicy` to `Route` or `EdgeConfig`**

File: `crates/shellwego-edge/src/router.rs`

Add to `Route`:
```rust
/// Retry policy
pub retry: Option<RetryPolicyConfig>,
```

**D4. Tests**

- Unit test: retry with `max_retries: 2` succeeds on 2nd attempt
- Unit test: retry exhausts all attempts and returns last error
- Unit test: non-retryable status code (e.g., 404) returns immediately without retry
- Unit test: exponential backoff delay calculation

---

### Phase E: Config Hot-Reload from Filesystem

**Estimated effort:** 3-4 hours

**E1. Add `notify` crate dependency**

File: `crates/shellwego-edge/Cargo.toml`

```toml
notify = { version = "6", features = ["macos_fsevent"] }
```

**E2. Implement real `watch_config()`**

File: `crates/shellwego-edge/src/router.rs` (or create `crates/shellwego-edge/src/config_watcher.rs`)

```rust
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config as NotifyConfig};
use std::path::Path;
use std::sync::mpsc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// Watch a configuration file for changes and trigger reload
pub fn watch_config_file(
    path: &str,
    router: Arc<RwLock<Router>>,
    callback: Arc<dyn Fn(Vec<Route>) + Send + Sync>,
) -> Result<RecommendedWatcher, EdgeError> {
    let path = Path::new(path).to_path_buf();

    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();

    let mut watcher = notify::recommended_watcher(tx)
        .map_err(|e| EdgeError::ConfigError(format!("Failed to create file watcher: {}", e)))?;

    watcher.watch(&path, RecursiveMode::NonRecursive)
        .map_err(|e| EdgeError::ConfigError(format!("Failed to watch config file: {}", e)))?;

    info!("Watching config file: {}", path.display());

    // Spawn a tokio task to process filesystem events
    tokio::spawn(async move {
        loop {
            match rx.recv() {
                Ok(Ok(event)) => {
                    if event.kind.is_modify() || event.kind.is_create() {
                        info!("Config file changed: {:?}", event.paths);
                        match tokio::fs::read_to_string(&path).await {
                            Ok(content) => {
                                match serde_json::from_str::<Vec<Route>>(&content) {
                                    Ok(routes) => {
                                        callback(routes);
                                        info!("Config reloaded from file");
                                    }
                                    Err(e) => {
                                        error!("Failed to parse config: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to read config file: {}", e);
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    warn!("File watch error: {}", e);
                }
                Err(_) => {
                    // Channel closed — watcher dropped
                    break;
                }
            }
        }
    });

    Ok(watcher)
}
```

**E3. Wire into `EdgeProxy`**

File: `crates/shellwego-edge/src/lib.rs`

Add to `EdgeConfig`:
```rust
/// Path to config file for hot-reload (None = no file watching)
pub config_file_path: Option<String>,
```

In `EdgeProxy::new()`:
```rust
if let Some(ref config_path) = config.config_file_path {
    let router_clone = router.clone();
    config_watcher::watch_config_file(
        config_path,
        router_clone,
        Arc::new(move |routes| {
            // This callback runs on the tokio runtime
            let router = router_clone.clone();
            tokio::spawn(async move {
                let mut guard = router.write().await;
                guard.clear();
                for route in routes {
                    let _ = guard.add_route(route);
                }
            });
        }),
    )?;
}
```

**E4. Add debounce to avoid reload storms**

Add a 500ms debounce between file change events and actual reload:

```rust
use tokio::time::{Duration, sleep};

// In the event processing loop:
let mut last_reload = Instant::now();
if event.kind.is_modify() {
    if last_reload.elapsed() < Duration::from_millis(500) {
        continue; // Debounce
    }
    last_reload = Instant::now();
    // ... reload
}
```

**E5. Tests**

- Unit test: create temp file, write config, verify watcher triggers reload
- Integration test: start proxy with config file, modify file, verify new routes take effect

---

### Phase F: gRPC / HTTP2 Proxying (Stretch Goal)

**Estimated effort:** 8-12 hours

**F1. Add HTTP/2 client support**

File: `crates/shellwego-edge/Cargo.toml`

```toml
h2 = "0.4"
```

**F2. Add HTTP/2 handshake path in proxy**

File: `crates/shellwego-edge/src/proxy.rs`

Add a method for HTTP/2 connections:
```rust
async fn forward_request_h2(
    &self,
    request: Request<Body>,
    upstream_url: &str,
) -> Result<Response<Body>, EdgeError> {
    let url: http::Uri = upstream_url.parse()
        .map_err(|e| EdgeError::RoutingError(format!("Invalid URL: {}", e)))?;

    let host = url.host()
        .ok_or_else(|| EdgeError::RoutingError("Missing host".into()))?;
    let port = url.port_u16().unwrap_or(443);

    let stream = TcpStream::connect((host, port)).await
        .map_err(|e| EdgeError::Unavailable(format!("Connect failed: {}", e)))?;

    // HTTP/2 client handshake
    let (mut sender, conn) = h2::client::handshake(stream).await
        .map_err(|e| EdgeError::RoutingError(format!("H2 handshake failed: {}", e)))?;

    tokio::spawn(async move {
        let _ = conn.await;
    });

    // Convert hyper Request to h2 request and send
    // ... (requires converting between hyper Body and h2 frame types)
}
```

**F3. Detect gRPC requests**

gRPC requests can be detected by:
- `Content-Type: application/grpc` header
- `:scheme: http` pseudo-header (HTTP/2)
- `TE: trailers` header

In `handle_request_inner()`, check for gRPC content type and route to HTTP/2 path.

**F4. Protocol detection logic**

```rust
fn is_grpc_request(request: &Request<Body>) -> bool {
    request.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.starts_with("application/grpc"))
        .unwrap_or(false)
}
```

**F5. Tests**

- Unit test: `is_grpc_request()` detects gRPC content type
- Integration test: proxy gRPC health check request through to a gRPC server

---

### Phase G: Connection Pool Pruning

**Estimated effort:** 1 hour

**G1. Spawn pool pruning task**

File: `crates/shellwego-edge/src/lib.rs`

In `EdgeProxy::new()`:
```rust
let pool = proxy.pool_clone();
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        pool.prune_expired();
        debug!("Connection pool pruned");
    }
});
```

Add `pool_clone()` method to `HttpProxy`:
```rust
pub fn pool_clone(&self) -> ConnectionPool {
    self.pool.clone()
}
```

---

## 6. Dependencies on Other Plans

| Plan ID | Dependency | Notes |
|---|---|---|
| **Plan 01 (Security Hardening)** | None | Edge proxy does not use JWT auth or KMS. Independent. |
| **Plan 02 (Schema Consolidation)** | Low | If `Route`, `Upstream`, `Middleware` types move to `shellwego-schema`, imports change. Monitor but not blocking. |
| **Plan 03 (Control Plane)** | **Medium** | The "control plane integration" gap (edge receiving route updates automatically) depends on control-plane having a push-based route distribution API. Can be deferred to a follow-up. |
| **Plan 04 (Agent)** | None | Agent and edge are independent crates. |

**Recommendation:** This plan is **self-contained** except for the optional control-plane integration (Phase F follow-up). Execute after Phase 0 (build fix) completes. The schema consolidation plan (Plan 02) should coordinate with this plan on shared types.

---

## 7. Acceptance Criteria

### Phase 0: Build
- [ ] `cargo check -p shellwego-edge` passes with **0 errors** (warnings acceptable)
- [ ] `cargo test -p shellwego-edge` passes all existing tests (15 tests)
- [ ] `cargo clippy -p shellwego-edge` has no new errors

### Phase A: Access Logging
- [ ] Config with `access_logging: true` produces log output on stdout
- [ ] Config with `access_log_path: "/tmp/access.log"` writes to file
- [ ] Log entries include: client IP, method, path, status, latency, upstream URL
- [ ] Config with `access_logging: false` produces no access log output
- [ ] Both Combined and JSON formats produce valid output

### Phase B: Health Checking
- [ ] Upstream with `health_check` config is polled at configured interval
- [ ] Upstream returning 200 stays healthy
- [ ] Upstream returning 500 or timing out is marked unhealthy
- [ ] Unhealthy upstream is excluded from `select_upstream()` rotation
- [ ] Upstream recovering (returning 200 again) is re-added to rotation
- [ ] 503 returned when all upstreams for a route are unhealthy

### Phase C: Circuit Breaker
- [ ] After `failure_threshold` failures, circuit opens and subsequent requests get 503
- [ ] After `timeout_secs`, circuit transitions to half-open
- [ ] After `success_threshold` successes in half-open, circuit closes
- [ ] Circuit breaker state is queryable via metrics or API

### Phase D: Retry Logic
- [ ] Request to upstream returning 502 is retried up to `max_retries`
- [ ] Connection error triggers retry if `retry_on_connection_error: true`
- [ ] 404 response is NOT retried (not in `retryable_status_codes`)
- [ ] Exponential backoff doubles delay between attempts
- [ ] `max_retries: 0` disables retry (default for routes without config)

### Phase E: Config Hot-Reload
- [ ] `config_file_path` set → file watcher starts
- [ ] Modifying config file triggers route table reload
- [ ] 500ms debounce prevents reload storms from rapid saves
- [ ] Invalid config file → error logged, existing routes unchanged
- [ ] `EdgeProxy::reload()` API still works independently of file watching

### Phase F: gRPC (Stretch)
- [ ] Request with `Content-Type: application/grpc` is routed via HTTP/2
- [ ] gRPC health check request reaches upstream and response is returned
- [ ] Non-gRPC requests continue using HTTP/1.1 path

### Phase G: Pool Pruning
- [ ] Background task prunes expired connections every 60 seconds
- [ ] No memory leak from stale pooled connections

### Integration
- [ ] Full end-to-end: start proxy with 2 upstreams (1 healthy, 1 unhealthy), verify traffic routes only to healthy upstream, kill healthy upstream, verify circuit breaker opens, bring it back, verify recovery
- [ ] `cargo test -p shellwego-edge` passes with all new and existing tests

---

## 8. Estimated Complexity

**XL** (Extra Large)

| Phase | Lines of Code (est.) | Complexity | Risk |
|---|---|---|---|
| Phase 0: Build fixes | ~50 changed | Medium | Low — mechanical fixes |
| Phase A: Access logging | ~200 new | Low | Low — well-trodden pattern |
| Phase B: Health checking | ~250 new | Medium | Medium — requires interior mutability refactor |
| Phase C: Circuit breaker | ~300 new | Medium-High | Medium — state machine, concurrency |
| Phase D: Retry logic | ~150 new | Medium | Low — isolated module |
| Phase E: Config hot-reload | ~150 new | Medium | Low — `notify` crate is mature |
| Phase F: gRPC proxying | ~400 new | High | High — HTTP/2 integration, frame conversion |
| Phase G: Pool pruning | ~20 changed | Low | Low — one spawn + interval |
| **Total** | **~1520 lines** | | |

**Estimated effort:** 30-45 hours total

**Priority ordering:**
1. Phase 0 (build fix) — BLOCKER, do first
2. Phase G (pool pruning) — trivial, do alongside Phase 0
3. Phase A (access logging) — high value, low risk
4. Phase B (health checking) — required for production
5. Phase D (retry logic) — required for production
6. Phase C (circuit breaker) — required for production
7. Phase E (config hot-reload) — nice-to-have for ops
8. Phase F (gRPC) — stretch goal, highest complexity

---

## 9. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **rcgen API changes between minor versions** — fix may not match installed version | Medium | High — build still broken | Pin rcgen version in Cargo.toml; check `cargo doc` for exact signatures; if 0.12 API is too different, consider 0.11 or 0.13 |
| **`Arc<AtomicBool>` breaks serde for `Upstream`** — config deserialization fails | High | High — routes can't be loaded | Use Option B (separate `HashMap<String, AtomicBool>` for health state) or implement custom `Serialize`/`Deserialize` for a newtype wrapper |
| **Health checker races with route reload** — health checker holds stale route references | Medium | Medium — brief routing inconsistencies | Use `Arc<RwLock<Vec<Route>>>` shared between health checker and router; health checker reads under read lock |
| **Circuit breaker false positives** — transient errors (DNS blips) open circuits prematurely | Medium | High — all traffic to upstream cut off | Use conservative defaults: `failure_threshold: 5`, `timeout_secs: 30`; add jitter to timeout; log circuit state changes prominently |
| **Retry amplification** — retries on 502 from overloaded upstream make things worse | Medium | High — cascading failure | Default to `max_retries: 2`; add `retry_on` headers awareness; consider "circuit breaker then retry" ordering: check circuit first, retry only if circuit closed |
| **File watcher doesn't work in Docker/container** — inotify may not propagate | Medium | Low — file reload just doesn't work | Fall back to API-based reload; document that file watching requires volume mounts |
| **HTTP/2 proxying complexity** — h2 crate integration with hyper Body is non-trivial | High | Medium — gRPC feature deferred | Mark Phase F as stretch goal; if too complex, defer to a dedicated gRPC proxy plan; consider using `hyper` 1.0 HTTP/2 client support instead of raw `h2` |
| **Access log file I/O becomes bottleneck** — synchronous writes on every request | Low | Medium — latency increase | Use `BufWriter` with periodic flushing (every 100 lines or every 1 second), not per-request flush |
| **`notify` crate version incompatibility** — macOS vs Linux filesystem event APIs differ | Low | Low | `notify` 6.x handles platform differences internally; no custom code needed |
