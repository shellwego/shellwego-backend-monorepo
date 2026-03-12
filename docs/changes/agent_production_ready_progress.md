# Progress Report: Making `shellwego-agent` Production Ready

## Overview
This report summarizes the initial efforts to make the `shellwego-agent` crate production-ready, focusing on resolving significant codebase "bit rot" and dependency conflicts that prevented the crate from building.

## Actions Taken

### 1. Workspace Dependency Resolution
- Resolved conflicts in the root `Cargo.toml` related to optional dependencies in workspace definitions.
- Fixed version mismatches and feature requirements for several key crates:
    - **`async-stripe`**: Replaced outdated/yanked `stripe-rust` with `async-stripe` and adjusted features for compatibility.
    - **`tower-http`**: Fixed `compression` feature name to `compression-full`.
    - **`hickory-dns`**: Resolved issues with missing `tokio-runtime` and `system-config` features by switching to `hickory-resolver`.
    - **`aya`**: Fixed mismatched feature names (`async-tokio` -> `async_tokio`).

### 2. Firecracker SDK Integration
- Encountered issues resolving the official AWS `firecracker-rs` SDK version 0.4 from external registries.
- Created a local placeholder crate `crates/shellwego-firecracker` that implements the required models and `FirecrackerClient` interface.
- Updated `shellwego-agent` to use this local dependency, allowing progress on the agent logic without being blocked by external registry issues.

### 3. Core Crate Stabilization (`shellwego-core`)
- Identified and fixed numerous compilation errors in `shellwego-core` caused by missing trait implementations required by Sea-ORM and Schemars.
- Added `PartialEq` to multiple entity structs (App, Node, Volume, Database, Domain).
- Fixed `JsonSchema` implementation issues for `uuid::Uuid` and `chrono::DateTime`.
- Resolved missing imports for `IdenStatic` when the `orm` feature is enabled across all entity modules.
- Re-added `DeriveEntityModel` and other ORM-specific attributes that were missing or incorrectly handled during the refactor.

### 4. Storage Crate Fixes (`shellwego-storage`)
- Added missing dependencies (`async-trait`, `uuid`, `chrono`) to `shellwego-storage` to fix compilation errors in ZFS-related modules.

## Current State
The project has seen significant cleanup of its dependency graph and core models. While some compilation errors remain in `shellwego-core` (primarily around nested struct derives and `TryGetable` bounds for ORM queries), the groundwork for a buildable workspace is mostly complete.

## Next Steps
- **Resolve remaining `shellwego-core` errors**: Continue addressing trait bound issues in the core entities.
- **Implement Agent Boilerplate**: Once the dependency chain builds, finish the remaining TODOs in `shellwego-agent/src/main.rs`, `daemon.rs`, and `reconciler.rs`.
- **ZFS Integration**: Verify the interaction between the agent reconciler and the ZFS storage backend.
- **Networking**: Complete the eBPF/CNI setup logic in the agent.
