# Schema Consolidation Plan: Eliminating DRY Violations

## Executive Summary

This plan addresses the DRY (Don't Repeat Yourself) violations in the ShellWeGo backend monorepo where types and interfaces are duplicated across multiple crates instead of being centralized in a single schema crate.

**Status**: Draft
**Author**: Architecture Review
**Date**: 2025-01-20
**Priority**: High

---

## 1. Problem Analysis

### 1.1 Current State

The codebase uses `shellwego-core` as the "shared kernel" for domain entities. However, several crates define their own types locally, violating DRY principles and creating maintenance burden.

**Current `shellwego-core` definition** (from `lib.rs`):
```rust
//! The shared kernel. All domain entities and common types live here.
//! No business logic, just pure data structures and validation.
```

### 1.2 Identified DRY Violations

| Crate | File | Types Defined Locally | Should Be In Schema |
|-------|------|----------------------|---------------------|
| `shellwego-agent` | `src/lib.rs` | `VirtualizationMode`, `AgentConfig`, `Capabilities` | ✅ Domain types |
| `shellwego-agent` | `src/vmm/mod.rs` | `MicrovmSummary`, `MicrovmState` | ✅ Domain types |
| `shellwego-agent` | `src/vmm/config.rs` | `MicrovmConfig`, `DriveConfig`, `NetworkInterface`, `MicrovmMetrics` | ✅ Domain types |
| `shellwego-network` | `src/lib.rs` | `NetworkConfig`, `NetworkSetup`, `NetworkError` | ✅ Domain types |
| `shellwego-network` | `src/quinn/mod.rs` | `Message`, `QuicConfig`, `AgentConnection` | ✅ Domain types |
| `shellwego-control-plane` | `src/api/handlers/apps.rs` | `ListAppsQuery`, `ScaleRequest` | ✅ API request types |
| `shellwego-firecracker` | `src/models.rs` | ~50+ Firecracker API types | ⚠️ External API types |

### 1.3 Why This Matters

1. **Maintenance Burden**: Changes to shared concepts require updates in multiple files
2. **Type Mismatches**: Risk of divergent definitions causing subtle bugs
3. **Documentation Fragmentation**: Types documented in multiple places
4. **Import Confusion**: Developers unsure which type to use
5. **API Contract Risk**: Wire format could differ between control-plane and agent

---

## 2. Proposed Solution: Rename and Expand `shellwego-core` → `shellwego-schema`

### 2.1 Rationale for Renaming

The name `shellwego-core` implies:
- Contains business logic (common pattern: "core" = domain logic)
- Is a runtime dependency with behavior
- Might have its own state/lifecycle

The name `shellwego-schema` communicates:
- **Pure data structures** - no business logic
- **Wire format definitions** - API contracts
- **Type definitions** - shared vocabulary
- **Schema-first design** - like protobuf/GraphQL schemas

This aligns with industry patterns:
- `types` package in TypeScript monorepos
- `schema` module in GraphQL services
- `.proto` files in gRPC services

### 2.2 Proposed Schema Crate Structure

```
crates/shellwego-schema/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Crate root with re-exports
│   ├── prelude.rs                # Common imports
│   ├── error.rs                  # SchemaError type
│   │
│   ├── entities/                 # Domain entities (current core)
│   │   ├── mod.rs
│   │   ├── app.rs                # App, AppStatus, ResourceSpec, etc.
│   │   ├── app_instance.rs       # AppInstance, InstanceStatus
│   │   ├── node.rs               # Node, NodeStatus, NodeCapacity
│   │   ├── database.rs           # Database, DatabaseEngine
│   │   ├── domain.rs             # Domain, DomainStatus
│   │   ├── volume.rs             # Volume, VolumeStatus, Snapshot
│   │   ├── secret.rs             # Secret, SecretType
│   │   ├── organization.rs       # Organization
│   │   ├── user.rs               # User, UserRole
│   │   ├── deployment.rs         # Deployment, DeploymentStatus
│   │   ├── backup.rs             # Backup, BackupStatus
│   │   ├── webhook.rs            # Webhook, WebhookDelivery
│   │   ├── audit.rs              # AuditLog
│   │   └── metrics.rs            # MetricPoint, MetricSeries
│   │
│   ├── vmm/                      # NEW: Virtual Machine Manager types
│   │   ├── mod.rs
│   │   ├── config.rs             # MicrovmConfig, DriveConfig
│   │   ├── state.rs              # MicrovmState, MicrovmSummary
│   │   ├── metrics.rs            # MicrovmMetrics
│   │   └── virtualization.rs     # VirtualizationMode, Capabilities
│   │
│   ├── network/                  # NEW: Network types
│   │   ├── mod.rs
│   │   ├── config.rs             # NetworkConfig, NetworkSetup
│   │   ├── quinn.rs              # QuicConfig, Message types
│   │   ├── wireguard.rs          # WireGuard types
│   │   └── error.rs              # NetworkError
│   │
│   ├── api/                      # NEW: API request/response types
│   │   ├── mod.rs
│   │   ├── apps.rs               # ListAppsQuery, ScaleRequest
│   │   ├── nodes.rs              # ListNodeQuery, etc.
│   │   ├── pagination.rs         # PaginationParams, Cursor
│   │   └── responses.rs          # ApiResponse<T>, ErrorResponse
│   │
│   ├── firecracker/              # NEW: Firecracker API models
│   │   ├── mod.rs
│   │   ├── instance.rs           # InstanceInfo, InstanceState
│   │   ├── machine.rs            # MachineConfiguration, CpuTemplate
│   │   ├── drives.rs             # Drive, PartialDrive, CacheType
│   │   ├── network.rs            # NetworkInterface, RateLimiter
│   │   ├── snapshot.rs           # SnapshotCreateParams, SnapshotLoadParams
│   │   └── metrics.rs            # FirecrackerMetrics, VmmMetrics
│   │
│   └── agent/                    # NEW: Agent configuration types
│       ├── mod.rs
│       ├── config.rs             # AgentConfig (serialize/deserialize only)
│       └── capabilities.rs       # Capabilities, detection results
```

---

## 3. Migration Strategy

### Phase 1: Rename and Prepare (Week 1)

**Tasks:**
1. Rename `shellwego-core` to `shellwego-schema`
2. Update all `use shellwego_core::` to `use shellwego_schema::`
3. Update Cargo.toml dependencies across all crates
4. Create new submodule structure (empty initially)
5. Add deprecation aliases for backward compatibility

**Commands:**
```bash
# Rename the crate
mv crates/shellwego-core crates/shellwego-schema

# Update Cargo.toml in schema crate
sed -i 's/name = "shellwego-core"/name = "shellwego-schema"/' crates/shellwego-schema/Cargo.toml

# Update all imports
find crates -name "*.rs" -exec sed -i 's/shellwego_core/shellwego_schema/g' {} \;
```

### Phase 2: Consolidate Agent Types (Week 2)

**Move from `shellwego-agent/src/lib.rs`:**
```rust
// FROM: shellwego-agent/src/lib.rs
pub enum VirtualizationMode { Kvm, Pvm, Wasm }
pub struct AgentConfig { ... }
pub struct Capabilities { ... }

// TO: shellwego-schema/src/vmm/virtualization.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
pub enum VirtualizationMode {
    /// KVM hardware virtualization (fastest, requires /dev/kvm)
    Kvm,
    /// PVM software virtualization (universal, no KVM required)
    Pvm,
    /// WASM runtime (lightest, for functions only)
    Wasm,
}

// TO: shellwego-schema/src/agent/config.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
pub struct AgentConfig {
    pub node_id: Option<Uuid>,
    pub control_plane_url: String,
    // ... all fields from original
}
```

**Update `shellwego-agent/src/lib.rs`:**
```rust
// After migration
pub use shellwego_schema::{
    vmm::{VirtualizationMode, MicrovmConfig, MicrovmState},
    agent::{AgentConfig, Capabilities},
};

// Keep only agent-specific business logic
pub mod daemon;
pub mod reconciler;
pub mod discovery;

// Re-export for convenience
pub use shellwego_schema::vmm::VirtualizationMode;
```

### Phase 3: Consolidate VMM Types (Week 2-3)

**Move from `shellwego-agent/src/vmm/`:**
```rust
// TO: shellwego-schema/src/vmm/config.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrovmConfig {
    pub app_id: Uuid,
    pub vm_id: Uuid,
    pub memory_mb: u64,
    pub cpu_shares: u64,
    pub kernel_path: PathBuf,
    pub kernel_boot_args: String,
    pub drives: Vec<DriveConfig>,
    pub network_interfaces: Vec<NetworkInterface>,
    pub vsock_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveConfig {
    pub drive_id: String,
    pub path_on_host: PathBuf,
    pub is_root_device: bool,
    pub is_read_only: bool,
    pub rate_limiter: Option<RateLimiterConfig>,
}

// TO: shellwego-schema/src/vmm/state.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MicrovmState {
    Uninitialized,
    Configured,
    Running,
    Paused,
    Halted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrovmSummary {
    pub app_id: Uuid,
    pub vm_id: Uuid,
    pub state: MicrovmState,
    pub started_at: DateTime<Utc>,
}
```

### Phase 4: Consolidate Network Types (Week 3)

**Move from `shellwego-network/src/lib.rs`:**
```rust
// TO: shellwego-schema/src/network/config.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub app_id: Uuid,
    pub vm_id: Uuid,
    pub bridge_name: String,
    pub tap_name: String,
    pub guest_mac: String,
    pub guest_ip: Ipv4Addr,
    pub host_ip: Ipv4Addr,
    pub subnet: Ipv4Network,
    pub gateway: Ipv4Addr,
    pub mtu: u16,
    pub bandwidth_limit_mbps: Option<u32>,
}

// TO: shellwego-schema/src/network/error.rs
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum NetworkError {
    #[error("Interface not found: {0}")]
    InterfaceNotFound(String),
    // ... all variants
}
```

### Phase 5: Consolidate API Types (Week 3-4)

**Move from `shellwego-control-plane/src/api/handlers/`:**
```rust
// TO: shellwego-schema/src/api/apps.rs
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
pub struct ListAppsQuery {
    pub organization_id: Option<Uuid>,
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
pub struct ScaleRequest {
    pub replicas: u32,
}

// TO: shellwego-schema/src/api/pagination.rs
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
```

### Phase 6: Firecracker Models (Week 4)

**Decision Required:** Should Firecracker API models be in schema?

**Option A: Include in schema** (Recommended)
- Single source of truth for all types
- Agent and firecracker crate share definitions
- Simpler imports

**Option B: Keep separate**
- Firecracker models are external API, not our domain
- Could be regenerated from Firecracker OpenAPI spec
- Separates concerns

**Recommendation:** Include in schema under `firecracker` module with clear documentation that these model the external Firecracker API.

```rust
// TO: shellwego-schema/src/firecracker/mod.rs
//! Firecracker API Models
//!
//! Generated from Firecracker API specification v1.16.0-dev
//! These types model the external Firecracker microVM API.
//! Changes to Firecracker API require updating these types.

pub mod instance;
pub mod machine;
pub mod drives;
pub mod network;
pub mod snapshot;
pub mod metrics;
```

---

## 4. Implementation Checklist

### 4.1 Per-Type Migration Checklist

For each type being moved:

- [ ] Create destination file in schema crate
- [ ] Copy type definition with all documentation
- [ ] Add `Serialize, Deserialize` derives if not present
- [ ] Add OpenAPI schema derives behind feature flag
- [ ] Add ORM derives behind feature flag (if applicable)
- [ ] Update imports in source crate to re-export from schema
- [ ] Update any dependent crates
- [ ] Move or duplicate tests (keep unit tests in source crate)
- [ ] Update documentation

### 4.2 Feature Flags

Maintain the existing feature flag pattern:

```toml
# shellwego-schema/Cargo.toml
[features]
default = []
orm = ["sea-orm", "sea-query"]
openapi = ["schemars", "utoipa"]
```

### 4.3 Breaking Changes Strategy

For downstream consumers:

```rust
// shellwego-schema/src/lib.rs
// Backward compatibility re-exports (deprecated)
#[deprecated(since = "0.2.0", note = "Use shellwego_schema::vmm::VirtualizationMode")]
pub use vmm::VirtualizationMode;
```

---

## 5. Dependency Graph After Migration

```
                    ┌─────────────────────┐
                    │   shellwego-schema  │
                    │   (pure types)      │
                    └──────────┬──────────┘
                               │
       ┌───────────────────────┼───────────────────────┐
       │                       │                       │
       ▼                       ▼                       ▼
┌──────────────┐    ┌──────────────────┐    ┌─────────────────┐
│ shellwego-   │    │ shellwego-       │    │ shellwego-      │
│ agent        │    │ control-plane    │    │ cli             │
│ (logic)      │    │ (API + services) │    │ (client)        │
└──────────────┘    └──────────────────┘    └─────────────────┘
       │                       │
       ▼                       ▼
┌──────────────┐    ┌──────────────────┐
│ shellwego-   │    │ shellwego-       │
│ network      │    │ firecracker      │
│ (impl)       │    │ (client)         │
└──────────────┘    └──────────────────┘
```

**Key principle:** `shellwego-schema` has NO dependencies on other ShellWeGo crates. It only depends on:
- `serde` (serialization)
- `uuid` (identifiers)
- `chrono` (timestamps)
- `sea-orm` (optional, for ORM feature)
- `schemars` (optional, for OpenAPI feature)

---

## 6. Testing Strategy

### 6.1 Compatibility Tests

After migration, verify:

```rust
// tests/schema_compatibility.rs

/// Verify serialization compatibility between old and new locations
#[test]
fn test_virtualization_mode_compatibility() {
    let mode = VirtualizationMode::Pvm;
    let json = serde_json::to_string(&mode).unwrap();
    
    // Should deserialize correctly from both locations
    let from_schema: shellwego_schema::vmm::VirtualizationMode = 
        serde_json::from_str(&json).unwrap();
    assert_eq!(from_schema, shellwego_schema::vmm::VirtualizationMode::Pvm);
}
```

### 6.2 Round-Trip Tests

All moved types must pass round-trip serialization:

```rust
#[test]
fn test_microvm_config_roundtrip() {
    let config = MicrovmConfig {
        app_id: Uuid::new_v4(),
        vm_id: Uuid::new_v4(),
        memory_mb: 256,
        // ...
    };
    
    let json = serde_json::to_string(&config).unwrap();
    let decoded: MicrovmConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config, decoded);
}
```

---

## 7. Documentation Updates

### 7.1 README Updates

Each crate README should document its relationship to schema:

```markdown
## Dependencies

This crate depends on `shellwego-schema` for:
- Domain entity types (`App`, `Node`, `Volume`, etc.)
- VMM configuration types (`MicrovmConfig`, `VirtualizationMode`)
- Network types (`NetworkConfig`, `NetworkError`)

Business logic is implemented in this crate; schema contains only data structures.
```

### 7.2 Architecture Documentation

Create `docs/architecture/schema.md`:

```markdown
# Schema Crate Architecture

The `shellwego-schema` crate is the single source of truth for all
type definitions in the ShellWeGo platform.

## Design Principles

1. **Pure Data**: No business logic, only data structures
2. **Wire Format**: Types define API contracts between services
3. **Feature Flags**: Optional derives for ORM, OpenAPI
4. **Zero Runtime Dependencies**: No IO, no state, no side effects

## Module Organization

| Module | Purpose |
|--------|---------|
| `entities` | Domain entities (App, Node, etc.) |
| `vmm` | Virtual machine manager types |
| `network` | Network configuration types |
| `api` | API request/response types |
| `firecracker` | External Firecracker API models |
| `agent` | Agent configuration types |
```

---

## 8. Rollback Plan

If issues arise:

1. **Phase 1-2 rollback**: Revert rename, restore `shellwego-core`
2. **Phase 3+ rollback**: Keep renamed crate, remove new modules
3. **Partial rollback**: Individual types can be moved back

All changes should be done in feature branches with thorough testing before merging.

---

## 9. Success Criteria

Migration is complete when:

- [ ] `shellwego-core` is renamed to `shellwego-schema`
- [ ] All types from the violation table are consolidated
- [ ] No duplicate type definitions exist across crates
- [ ] All tests pass
- [ ] Documentation is updated
- [ ] No breaking changes to public API
- [ ] CI/CD pipeline is green

---

## 10. Timeline

| Week | Phase | Owner | Status |
|------|-------|-------|--------|
| 1 | Rename and Prepare | Platform Team | Not Started |
| 2 | Agent Types Consolidation | Agent Team | Not Started |
| 2-3 | VMM Types Consolidation | Agent Team | Not Started |
| 3 | Network Types Consolidation | Network Team | Not Started |
| 3-4 | API Types Consolidation | API Team | Not Started |
| 4 | Firecracker Models Decision | Platform Team | Not Started |
| 4+ | Documentation & Cleanup | All Teams | Not Started |

---

## 11. Appendix: Type Inventory

### A. Current `shellwego-core` Entities

| Type | Location | ORM Support | OpenAPI |
|------|----------|-------------|---------|
| `App` | `entities/app.rs` | ✅ | ✅ |
| `AppInstance` | `entities/app_instance.rs` | ✅ | ✅ |
| `Node` | `entities/node.rs` | ✅ | ✅ |
| `Database` | `entities/database.rs` | ✅ | ✅ |
| `Domain` | `entities/domain.rs` | ✅ | ✅ |
| `Volume` | `entities/volume.rs` | ✅ | ✅ |
| `Secret` | `entities/secret.rs` | ✅ | ✅ |

### B. Types to Migrate

| Type | Current Location | Target Location |
|------|------------------|-----------------|
| `VirtualizationMode` | `agent/src/lib.rs` | `schema/src/vmm/virtualization.rs` |
| `AgentConfig` | `agent/src/lib.rs` | `schema/src/agent/config.rs` |
| `Capabilities` | `agent/src/lib.rs` | `schema/src/agent/capabilities.rs` |
| `MicrovmConfig` | `agent/src/vmm/config.rs` | `schema/src/vmm/config.rs` |
| `MicrovmState` | `agent/src/vmm/config.rs` | `schema/src/vmm/state.rs` |
| `MicrovmSummary` | `agent/src/vmm/mod.rs` | `schema/src/vmm/state.rs` |
| `DriveConfig` | `agent/src/vmm/config.rs` | `schema/src/vmm/config.rs` |
| `NetworkInterface` | `agent/src/vmm/config.rs` | `schema/src/vmm/config.rs` |
| `NetworkConfig` | `network/src/lib.rs` | `schema/src/network/config.rs` |
| `NetworkSetup` | `network/src/lib.rs` | `schema/src/network/config.rs` |
| `NetworkError` | `network/src/lib.rs` | `schema/src/network/error.rs` |
| `ListAppsQuery` | `control-plane/src/api/handlers/apps.rs` | `schema/src/api/apps.rs` |
| `ScaleRequest` | `control-plane/src/api/handlers/apps.rs` | `schema/src/api/apps.rs` |

---

## 12. Related Documents

- [Entity Schizophrenia](./EntitySchizophrenia.md) - Related issue on entity duplication
- [Codebase "Bit Rot" in Scaffolding](./changes/Codebase "Bit Rot" in Scaffolding.md)
- [REST API Endpoints](./rest.endpoints.md)

---

*End of Plan Document*
