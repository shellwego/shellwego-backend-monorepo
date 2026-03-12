# Schema Global Adoption Plan: Eliminating DRY Violations

## Executive Summary

This plan addresses the critical DRY (Don't Repeat Yourself) violations in the ShellWeGo backend monorepo where `shellwego-schema` exists but is NOT being used as the single source of truth for type definitions. Multiple crates define their own types locally, creating maintenance burden, type mismatch risks, and documentation fragmentation.

**Status**: Phase 1, 2, 3, 4 & 5 Complete - Schema Types Consolidated
**Author**: Architecture Review
**Date**: 2025-01-20
**Last Updated**: 2025-01-22
**Priority**: Critical
**Supersedes**: `schema-consolidation.plan.md` (which discussed renaming `shellwego-core` - now complete)

### Migration Progress

| Phase | Status | Description |
|-------|--------|-------------|
| Phase 1 | ✅ **COMPLETE** | Firecracker Models moved to `schema/src/firecracker/` |
| Phase 2 | ✅ **COMPLETE** | Agent Types consolidated to schema |
| Phase 3 | ✅ **COMPLETE** | Control Plane Handlers use schema types |
| Phase 4 | ✅ **COMPLETE** | Network Consolidation |
| Phase 5 | ✅ **COMPLETE** | Registry/Storage OCI types moved to `schema/src/oci/` |
| Phase 6 | ⏳ Pending | Billing Entities |

---

## 1. Current State Analysis

### 1.1 The Schema Crate Exists But Is Underutilized

The `shellwego-schema` crate is well-organized with proper module structure:

```
crates/shellwego-schema/src/
├── lib.rs              # Re-exports commonly used types
├── prelude.rs          # Common imports
├── error.rs            # CoreError, CoreResult
├── entities/           # Domain entities (App, Node, Volume, etc.)
├── vmm/                # VMM types (MicrovmConfig, DriveConfig, etc.)
├── network/            # Network types (NetworkConfig, QuicConfig, etc.)
├── api/                # API request/response types
├── agent/              # Agent configuration types
└── firecracker/        # ✅ NEW: Firecracker API models (Phase 1 complete)
    ├── mod.rs          # Module exports
    ├── instance.rs     # InstanceInfo, InstanceState, FirecrackerVersion
    ├── boot.rs         # BootSource
    ├── machine.rs      # MachineConfiguration, CpuTemplate, HugePages, CpuConfig
    ├── drives.rs       # Drive, PartialDrive, CacheType, IoEngine, Pmem
    ├── network.rs      # NetworkInterface, PartialNetworkInterface, RateLimiter, TokenBucket
    ├── balloon.rs      # Balloon, BalloonUpdate, BalloonStats
    ├── devices.rs      # Vsock, EntropyDevice, SerialDevice
    ├── logging.rs      # Logger, LogLevel, Metrics
    ├── metrics.rs      # FirecrackerMetrics, VmmMetrics, NetMetrics, BlockMetrics
    ├── actions.rs      # InstanceActionInfo, ActionType, Vm, VmState
    ├── snapshot.rs     # SnapshotCreateParams, SnapshotLoadParams, SnapshotType
    ├── memory.rs       # MemoryHotplugConfig, MemoryHotplugSizeUpdate
    ├── mmds.rs         # MmdsConfig, MmdsVersion
    ├── full_config.rs  # FullVmConfiguration
    └── error.rs        # Error
```

**Phase 1 Migration Complete**: All Firecracker models have been moved from `shellwego-firecracker/src/models.rs` to `shellwego-schema/src/firecracker/`. The firecracker crate now re-exports these types from schema.

### 1.2 Architecture Principle Violation

**Expected dependency flow:**
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
└──────────────┘    └──────────────────┘    └─────────────────┘
```

**Current broken flow:**
- Crates define local types that should be in schema
- No single source of truth
- Type definitions scattered across the codebase

---

## 2. Comprehensive Violation Inventory

### 2.1 Critical Violations (Same Type Name, Different Location)

| Type | Schema Location | Duplicate Location | Severity | Status |
|------|-----------------|-------------------|----------|--------|
| `NetworkInterface` | `schema/src/firecracker/network.rs` | ~~`firecracker/src/models.rs`~~ | **CRITICAL** | ✅ Resolved |
| `RateLimiter` / `TokenBucket` | `schema/src/firecracker/network.rs` | ~~`firecracker/src/models.rs`~~ | **HIGH** | ✅ Resolved |
| `SnapshotType` | `schema/src/firecracker/snapshot.rs` | ~~`firecracker/src/models.rs`~~ | **HIGH** | ✅ Resolved |
| `AgentConnection` | `schema/src/network/quinn.rs` | ~~`control-plane/src/state.rs`~~ | **CRITICAL** | ✅ Resolved |
| `WasmConfig` | `schema/src/agent/wasm.rs` (as `WasmRuntimeConfig`) | ~~`agent/src/wasm/mod.rs`~~ | **CRITICAL** | ✅ Resolved |
| `SnapshotType` (Agent) | `schema/src/agent/snapshot.rs` (as `AgentSnapshotType`) | ~~`agent/src/snapshot.rs`~~ | **HIGH** | ✅ Resolved |
| `SnapshotInfo` | `schema/src/agent/snapshot.rs` (as `AgentSnapshotInfo`) | ~~`agent/src/snapshot.rs`~~ | **HIGH** | ✅ Resolved |
| `DesiredState` | `schema/src/agent/desired_state.rs` | ~~`agent/src/daemon.rs`~~ | **HIGH** | ✅ Resolved |
| `DesiredApp` | `schema/src/agent/desired_state.rs` | ~~`agent/src/daemon.rs`~~ | **HIGH** | ✅ Resolved |
| `ErrorResponse` | `schema/src/api/responses.rs` | `control-plane/src/api/mod.rs` | **HIGH** | ⏳ Pending |

### 2.2 Entity Redefinitions in Control Plane

The `control-plane/src/api/handlers.rs` redefines entities that already exist in `shellwego-schema/src/entities/`:

| Schema Entity | Schema Location | Duplicate Location | Missing Fields |
|---------------|-----------------|-------------------|----------------|
| `App` | `entities/app.rs` | `api/handlers.rs:48-55` | Missing: slug, resources, env, domains, volumes, health_check, source, etc. |
| `Node` | `entities/node.rs` | `api/handlers.rs:152-158` | Missing: capacity, capabilities, network, labels, running_apps, etc. |
| `Volume` | `entities/volume.rs` | `api/handlers.rs:209-215` | Missing: size_bytes, volume_type, filesystem, snapshots, backup_policy, etc. |
| `Domain` | `entities/domain.rs` | `api/handlers.rs:269-275` | Missing: tls_status, dns_validation, routing_config, edge_features, etc. |
| `Database` | `entities/database.rs` | `api/handlers.rs:330-336` | Missing: version, endpoint, resources, usage, ha, backup_config, etc. |
| `Secret` | `entities/secret.rs` | `api/handlers.rs:390-395` | Missing: scope, app_id, current_version, versions, last_used_at, etc. |

### 2.3 API Request/Response Duplications

| Type | Schema Location | Duplicate Location |
|------|-----------------|-------------------|
| `ListAppsQuery` | `api/apps.rs` | `control-plane/api/handlers.rs:66-73` |
| `ListNodesQuery` | `api/apps.rs` | `control-plane/api/handlers.rs:167-170` |
| `CreateAppRequest` | `entities/app.rs` | `control-plane/api/handlers.rs:57-64` |
| `CreateVolumeRequest` | `entities/volume.rs` | `control-plane/api/handlers.rs:217-222` |
| `CreateDomainRequest` | `entities/domain.rs` | `control-plane/api/handlers.rs:277-283` |
| `CreateDatabaseRequest` | `entities/database.rs` | `control-plane/api/handlers.rs:338-343` |
| `CreateSecretRequest` | `entities/secret.rs` | `control-plane/api/handlers.rs:397-404` |
| `HealthResponse` | `api/responses.rs` | `control-plane/api/handlers.rs:18-22` |

### 2.4 Firecracker Models: External API Types

The `shellwego-firecracker/src/models.rs` contains 50+ types modeling the external Firecracker API. These include:

- `InstanceInfo`, `InstanceState`
- `BootSource`, `MachineConfiguration`, `CpuConfig`
- `Drive`, `PartialDrive`, `Pmem`
- `NetworkInterface`, `RateLimiter`, `TokenBucket`
- `Balloon`, `BalloonStats`, `Vsock`, `EntropyDevice`
- `Logger`, `Metrics`, `FirecrackerMetrics`
- `SnapshotCreateParams`, `SnapshotLoadParams`
- `MemoryHotplugConfig`, `MmdsConfig`

**Decision Required:** Should these be moved to schema?

**Recommendation:** Move to `shellwego-schema/src/firecracker/` because:
1. Agent crate needs these types for VMM management
2. Control plane needs them for snapshot/migration features
3. Having them separate creates import confusion
4. The schema crate is designed to handle external API wire formats

### 2.5 Registry/Storage Duplications

| Type | Location 1 | Location 2 | Issue |
|------|-----------|-----------|-------|
| `Manifest` | `registry/src/lib.rs` | `storage/src/oci.rs` | Same concept, different definitions |
| `Platform` | `registry/src/lib.rs` | `storage/src/oci.rs` | Same concept, different definitions |
| `Descriptor` | `registry/src/cache.rs` | `registry/src/lib.rs` | Similar structure |

### 2.6 Billing Types

The `shellwego-billing` crate defines types that could be domain entities:

- `Customer`, `Address`, `UsageEvent`, `UsageSummary`
- `Invoice`, `LineItem`, `BillingPeriod`, `PaymentResult`
- `BillingConfig`, `DunningConfig`

**Recommendation:** Move `Customer`, `BillingPeriod`, and shared types to schema. Keep billing-specific logic types in billing crate.

### 2.7 Observability Types

Types in `shellwego-observability` that could be shared:
- `TracingConfig`, `LogConfig`, `MetricsConfig`
- `LogEntry`, `LogLevel` (duplicates `firecracker::LogLevel`)
- `HealthStatus` (similar to `api/responses::ServiceStatus`)

---

## 3. Migration Strategy

### Phase 1: Firecracker Models ✅ COMPLETE

**Objective:** Move all Firecracker API models to schema.

**Status:** Completed on 2025-01-21

**Completed Tasks:**
- [x] Create `shellwego-schema/src/firecracker/` module structure
- [x] Move all types from `shellwego-firecracker/src/models.rs` to schema
- [x] Add proper `cfg_attr` for OpenAPI derives
- [x] Update `shellwego-firecracker` to re-export from schema
- [x] Delete original `models.rs` file
- [x] Update Cargo.toml dependencies

**Implemented File Structure:**
```
shellwego-schema/src/firecracker/
├── mod.rs              # Module exports
├── instance.rs         # InstanceInfo, InstanceState, FirecrackerVersion
├── boot.rs             # BootSource
├── machine.rs          # MachineConfiguration, CpuTemplate, HugePages, CpuConfig
├── drives.rs           # Drive, PartialDrive, CacheType, IoEngine, Pmem
├── network.rs          # NetworkInterface, PartialNetworkInterface, RateLimiter, TokenBucket
├── balloon.rs          # Balloon, BalloonUpdate, BalloonStats, BalloonStatsUpdate
├── devices.rs          # Vsock, EntropyDevice, SerialDevice
├── logging.rs          # Logger, LogLevel, Metrics
├── metrics.rs          # FirecrackerMetrics, VmmMetrics, NetMetrics, BlockMetrics
├── actions.rs          # InstanceActionInfo, ActionType, Vm, VmState
├── snapshot.rs         # SnapshotCreateParams, SnapshotLoadParams, SnapshotType
├── memory.rs           # MemoryHotplugConfig, MemoryHotplugSizeUpdate, MemoryHotplugStatus
├── mmds.rs             # MmdsConfig, MmdsVersion
├── full_config.rs      # FullVmConfiguration
└── error.rs            # Error
```

**Key Changes Made:**
- `shellwego-firecracker/src/lib.rs` now re-exports types from `shellwego-schema`
- `shellwego-firecracker/src/models.rs` has been DELETED
- `shellwego-firecracker/Cargo.toml` now depends on `shellwego-schema`
- All Firecracker models now have proper OpenAPI schema derives

### Phase 2: Consolidate Agent Types ✅ COMPLETE

**Objective:** Remove all local type definitions in agent crate.

**Status:** Completed on 2025-01-21

**Completed Tasks:**
- [x] Create `shellwego-schema/src/agent/desired_state.rs` module
- [x] Move `DesiredState`, `DesiredApp`, `DesiredVolume`, `VolumeMount` types to schema
- [x] Update `agent/src/wasm/mod.rs` to use `WasmRuntimeConfig`, `WasmRuntimeStats`, `WasmExitStatus` from schema
- [x] Update `agent/src/snapshot.rs` to use `AgentSnapshotType`, `AgentSnapshotInfo` from schema
- [x] Update `agent/src/daemon.rs` to import desired state types from schema
- [x] Update `agent/src/lib.rs` to re-export new types from schema

**Implemented File Structure:**
```
shellwego-schema/src/agent/
├── mod.rs              # Module exports
├── config.rs           # AgentConfig, AgentConfigJson
├── capabilities.rs     # Capabilities, NodeCapacity
├── wasm.rs             # WasmRuntimeConfig, WasmRuntimeStats, WasmExitStatus
├── snapshot.rs         # AgentSnapshotType, AgentSnapshotInfo
└── desired_state.rs    # DesiredState, DesiredApp, DesiredVolume, VolumeMount
```

**Key Changes Made:**
- `agent/src/wasm/mod.rs` now re-exports `WasmRuntimeConfig`, `WasmRuntimeStats`, `WasmExitStatus` from schema
- `agent/src/snapshot.rs` now re-exports `AgentSnapshotType`, `AgentSnapshotInfo` from schema
- Local type definitions for `WasmConfig`, `WasmStats`, `ExitStatus`, `SnapshotType`, `SnapshotInfo`, `DesiredState`, `DesiredApp`, `DesiredVolume`, `VolumeMount` have been removed
- Agent crate now properly imports all shared types from schema

### Phase 3: Control Plane Handler Cleanup ✅ COMPLETE

**Objective:** Use schema types in API handlers instead of local definitions.

**Status:** Completed on 2025-01-22

**Completed Tasks:**
- [x] Import `HealthResponse` from schema's `api::responses` module
- [x] Import `ErrorResponse` from schema's `api::responses` module  
- [x] Import `PaginatedResponse`, `PaginationParams`, `Cursor` from schema's `api::pagination` module
- [x] Import `ScaleRequest` from schema's `api` module
- [x] Import `ResourceRequest` from schema's `entities` module
- [x] Update `response.rs` to re-export from schema with local extensions
- [x] Use schema's `AgentConnection` type (already re-exported in state.rs)

**Implementation Details:**

The control-plane handlers now import the following from schema:
```rust
use shellwego_schema::entities::ResourceRequest;
use shellwego_schema::api::ScaleRequest;
use shellwego_schema::api::responses::HealthResponse;
use shellwego_schema::api::pagination::PaginatedResponse;
```

The `response.rs` module now re-exports types from schema:
```rust
pub use shellwego_schema::api::responses::{
    ApiResponse, ErrorResponse, HealthResponse, ServiceStatus, ComponentHealth,
};
pub use shellwego_schema::api::pagination::{PaginatedResponse, PaginationParams, Cursor};
```

**Note:** API response types (App, Node, Volume, etc.) remain as local DTOs optimized for
API responses. These are distinct from full domain entities and serve as simplified views.
Auth types (TokenResponse, CreateTokenRequest) remain local as they are not in schema.

### Phase 4: Network Crate Consolidation ✅ COMPLETE

**Objective:** Consolidate all network-related types in schema.

**Status:** Completed on 2025-01-22

**Completed Tasks:**
- [x] `NetworkConfig`, `NetworkSetup` already in schema (pre-existing)
- [x] `AgentConnection` already in schema and control-plane using it via re-export
- [x] Created `shellwego-schema/src/network/discovery.rs` with `ServiceInstance` and `DiscoveryError` types
- [x] Updated `shellwego-schema/src/network/mod.rs` to export discovery types
- [x] Updated `shellwego-schema/src/lib.rs` to re-export discovery types at crate root
- [x] Updated `shellwego-network/src/discovery.rs` to re-export types from schema (kept business logic classes)
- [x] Updated `shellwego-agent/src/discovery.rs` to use schema types directly

**Implemented File Structure:**
```
shellwego-schema/src/network/
├── mod.rs              # Module exports
├── config.rs           # NetworkConfig, NetworkSetup (pre-existing)
├── error.rs            # NetworkError (pre-existing)
├── quinn.rs            # QuicConfig, AgentConnection, Message, ResourceLimits, ChannelPriority (pre-existing)
└── discovery.rs        # ✅ NEW: ServiceInstance, DiscoveryError
```

**Key Changes Made:**
- `ServiceInstance` and `DiscoveryError` types moved from `shellwego-network/src/discovery.rs` to `shellwego-schema/src/network/discovery.rs`
- Business logic classes (`DiscoveryResolver`, `DiscoveryRegistry`) remain in `shellwego-network/src/discovery.rs`
- `shellwego-network` re-exports types from schema for convenience
- `shellwego-agent` now imports types directly from schema

### Phase 5: Registry/Storage Consolidation ✅ COMPLETE

**Objective:** Consolidate OCI/Registry types.

**Status:** Completed on 2025-01-21

**Completed Tasks:**
- [x] Create `shellwego-schema/src/oci/` module structure
- [x] Move shared types: `Manifest`, `Platform`, `Descriptor`, `ConfigDescriptor`, `LayerDescriptor`, `ManifestDescriptor`
- [x] Move: `ImageConfig`, `ContainerConfig`, `RootFs`, `HistoryEntry`
- [x] Move: `RegistryAuth`, `AuthToken`, `OciConfig`
- [x] Update `shellwego-registry` to use schema types
- [x] Update `shellwego-storage` to use schema types
- [x] Delete duplicate type definitions from both crates

**Implemented File Structure:**
```
shellwego-schema/src/oci/
├── mod.rs              # Module exports
├── manifest.rs         # Manifest, ManifestIndex
├── descriptor.rs       # Descriptor, ConfigDescriptor, LayerDescriptor, ManifestDescriptor
├── platform.rs         # Platform
├── config.rs           # ImageConfig, ContainerConfig, RootFs, HistoryEntry
└── auth.rs             # RegistryAuth, AuthToken, OciConfig
```

**Key Changes Made:**
- `shellwego-registry/src/lib.rs` now imports OCI types from `shellwego-schema`
- `shellwego-registry/src/cache.rs` uses schema's `Manifest`, `Descriptor`, `Platform`
- `shellwego-registry/src/pull.rs` uses schema's `Manifest`, `RegistryAuth`, `ImageConfig`, etc.
- `shellwego-storage/src/oci.rs` uses schema's `OciConfig`, `Platform`, `Manifest`, etc.
- Both crates' `Cargo.toml` now depend on `shellwego-schema`

### Phase 6: Billing Entity Migration (Week 4)

**Objective:** Move billing domain entities to schema.

**Tasks:**
1. Create `shellwego-schema/src/billing/` module
2. Move: `Customer`, `BillingPeriod`, `Invoice`, `PaymentResult`
3. Keep billing-specific logic in billing crate

---

## 4. Detailed Migration Examples

### 4.1 Example: AgentConnection Consolidation

**Current State (TWO definitions):**

```rust
// schema/src/network/quinn.rs
pub struct AgentConnection {
    pub node_id: Uuid,
    pub remote_addr: String,
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

// control-plane/src/state.rs
pub struct AgentConnection {
    pub node_id: Option<Uuid>,
    pub hostname: Option<String>,
}
```

**Resolution:**
```rust
// schema/src/network/quinn.rs - UNIFIED
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct AgentConnection {
    /// Node ID (None during registration)
    pub node_id: Option<Uuid>,
    /// Remote address of the agent
    pub remote_addr: String,
    /// Hostname of the agent node
    pub hostname: Option<String>,
    /// Connection established timestamp
    pub connected_at: DateTime<Utc>,
}

// control-plane/src/state.rs - UPDATED
use shellwego_schema::network::AgentConnection;
// Remove local definition
```

### 4.2 Example: NetworkInterface Consolidation

**Current State (TWO definitions):**

```rust
// schema/src/vmm/config.rs - Higher-level abstraction
pub struct NetworkInterface {
    pub iface_id: String,
    pub host_dev_name: String,
    pub guest_mac: String,
    pub guest_ip: String,
    pub host_ip: String,
    pub tx_rate_limiter: Option<RateLimiterConfig>,
    pub rx_rate_limiter: Option<RateLimiterConfig>,
}

// firecracker/src/models.rs - Firecracker API format
pub struct NetworkInterface {
    pub iface_id: String,
    pub host_dev_name: String,
    pub guest_mac: Option<String>,
    pub rx_rate_limiter: Option<RateLimiter>,
    pub tx_rate_limiter: Option<RateLimiter>,
}
```

**Resolution:**
```rust
// schema/src/firecracker/network.rs - Firecracker API format (source of truth)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub iface_id: String,
    pub host_dev_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_mac: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx_rate_limiter: Option<RateLimiter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_rate_limiter: Option<RateLimiter>,
}

// schema/src/vmm/config.rs - Higher-level abstraction
// Rename to avoid confusion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmmNetworkInterface {
    pub iface_id: String,
    pub host_dev_name: String,
    pub guest_mac: String,
    pub guest_ip: String,
    pub host_ip: String,
    pub tx_rate_limiter: Option<RateLimiterConfig>,
    pub rx_rate_limiter: Option<RateLimiterConfig>,
}

