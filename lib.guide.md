Based on my comprehensive analysis of the ShellWeGo codebase and current Rust ecosystem research, here's an updated `lib.guide.md` with modern alternatives, deprecated crate replacements, and battle-tested libraries for 2024-2025:

```markdown
Here are battle-tested libraries to slash LOC while maintaining performance and safety. Each replaces handwritten code with zero-cost abstractions.

---

## Core & Async

| Library | Replaces | Savings | Why |
|---------|----------|---------|-----|
| **`tokio-console`** | Custom metrics/debug | ~500 LOC | Real-time async task introspection |
| **`dashmap`** | `RwLock<HashMap>` | ~200 LOC | Concurrent hashmap without locks |
| **`parking_lot`** | `std::sync` primitives | ~100 LOC | Faster, smaller mutexes/conds |
| **`deadpool`** | Custom connection pools | ~400 LOC | Async pool for DB/HTTP/NATS |
| **`bb8`** | Async connection pools | ~300 LOC | Alternative with health checks |
| **`tokio-stream`** | Stream combinators | ~300 LOC | Async iteration utilities |
| **`quinn`** | TCP between nodes | ~500 LOC | QUIC for control plane mesh (from lib.guide.md) |
| **`futures-lite`** | `futures` heavy | ~100 LOC | Lightweight async utilities |

---

## Web & API

| Library | Replaces | Savings | Why |
|---------|----------|---------|-----|
| **`axum`** + **`tower-http`** | Custom HTTP glue | ~600 LOC | Modern, composable web framework (already in use) |
| **`axum-extra`** | Custom extractors | ~400 LOC | Typed headers, cache control, protobuf |
| **`garde`** | Manual validation | ~600 LOC | Declarative validation (faster than validator) |
| **`aide`** | Utoipa boilerplate | ~800 LOC | Axum-native OpenAPI, no macros |
| **`rust-embed`** | Static file serving | ~200 LOC | Embed assets in binary |
| **`askama`** | String templates | ~400 LOC | Type-checked HTML/JSON templates |
| **`maud`** | HTML generation | ~300 LOC | Compile-time HTML macros |
| **`tower-sessions`** | Custom session mgmt | ~300 LOC | Type-safe session handling |

**Note**: The codebase uses `axum` 0.7 (latest) and `utoipa` - consider migrating to `aide` for derive-free OpenAPI if macro complexity becomes an issue.

---

## Database & Storage

| Library | Replaces | Savings | Why |
|---------|----------|---------|-----|
| **`sea-orm`** | Raw SQLx | ~2000 LOC | ActiveRecord pattern, migrations, relations |
| **`sqlx`** (current) | Raw SQL | ~1500 LOC | Compile-time checked queries (already in use) |
| **`migrator`** | Custom migrations | ~400 LOC | Versioned schema changes |
| **`sled`** | SQLite for metadata | ~500 LOC | Pure-Rust KV, zero-config |
| **`zstd`** | Custom compression | ~200 LOC | Streaming compression for snapshots |
| **`fred`** | `redis` crate | ~200 LOC | Modern Redis client with cluster support |

**Update**: The codebase uses `sqlx` - consider adding `sea-orm` for the control plane's complex entity relationships while keeping `sqlx` for raw performance-critical queries.

---

## Networking & VMM

| Library | Replaces | Savings | Why |
|---------|----------|---------|-----|
| **`firecracker-rs`** (AWS) | Custom VMM HTTP | ~1500 LOC | Official Rust SDK (from lib.guide.md) |
| **`vmm-sys-util`** | `libc` calls | ~400 LOC | Safe wrappers for KVM/ioctls |
| **`netlink-sys`** | Raw netlink | ~600 LOC | Async netlink protocols |
| **`xdp-rs`** | eBPF loader | ~800 LOC | XDP program loading |
| **`rustls-acme`** | TLS cert management | ~600 LOC | Automatic Let's Encrypt |
| **`hickory-dns`** (ex-trust-dns) | `trust-dns-resolver` | ~200 LOC | Modern async DNS (trust-dns renamed) |

**Note**: `trust-dns` was renamed to `hickory-dns` in 2024. The codebase doesn't currently use DNS resolution crates directly, but if adding service discovery, use `hickory-dns`.

---

## Security & Crypto

| Library | Replaces | Savings | Why |
|---------|----------|---------|-----|
| **`jsonwebtoken`** (already have) | — | — | Keep, but add **`jwk-authenticate`** |
| **`pasetors`** | JWT | ~200 LOC | PASETO: crypto-agile tokens |
| **`secrecy`** | String secrets | ~150 LOC | Zero-on-drop, redacted Debug |
| **`rust-argon2`** (already have) | — | — | Keep |
| **`magic-crypt`** | Custom encryption | ~300 LOC | AES-GCM-SIV, misuse-resistant |
| **`zeroize`** | Manual secret clearing | ~100 LOC | Secure memory wiping |

---

## CLI & UX

| Library | Replaces | Savings | Why |
|---------|----------|---------|-----|
| **`inquire`** | dialoguer | ~200 LOC | Better interactive prompts (already using dialoguer) |
| **`spinoff`** | indicatif | ~150 LOC | Simpler spinners |
| **`color-eyre`** | anyhow | ~100 LOC | Beautiful error reports |
| **`tracing-appender`** | File logging | ~200 LOC | Non-blocking log writing |
| **`tracing-opentelemetry`** | Custom tracing | ~400 LOC | OTel/Jaeger export |
| **`ratatui`** | Static output | ~800 LOC | TUI dashboards for `shellwego top` |
| **`comfy-table`** (already have) | manual tables | ~200 LOC | Keep - excellent for CLI output |

**Update**: The codebase uses `comfy-table` and `dialoguer` - these are solid. Consider `inquire` only if you need more advanced interactive features.

---

## Observability

| Library | Replaces | Savings | Why |
|---------|----------|---------|-----|
| **`metrics`** + **`metrics-prometheus`** | Custom metrics | ~600 LOC | Unified metrics facade |
| **`tracing-flame`** | Profiling | ~300 LOC | Flamegraph generation |
| **`opentelemetry-rust`** | APM integration | ~500 LOC | Traces/metrics/logs correlation |
| **`pyroscope-rs`** | Continuous profiling | ~400 LOC | Production flamegraphs |
| **`tracing`** (already have) | — | — | Standard, keep it |

---

## Testing & DevEx

| Library | Replaces | Savings | Why |
|---------|----------|---------|-----|
| **`fake`** | Test fixtures | ~400 LOC | Fake data generation |
| **`insta`** | Snapshot tests | ~600 LOC | Inline snapshot testing |
| **`proptest`** | Property tests | ~800 LOC | Fuzzing-style testing |
| **`mockall`** | Manual mocks | ~1000 LOC | Mock generation |
| **`criterion`** | Custom benches | ~300 LOC | Statistical benchmarking |
| **`tempfile`** | Test dirs | ~100 LOC | Already using - keep it |
| **`assert_cmd`** + **`predicates`** | CLI testing | ~300 LOC | Already in dev-dependencies |

---

## Configuration & Environment

| Library | Replaces | Savings | Why |
|---------|----------|---------|-----|
| **`config`** (already have) | Manual env parsing | ~400 LOC | Hierarchical config (keep) |
| **`figment`** | `config` crate | ~200 LOC | More flexible, type-safe |
| **`dotenvy`** | `dotenv` | ~50 LOC | `dotenv` is unmaintained |
| **`envy`** | Manual env var parsing | ~150 LOC | Deserialize env vars to structs |

**Critical Update**: `dotenv` is deprecated/unmaintained. If using `.env` files, switch to `dotenvy`.

---

## Serialization

| Library | Replaces | Savings | Why |
|---------|----------|---------|-----|
| **`serde`** (already have) | — | — | Standard |
| **`serde_json`** (already have) | — | — | Standard |
| **`simd-json`** | `serde_json` | ~100 LOC | SIMD-accelerated JSON parsing (x86_64) |
| **`rkyv`** | Bincode for caching | ~200 LOC | Zero-copy deserialization |

**Note**: `serde_yaml` is deprecated. The codebase uses `toml` which is maintained. For YAML needs, consider `serde_yml` (community fork) or avoid YAML.

---

## Total Impact

| Category | Est. LOC Saved | Complexity Reduction |
|----------|---------------|----------------------|
| Database (Sea-ORM) | 2,000 | Massive |
| VMM (firecracker-rs) | 1,500 | Critical |
| Testing (mockall/proptest) | 1,800 | High |
| Networking (quinn/netlink) | 1,400 | Medium |
| Observability | 1,300 | Medium |
| Web (aide/garde) | 1,200 | High |
| **TOTAL** | **~9,200 LOC** | **Dramatic** |

---

## Critical Deprecation Updates (2024-2025)

| Deprecated | Replacement | Action Required |
|------------|-------------|-----------------|
| `dotenv` | `dotenvy` | Replace in CLI tools |
| `serde_yaml` | `serde_yml` or avoid YAML | Only if YAML needed |
| `trust-dns` | `hickory-dns` | Update if used for DNS |
| `time` 0.2 | `time` 0.3+ | Already using chrono, good |
| `lazy_static` | `std::sync::LazyLock` (Rust 1.80+) | Modern Rust native |

---

## Recommended Immediate Adds (Updated for 2025)

```toml
# In workspace.dependencies - additions for 2025
secrecy = "0.10"          # Secret handling (updated)
dashmap = "6.0"           # Concurrent maps (updated)
deadpool = "0.12"         # Connection pooling (updated)
aide = "0.13"             # OpenAPI without macros (updated)
garde = "0.20"            # Validation (updated)
color-eyre = "0.6"        # Pretty errors
metrics = "0.24"          # Metrics facade (updated)
ratatui = "0.29"          # TUI for CLI (updated)
quinn = "0.11"            # QUIC protocol (updated)
hickory-dns = "0.24"      # DNS resolver (trust-dns successor)
dotenvy = "0.15"          # Environment files (replaces dotenv)
sled = "0.34"             # Embedded KV store
```

## Biggest Wins (Updated)

1. ~~**`sea-orm`** → Deletes entire `db/` module, gives migrations/relations free (control plane would benefit most)~~ ✅ **COMPLETED**
2. ~~**`firecracker-rs`** → Deletes `vmm/driver.rs`, official AWS SDK (critical for agent crate)~~ ✅ **COMPLETED**
3. **`aide`** → Deletes `api/docs.rs`, derive-free OpenAPI (control plane API cleanup)
4. **`quinn`** → Replaces NATS for CP<->Agent, zero external deps (if you want to drop NATS dependency)
5. **`ratatui`** → `shellwego top` as beautiful TUI instead of polling API (CLI enhancement)
6. **`hickory-dns`** → Service discovery without external dependencies (if adding DNS-based discovery)

## Current Codebase Analysis Notes

**What's already excellent:**
- `axum` 0.7 + `tower-http` - Modern, maintained
- `sqlx` - Compile-time checked queries
- `tokio` with full features - Industry standard
- `utoipa` - OpenAPI generation (consider `aide` only if macro overhead becomes issue)
- `clap` 4.x - Latest derive features
- `tracing` + `tracing-subscriber` - Standard observability

**Specific recommendations for ShellWeGo:**

1. **Agent crate**: Add `secrecy` for the `join_token` and any API keys in `AgentConfig`
2. **Control plane**: Consider `sea-orm` for the complex entity relationships in `entities/` 
3. **Network crate**: `quinn` could replace the need for NATS in some mesh scenarios
4. **CLI crate**: `ratatui` for a `top`-like interface showing node/resource status
5. **Storage crate**: `zstd` for snapshot compression (already noted in comments)
6. **All crates**: Replace any `dotenv` usage with `dotenvy` (check transitive deps)
```