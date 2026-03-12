# Schema Consolidation Migration Summary

## Overview

This document summarizes the changes made to consolidate types from multiple crates into the `shellwego-schema` crate, following the DRY (Don't Repeat Yourself) principle.

## Phase 1: Rename shellwego-core → shellwego-schema

### Changes Made:
1. **Directory Rename**: `crates/shellwego-core` → `crates/shellwego-schema`
2. **Cargo.toml Updates**:
   - Package name changed from `shellwego-core` to `shellwego-schema`
   - Description updated to "Schema crate: pure data structures, types, and wire format definitions for ShellWeGo"
   - Added dependencies: `ipnetwork`, `base64`, `nix`

3. **Dependent Crates Updated**:
   - `shellwego-agent/Cargo.toml`: Now depends on `shellwego-schema`
   - `shellwego-cli/Cargo.toml`: Now depends on `shellwego-schema`
   - `shellwego-control-plane/Cargo.toml`: Now depends on `shellwego-schema`
   - `shellwego-network/Cargo.toml`: Added `shellwego-schema` dependency

4. **Import Updates**: All `use shellwego_core::` changed to `use shellwego_schema::`

## Phase 2: New Module Structure in shellwego-schema

### Created Modules:

#### 1. `vmm/` - Virtual Machine Manager Types
- `virtualization.rs`: `VirtualizationMode` enum
- `state.rs`: `MicrovmState`, `MicrovmSummary`
- `config.rs`: `MicrovmConfig`, `DriveConfig`, `NetworkInterface`, `RateLimiterConfig`, `WasmConfig`
- `metrics.rs`: `MicrovmMetrics`

#### 2. `network/` - Network Types
- `config.rs`: `NetworkConfig`, `NetworkSetup`
- `error.rs`: `NetworkError` enum
- `quinn.rs`: `Message`, `QuicConfig`, `ResourceLimits`, `AgentConnection`, `ChannelPriority`

#### 3. `api/` - API Request/Response Types
- `apps.rs`: `ListAppsQuery`, `ScaleRequest`, `DeployRequest`, `DeployStrategy`, `ListNodesQuery`
- `pagination.rs`: `PaginatedResponse`, `PaginationParams`, `Cursor`
- `responses.rs`: `ApiResponse`, `ErrorResponse`, `HealthResponse`, `ServiceStatus`

#### 4. `agent/` - Agent Types
- `config.rs`: `AgentConfig`, `AgentConfigJson`
- `capabilities.rs`: `Capabilities`, `NodeCapacity`

## Phase 3: Updated Dependent Crates

### shellwego-agent
- `src/lib.rs`: Now re-exports `VirtualizationMode`, `AgentConfig`, `Capabilities` from schema
- `src/vmm/mod.rs`: Re-exports `MicrovmConfig`, `DriveConfig`, `NetworkInterface`, etc. from schema
- `src/vmm/config.rs`: Simplified to re-export from schema

### shellwego-network
- `src/lib.rs`: Now re-exports `NetworkConfig`, `NetworkSetup`, `NetworkError`, `Message`, `QuicConfig` from schema

### shellwego-control-plane
- `src/api/handlers/apps.rs`: Imports `ListAppsQuery`, `ScaleRequest` from schema

## File Structure

```
crates/shellwego-schema/
├── Cargo.toml                    # Updated with new dependencies
├── src/
│   ├── lib.rs                    # Module re-exports
│   ├── entities/                 # Existing domain entities
│   ├── vmm/                      # NEW: VMM types
│   │   ├── mod.rs
│   │   ├── virtualization.rs
│   │   ├── state.rs
│   │   ├── config.rs
│   │   └── metrics.rs
│   ├── network/                  # NEW: Network types
│   │   ├── mod.rs
│   │   ├── config.rs
│   │   ├── error.rs
│   │   └── quinn.rs
│   ├── api/                      # NEW: API types
│   │   ├── mod.rs
│   │   ├── apps.rs
│   │   ├── pagination.rs
│   │   └── responses.rs
│   └── agent/                    # NEW: Agent types
│       ├── mod.rs
│       ├── config.rs
│       └── capabilities.rs
```

## Benefits Achieved

1. **Single Source of Truth**: All shared types are now in one crate
2. **Reduced Duplication**: Types are defined once and re-exported where needed
3. **Clear Separation**: Pure data structures (schema) vs. business logic (other crates)
4. **Easier Maintenance**: Changes to shared types only need to be made in one place
5. **Better Documentation**: Types are documented in their canonical location

## Remaining Work

- [ ] Create `firecracker/` module with models from `shellwego-firecracker`
- [ ] Verify all tests pass
- [ ] Update documentation files to reflect new schema structure