impl From<VmmNetworkInterface> for firecracker::NetworkInterface {
    fn from(vmm: VmmNetworkInterface) -> Self {
        Self {
            iface_id: vmm.iface_id,
            host_dev_name: vmm.host_dev_name,
            guest_mac: Some(vmm.guest_mac),
            rx_rate_limiter: vmm.rx_rate_limiter.map(Into::into),
            tx_rate_limiter: vmm.tx_rate_limiter.map(Into::into),
        }
    }
}
```

### 4.3 Example: Control Plane Handlers

**Current State (REDUNDANT definitions):**
```rust
// control-plane/src/api/handlers.rs

#[derive(Debug, Serialize, Deserialize)]
pub struct App {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub image: String,
    pub replicas: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAppRequest {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub replicas: u32,
}
```

**Resolution:**
```rust
// control-plane/src/api/handlers.rs
use shellwego_schema::entities::{App, CreateAppRequest};
use shellwego_schema::api::responses::ErrorResponse;

// Handlers use schema types directly
pub async fn create_app(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CreateAppRequest>,
) -> Result<(StatusCode, Json<App>), (StatusCode, Json<ErrorResponse>)> {
    // Implementation uses schema types
}
```

---

## 5. Implementation Checklist

### 5.1 Pre-Migration Checklist

- [ ] Run full test suite and capture baseline
- [ ] Create feature branch for migration
- [ ] Ensure CI/CD pipeline is green
- [ ] Document current type locations

### 5.2 Per-Type Migration Checklist

For each type being consolidated:

- [ ] Identify all usages across the codebase (`grep -r "TypeName" crates/`)
- [ ] Ensure destination file exists in schema
- [ ] Copy type definition with all documentation
- [ ] Add `Serialize, Deserialize` derives if not present
- [ ] Add OpenAPI derives behind feature flag
- [ ] Add ORM derives behind feature flag (if applicable)
- [ ] Add conversion traits if needed (`From`, `Into`)
- [ ] Update imports in source crate to re-export from schema
- [ ] Update any dependent crates
- [ ] Move or duplicate tests
- [ ] Run tests to verify compatibility
- [ ] Update documentation

### 5.3 Post-Migration Verification

- [ ] All tests pass
- [ ] No compilation warnings
- [ ] `cargo clippy` passes
- [ ] Documentation is updated
- [ ] No duplicate type definitions remain
- [ ] All crates properly import from schema

---

## 6. Testing Strategy

### 6.1 Serialization Compatibility Tests

```rust
// tests/schema_compatibility.rs

/// Verify serialization compatibility after migration
#[test]
fn test_agent_connection_compatibility() {
    let conn = AgentConnection {
        node_id: Some(Uuid::new_v4()),
        remote_addr: "10.0.0.1:4433".to_string(),
        hostname: Some("worker-1".to_string()),
        connected_at: Utc::now(),
    };

    let json = serde_json::to_string(&conn).unwrap();
    let decoded: AgentConnection = serde_json::from_str(&json).unwrap();
    assert_eq!(conn.node_id, decoded.node_id);
}
```

### 6.2 Type Alias Compatibility

```rust
/// Ensure old import paths still work (with deprecation warning)
#[deprecated(since = "0.2.0", note = "Use shellwego_schema::network::AgentConnection")]
pub use shellwego_schema::network::AgentConnection;
```

### 6.3 Integration Tests

After each phase, run integration tests to verify:
- Control plane can communicate with agents
- API endpoints serialize/deserialize correctly
- Database operations work with ORM feature

---

## 7. Risk Mitigation

### 7.1 Breaking Change Prevention

1. **Deprecation Warnings**: Add deprecation warnings for old import paths
2. **Re-exports**: Keep re-exports in source crates for backward compatibility
3. **Feature Flags**: Ensure feature flags work correctly after migration

### 7.2 Rollback Strategy

If issues arise:
1. Each phase is in a separate branch
2. Can revert individual type migrations
3. Keep old definitions temporarily with deprecation warnings

---

## 8. Success Metrics

Migration is complete when:

- [ ] **Zero Duplicates**: No type is defined in more than one location
- [ ] **Single Import**: All crates import shared types from schema
- [ ] **Test Coverage**: All moved types have serialization tests
- [ ] **Documentation**: All modules have updated documentation
- [ ] **CI Green**: All tests pass, no warnings
- [ ] **Clippy Clean**: `cargo clippy --all` passes without errors

---

## 9. Timeline

| Week | Phase | Focus Area | Owner |
|------|-------|------------|-------|
| 1 | Phase 1 | Firecracker Models | Platform Team |
| 2 | Phase 2 | Agent Types | Agent Team |
| 2-3 | Phase 3 | Control Plane Handlers | API Team |
| 3 | Phase 4 | Network Consolidation | Network Team |
| 3-4 | Phase 5 | Registry/Storage | Platform Team |
| 4 | Phase 6 | Billing Entities | Platform Team |
| 4+ | Cleanup | Documentation & Tests | All Teams |

---

## 10. Related Documents

- [Entity Schizophrenia](./EntitySchizophrenia.md) - Original problem identification
- [Schema Consolidation Plan](./schema-consolidation.plan.md) - Original rename plan (completed)
- [REST API Endpoints](./rest.endpoints.md) - API documentation

---

## 11. Appendix: Complete Type Inventory

### A. Types Currently in Schema (Keep)

| Module | Types |
|--------|-------|
| `entities/app.rs` | App, AppStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec, RegistryAuth, CreateAppRequest, UpdateAppRequest |
| `entities/node.rs` | Node, NodeStatus, NodeCapacity, NodeCapabilities, NodeNetwork, NodeLabels, RegisterNodeRequest, NodeJoinResponse |
| `entities/volume.rs` | Volume, VolumeStatus, VolumeType, FilesystemType, Snapshot, BackupPolicy, CreateVolumeRequest |
| `entities/domain.rs` | Domain, DomainStatus, TlsCertificate, DnsValidation, RoutingConfig, EdgeFeatures, CreateDomainRequest |
| `entities/database.rs` | Database, DatabaseEngine, DatabaseStatus, DatabaseEndpoint, HighAvailability, CreateDatabaseRequest |
| `entities/secret.rs` | Secret, SecretScope, SecretVersion, CreateSecretRequest, RotateSecretRequest |
| `entities/organization.rs` | Organization, OrgSettings, TeamMember, ApiKey, PlanTier, TeamRole |
| `entities/backup.rs` | Backup, BackupStatus, BackupMetadata, RestoreJob, ResourceType, CompressionFormat |
| `entities/build.rs` | Build, BuildStatus, BuildSource, Deployment, DeploymentStatus, DeploymentStrategy |
| `entities/metrics.rs` | MetricSample, MetricSeries, AlertRule, AlertCondition, AlertSeverity |
| `entities/webhook.rs` | Webhook, WebhookEventType, WebhookDelivery |
| `entities/audit.rs` | AuditLogEntry, AuditMetadata, ActorType |
| `vmm/config.rs` | MicrovmConfig, DriveConfig, RateLimiterConfig, NetworkInterface, WasmConfig |
| `vmm/state.rs` | MicrovmState, MicrovmSummary |
| `vmm/metrics.rs` | MicrovmMetrics |
| `vmm/virtualization.rs` | VirtualizationMode |
| `network/config.rs` | NetworkConfig, NetworkSetup |
| `network/discovery.rs` | ✅ NEW: ServiceInstance, DiscoveryError |
| `network/quinn.rs` | QuicConfig, Message, AgentConnection, ResourceLimits, ChannelPriority |
| `network/error.rs` | NetworkError |
| `api/apps.rs` | ListAppsQuery, ListNodesQuery, ScaleRequest, DeployRequest, DeployStrategy |
| `api/pagination.rs` | PaginatedResponse, PaginationParams, Cursor |
| `api/responses.rs` | ApiResponse, ErrorResponse, HealthResponse, ComponentHealth, ServiceStatus |
| `agent/config.rs` | AgentConfig, AgentConfigJson |
| `agent/capabilities.rs` | Capabilities, NodeCapacity |
| `agent/wasm.rs` | ✅ NEW: WasmRuntimeConfig, WasmRuntimeStats, WasmExitStatus |
| `agent/snapshot.rs` | ✅ NEW: AgentSnapshotType, AgentSnapshotInfo |
| `firecracker/*` | ✅ NEW: All 50+ Firecracker API types (see Section B) |
| `error.rs` | CoreError, CoreResult |

### B. Types Migrated (Phase 1 - Firecracker) ✅ COMPLETE

All Firecracker types have been moved to `schema/src/firecracker/`:

| Module | Types |
|--------|-------|
| `firecracker/instance.rs` | InstanceInfo, InstanceState, FirecrackerVersion |
| `firecracker/boot.rs` | BootSource |
| `firecracker/machine.rs` | MachineConfiguration, CpuTemplate, HugePages, CpuConfig, CpuidLeafModifier, CpuidRegisterModifier, MsrModifier, ArmRegisterModifier, VcpuFeatures, CpuidRegister |
| `firecracker/drives.rs` | Drive, PartialDrive, CacheType, IoEngine, Pmem |
| `firecracker/network.rs` | NetworkInterface, PartialNetworkInterface, RateLimiter, TokenBucket |
| `firecracker/balloon.rs` | Balloon, BalloonUpdate, BalloonStats, BalloonStatsUpdate, BalloonStartCmd, BalloonHintingStatus |
| `firecracker/devices.rs` | Vsock, EntropyDevice, SerialDevice |
| `firecracker/logging.rs` | Logger, LogLevel, Metrics |
| `firecracker/metrics.rs` | FirecrackerMetrics, VmmMetrics, NetMetrics, BlockMetrics |
| `firecracker/actions.rs` | InstanceActionInfo, ActionType, Vm, VmState |
| `firecracker/snapshot.rs` | SnapshotCreateParams, SnapshotLoadParams, SnapshotType, MemoryBackend, MemoryBackendType, NetworkOverride |
| `firecracker/memory.rs` | MemoryHotplugConfig, MemoryHotplugSizeUpdate, MemoryHotplugStatus |
| `firecracker/mmds.rs` | MmdsConfig, MmdsVersion, MmdsContentsObject |
| `firecracker/full_config.rs` | FullVmConfiguration |
| `firecracker/error.rs` | Error |

### C. Types Migrated (Phase 2 - Agent) ✅ COMPLETE

All Agent types have been moved to `schema/src/agent/`:

| Module | Types |
|--------|-------|
| `agent/wasm.rs` | WasmRuntimeConfig, WasmRuntimeStats, WasmExitStatus |
| `agent/snapshot.rs` | AgentSnapshotType, AgentSnapshotInfo |
| `agent/desired_state.rs` | DesiredState, DesiredApp, DesiredVolume, VolumeMount |

### D. Types Migrated (Phase 3 - Control Plane) ✅ COMPLETE

| Current Location | Types | Action | Status |
|------------------|-------|--------|--------|
| `control-plane/src/api/handlers.rs` | HealthResponse | Import from schema | ✅ Complete |
| `control-plane/src/api/handlers.rs` | ErrorResponse | Import from schema | ✅ Complete |
| `control-plane/src/api/handlers.rs` | ScaleRequest | Import from schema | ✅ Complete |
| `control-plane/src/api/handlers.rs` | ResourceRequest | Import from schema | ✅ Complete |
| `control-plane/src/api/handlers.rs` | PaginatedResponse | Import from schema | ✅ Complete |
| `control-plane/src/state.rs` | AgentConnection | Already using schema type | ✅ Complete |
| `control-plane/src/api/response.rs` | ApiResponse, ErrorResponse, HealthResponse | Re-export from schema | ✅ Complete |
| `control-plane/src/api/response.rs` | PaginatedResponse, PaginationParams, Cursor | Re-export from schema | ✅ Complete |

**Note:** API response types (App, Node, Volume, etc.) remain as local DTOs optimized for
API responses. These are distinct from full domain entities and serve as simplified views.
Auth types (TokenResponse, CreateTokenRequest) remain local as they are not in schema.

### E. Types Migrated (Phase 5 - Registry/Storage) ✅ COMPLETE

| Previous Location | Types | New Location |
|-------------------|-------|--------------|
| `registry/src/lib.rs` | ~~AuthToken, RegistryAuth, Manifest, ConfigDescriptor, LayerDescriptor, ManifestDescriptor, Platform, ImageConfig, ContainerConfig, RootFs, HistoryEntry~~ | `schema/src/oci/` |
| `registry/src/cache.rs` | LayerCache, CachedImageInfo, CacheStats | Kept in registry (implementation-specific) |
| `storage/src/oci.rs` | ~~OciConfig, Platform, Manifest, ConfigDescriptor, LayerDescriptor~~ | `schema/src/oci/` |
| `storage/src/oci.rs` | OciClient, OciError | Kept in storage (implementation logic) |

---

*End of Plan Document*
