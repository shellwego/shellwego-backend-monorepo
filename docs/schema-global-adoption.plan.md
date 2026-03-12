# Schema Global Adoption Plan: Eliminating DRY Violations

## Executive Summary

This plan addresses the critical DRY (Don't Repeat Yourself) violations in the ShellWeGo backend monorepo where `shellwego-schema` exists but is NOT being used as the single source of truth for type definitions. Multiple crates define their own types locally, creating maintenance burden, type mismatch risks, and documentation fragmentation.

**Status**: Ready for Implementation
**Author**: Architecture Review
**Date**: 2025-01-20
**Priority**: Critical
**Supersedes**: `schema-consolidation.plan.md` (which discussed renaming `shellwego-core` - now complete)

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
└── agent/              # Agent configuration types
```

However, many crates ignore this schema and define their own types locally.

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

| Type | Schema Location | Duplicate Location | Severity |
|------|-----------------|-------------------|----------|
| `NetworkInterface` | `schema/src/vmm/config.rs` | `firecracker/src/models.rs` | **CRITICAL** |
| `WasmConfig` | `schema/src/vmm/config.rs` | `agent/src/wasm/mod.rs` | **CRITICAL** |
| `AgentConnection` | `schema/src/network/quinn.rs` | `control-plane/src/state.rs` | **CRITICAL** |
| `RateLimiter` / `RateLimiterConfig` | `schema/src/vmm/config.rs` | `firecracker/src/models.rs` | **HIGH** |
| `SnapshotType` | `firecracker/src/models.rs` | `agent/src/snapshot.rs` | **HIGH** |
| `ErrorResponse` | `schema/src/api/responses.rs` | `control-plane/src/api/mod.rs` | **HIGH** |

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

### Phase 1: Firecracker Models (Week 1)

**Objective:** Move all Firecracker API models to schema.

**Tasks:**
1. Create `shellwego-schema/src/firecracker/` module structure
2. Move all types from `shellwego-firecracker/src/models.rs`
3. Add proper `cfg_attr` for OpenAPI derives
4. Update `shellwego-firecracker` to re-export from schema
5. Update `shellwego-agent` VMM types to use schema types

**File Structure:**
```
shellwego-schema/src/firecracker/
├── mod.rs              # Module exports
├── instance.rs         # InstanceInfo, InstanceState, FirecrackerVersion
├── boot.rs             # BootSource
├── machine.rs          # MachineConfiguration, CpuTemplate, HugePages, CpuConfig
├── drives.rs           # Drive, PartialDrive, CacheType, IoEngine
├── network.rs          # NetworkInterface, RateLimiter, TokenBucket
├── balloon.rs          # Balloon, BalloonStats, BalloonUpdate
├── devices.rs          # Vsock, EntropyDevice, SerialDevice, Pmem
├── logging.rs          # Logger, Metrics, LogLevel
├── metrics.rs          # FirecrackerMetrics, VmmMetrics, NetMetrics, BlockMetrics
├── actions.rs          # InstanceActionInfo, ActionType
├── snapshot.rs         # SnapshotCreateParams, SnapshotLoadParams, SnapshotType
├── memory.rs           # MemoryBackend, MemoryHotplugConfig
└── mmds.rs             # MmdsConfig, MmdsVersion
```

**Migration Command:**
```bash
# After updating schema
cargo check -p shellwego-firecracker
cargo check -p shellwego-agent
```

### Phase 2: Consolidate Agent Types (Week 2)

**Objective:** Remove all local type definitions in agent crate.

**Files to Update:**
- `shellwego-agent/src/lib.rs` - Remove `VirtualizationMode` (use schema)
- `shellwego-agent/src/wasm/mod.rs` - Remove `WasmConfig` (use schema)
- `shellwego-agent/src/snapshot.rs` - Remove `SnapshotType` (use schema)

**Pattern:**
```rust
// BEFORE (agent/src/wasm/mod.rs)
pub struct WasmConfig {
    pub max_memory_mb: u32,
}

// AFTER
// Remove local definition, import from schema
use shellwego_schema::vmm::WasmConfig;
```

### Phase 3: Control Plane Handler Cleanup (Week 2-3)

**Objective:** Use schema entities in API handlers instead of local definitions.

**Current Problem:**
```rust
// control-plane/src/api/handlers.rs - WRONG
#[derive(Debug, Serialize, Deserialize)]
pub struct App {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub image: String,
    pub replicas: u32,
}
```

**Solution:**
```rust
// control-plane/src/api/handlers.rs - CORRECT
use shellwego_schema::entities::{App, Node, Volume, Domain, Database, Secret};
use shellwego_schema::api::{ListAppsQuery, ListNodesQuery, ScaleRequest};
use shellwego_schema::api::responses::{ErrorResponse, HealthResponse};
```

**Tasks:**
1. Replace all entity structs with imports from schema
2. Replace request/response types with schema imports
3. Update handler signatures to use schema types
4. Ensure serialization compatibility

### Phase 4: Network Crate Consolidation (Week 3)

**Objective:** Consolidate all network-related types in schema.

**Tasks:**
1. Move `NetworkConfig`, `NetworkSetup` from network crate to schema (if not already)
2. Ensure `AgentConnection` in control-plane uses schema type
3. Consolidate `discovery.rs` types (`ServiceInstance`, `DiscoveryError`)

### Phase 5: Registry/Storage Consolidation (Week 3-4)

**Objective:** Consolidate OCI/Registry types.

**Tasks:**
1. Create `shellwego-schema/src/oci/` module
2. Move shared types: `Manifest`, `Platform`, `Descriptor`, `ConfigDescriptor`
3. Update both registry and storage crates to use schema

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
| `network/quinn.rs` | QuicConfig, Message, AgentConnection, ResourceLimits, ChannelPriority |
| `network/error.rs` | NetworkError |
| `api/apps.rs` | ListAppsQuery, ListNodesQuery, ScaleRequest, DeployRequest, DeployStrategy |
| `api/pagination.rs` | PaginatedResponse, PaginationParams, Cursor |
| `api/responses.rs` | ApiResponse, ErrorResponse, HealthResponse, ComponentHealth, ServiceStatus |
| `agent/config.rs` | AgentConfig, AgentConfigJson |
| `agent/capabilities.rs` | Capabilities, NodeCapacity |
| `error.rs` | CoreError, CoreResult |

### B. Types to Migrate (Phase 1 - Firecracker)

| Current Location | Types |
|------------------|-------|
| `firecracker/src/models.rs` | InstanceInfo, InstanceState, FirecrackerVersion, BootSource, MachineConfiguration, CpuTemplate, HugePages, CpuConfig, CpuidLeafModifier, CpuidRegisterModifier, MsrModifier, ArmRegisterModifier, VcpuFeatures, Drive, PartialDrive, CacheType, IoEngine, Pmem, NetworkInterface (FC), PartialNetworkInterface, RateLimiter, TokenBucket, Balloon, BalloonUpdate, BalloonStats, BalloonStatsUpdate, BalloonStartCmd, BalloonHintingStatus, Vsock, EntropyDevice, SerialDevice, Logger, LogLevel, Metrics, FirecrackerMetrics, VmmMetrics, NetMetrics, BlockMetrics, InstanceActionInfo, ActionType, Vm, VmState, SnapshotCreateParams, SnapshotLoadParams, SnapshotType, MemoryBackend, MemoryBackendType, NetworkOverride, MemoryHotplugConfig, MemoryHotplugSizeUpdate, MemoryHotplugStatus, MmdsConfig, MmdsVersion, FullVmConfiguration, Error |

### C. Types to Migrate (Phase 2 - Agent)

| Current Location | Types | Target |
|------------------|-------|--------|
| `agent/src/wasm/mod.rs` | WasmConfig, WasmError, WasmStats, ExitStatus | `schema/src/wasm/` |
| `agent/src/snapshot.rs` | SnapshotType, SnapshotInfo, SnapshotManager | Consolidate with FC types |
| `agent/src/daemon.rs` | DesiredState, DesiredApp, VolumeMount, DesiredVolume | `schema/src/agent/` |

### D. Types to Migrate (Phase 3 - Control Plane)

| Current Location | Types | Action |
|------------------|-------|--------|
| `control-plane/src/api/handlers.rs` | App, Node, Volume, Domain, Database, Secret, CreateAppRequest, CreateVolumeRequest, CreateDomainRequest, CreateDatabaseRequest, CreateSecretRequest, ListAppsQuery, ListNodesQuery, HealthResponse, TokenResponse, CreateTokenRequest | DELETE - Use schema imports |
| `control-plane/src/state.rs` | AgentConnection | DELETE - Use schema type |
| `control-plane/src/operators/mod.rs` | DatabaseSpec, ResourceSpec, HaConfig, BackupConfig, ConnectionInfo, InstanceStatus, BackupInfo, OperatorConfig, ResourceQuotas | Consolidate with schema entities |

### E. Types to Migrate (Phase 5 - Registry/Storage)

| Current Location | Types | Target |
|------------------|-------|--------|
| `registry/src/lib.rs` | AuthToken, RegistryAuth, Manifest, ConfigDescriptor, LayerDescriptor, ManifestDescriptor, Platform, ImageConfig, ContainerConfig, RootFs, HistoryEntry | `schema/src/oci/` |
| `registry/src/cache.rs` | LayerCache, CachedImageInfo, CacheStats | Keep in registry |
| `storage/src/oci.rs` | OciConfig, Platform, OciClient, Manifest, ConfigDescriptor, LayerDescriptor | Consolidate with registry types |

---

*End of Plan Document*
