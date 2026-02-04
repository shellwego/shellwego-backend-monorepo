plan:
  uuid: 'shellwego-production-scaffold-v2'
  status: 'todo'
  title: 'ShellWeGo: Production-Grade Sovereign Cloud Scaffold'
  introduction: |
    We are establishing the structural skeleton for a high-performance PaaS. This isn't a toy; it's a distributed system. The plan enforces a workspace-based monorepo architecture using Rust's ecosystem titans (`tokio`, `axum`, `sqlx`, `clap`, `utoipa`).

    This scaffolding phase defines the **exact file topology** required to support every feature from the whitepaper: Apps, Nodes, Volumes, Domains, Managed Databases, and Secrets. We strictly adhere to "no implementation yet," generating file placeholders with high-intent `// TODO` comments that map out the hexagonal architecture.
  parts:
    - uuid: 'part-1-workspace-foundation'
      status: 'todo'
      name: 'Part 1: Workspace & Shared Kernel'
      reason: |
        The monorepo root and the shared `core` library are the bedrock. We prevent circular deps and ensure type safety across the wire (CLI <-> API <-> Agent) using shared Serde models.
      steps:
        - uuid: 'step-1-cargo-root'
          status: 'todo'
          name: 'Initialize Workspace Configuration'
          reason: |
            Defines the build universe.
          files:
            - Cargo.toml
            - rust-toolchain.toml
            - .gitignore
            - .dockerignore
          operations:
            - 'Configure workspace members: `crates/*`.'
            - 'Define workspace-level dependencies (versions pinned) to ensure consistency.'
        - uuid: 'step-2-core-domain'
          status: 'todo'
          name: 'Scaffold ShellWeGo-Core'
          reason: |
            The unified domain language.
          files:
            - crates/shellwego-core/Cargo.toml
            - crates/shellwego-core/src/lib.rs
            - crates/shellwego-core/src/prelude.rs
            - crates/shellwego-core/src/entities/mod.rs
            - crates/shellwego-core/src/entities/app.rs
            - crates/shellwego-core/src/entities/node.rs
            - crates/shellwego-core/src/entities/volume.rs
            - crates/shellwego-core/src/entities/domain.rs
            - crates/shellwego-core/src/entities/database.rs
            - crates/shellwego-core/src/entities/secret.rs
          operations:
            - 'Setup `serde`, `uuid`, `chrono`, `strum` dependencies.'
            - 'Create file stubs for all entity structs (request/response DTOs).'
            - 'Define `// TODO: derive(ToSchema)` for `utoipa` (Swagger) compatibility.'
      context_files:
        compact:
          - Cargo.toml
          - crates/shellwego-core/Cargo.toml
        medium: []
        extended: []

    - uuid: 'part-2-control-plane-api'
      status: 'todo'
      name: 'Part 2: Control Plane (API & Docs)'
      reason: |
        The interface layer. We structure this for `Axum` with automated OpenAPI generation via `utoipa`.
      steps:
        - uuid: 'step-1-cp-entrypoint'
          status: 'todo'
          name: 'Control Plane Entry & Config'
          reason: |
            Service bootstrapper.
          files:
            - crates/shellwego-control-plane/Cargo.toml
            - crates/shellwego-control-plane/src/main.rs
            - crates/shellwego-control-plane/src/config.rs
            - crates/shellwego-control-plane/src/state.rs
          operations:
            - 'Deps: `axum`, `tokio`, `tower`, `utoipa`, `sqlx`.'
            - 'Stub `AppState` struct (DB pool, NATS connection).'
        - uuid: 'step-2-api-handlers'
          status: 'todo'
          name: 'REST Handlers & OpenAPI Spec'
          reason: |
            Route handlers mirroring the API documentation.
          files:
            - crates/shellwego-control-plane/src/api/mod.rs
            - crates/shellwego-control-plane/src/api/docs.rs
            - crates/shellwego-control-plane/src/api/handlers/apps.rs
            - crates/shellwego-control-plane/src/api/handlers/nodes.rs
            - crates/shellwego-control-plane/src/api/handlers/volumes.rs
            - crates/shellwego-control-plane/src/api/handlers/domains.rs
            - crates/shellwego-control-plane/src/api/handlers/auth.rs
          operations:
            - 'Create module structure for all resources.'
            - '`api/docs.rs`: Stub `// TODO: #[derive(OpenApi)]` struct.'
            - 'Stub handler functions signatures (e.g., `async fn create_app(...)`).'
      context_files:
        compact:
          - crates/shellwego-control-plane/src/main.rs
          - crates/shellwego-control-plane/src/api/mod.rs
        medium:
          - crates/shellwego-core/src/lib.rs
        extended: []

    - uuid: 'part-3-control-plane-backend'
      status: 'todo'
      name: 'Part 3: Control Plane (Logic & State)'
      reason: |
        The business logic layer, separated from HTTP transport.
      steps:
        - uuid: 'step-1-service-layer'
          status: 'todo'
          name: 'Service Logic & Message Bus'
          reason: |
            Orchestration logic (Scheduler) and Persistence.
          files:
            - crates/shellwego-control-plane/src/services/mod.rs
            - crates/shellwego-control-plane/src/services/scheduler.rs
            - crates/shellwego-control-plane/src/services/deployment.rs
            - crates/shellwego-control-plane/src/db/mod.rs
            - crates/shellwego-control-plane/src/events/bus.rs
          operations:
            - 'Deps: `async-nats`.'
            - 'Stub `Scheduler` trait and `NatsBus` publisher.'
            - 'Stub `DeploymentStateMachine`.'
      context_files:
        compact:
          - crates/shellwego-control-plane/src/services/mod.rs
        medium: []
        extended: []

    - uuid: 'part-4-agent-runtime'
      status: 'todo'
      name: 'Part 4: Agent (Runtime & Firecracker)'
      reason: |
        The heavy lifter. Needs explicit structure for managing microVM life-cycles and reconciliation loops.
      steps:
        - uuid: 'step-1-agent-structure'
          status: 'todo'
          name: 'Agent Daemon Skeleton'
          reason: |
            The long-running process on worker nodes.
          files:
            - crates/shellwego-agent/Cargo.toml
            - crates/shellwego-agent/src/main.rs
            - crates/shellwego-agent/src/daemon.rs
            - crates/shellwego-agent/src/reconciler.rs
          operations:
            - 'Deps: `tokio`, `hyper` (client), `sysinfo`.'
            - 'Stub `Daemon` struct for node registration loop.'
            - 'Stub `Reconciler` for desired-state enforcement.'
        - uuid: 'step-2-firecracker-driver'
          status: 'todo'
          name: 'Firecracker VMM Integration'
          reason: |
            Low-level VM interaction.
          files:
            - crates/shellwego-agent/src/vmm/mod.rs
            - crates/shellwego-agent/src/vmm/driver.rs
            - crates/shellwego-agent/src/vmm/config.rs
          operations:
            - 'Stub `VmmDriver` for `socket` communication.'
            - 'Add `// TODO: implement jailer wrapping`.'
      context_files:
        compact:
          - crates/shellwego-agent/src/main.rs
        medium:
          - crates/shellwego-core/src/entities/node.rs
        extended: []

    - uuid: 'part-5-infra-layer'
      status: 'todo'
      name: 'Part 5: Infrastructure (Net & Storage)'
      reason: |
        Dedicated crates for privileged OS operations (ZFS, eBPF/Netlink). Kept separate for security auditing.
      steps:
        - uuid: 'step-1-storage-driver'
          status: 'todo'
          name: 'Storage Crate (ZFS)'
          reason: |
            ZFS command wrappers.
          files:
            - crates/shellwego-storage/Cargo.toml
            - crates/shellwego-storage/src/lib.rs
            - crates/shellwego-storage/src/zfs/mod.rs
            - crates/shellwego-storage/src/zfs/cli.rs
          operations:
            - 'Stub methods: `create_dataset`, `snapshot`, `clone`, `mount`.'
        - uuid: 'step-2-network-driver'
          status: 'todo'
          name: 'Network Crate (CNI)'
          reason: |
            Networking setup for microVMs.
          files:
            - crates/shellwego-network/Cargo.toml
            - crates/shellwego-network/src/lib.rs
            - crates/shellwego-network/src/cni/mod.rs
            - crates/shellwego-network/src/cni/bridge.rs
            - crates/shellwego-network/src/cni/ebpf.rs
          operations:
            - 'Stub `setup_tap`, `configure_bridge`, `attach_ebpf_program`.'
      context_files:
        compact:
          - crates/shellwego-storage/src/lib.rs
          - crates/shellwego-network/src/lib.rs
        medium: []
        extended: []

    - uuid: 'part-6-cli-ux'
      status: 'todo'
      name: 'Part 6: User Interface (CLI)'
      reason: |
        The primary consumption method. Modeled with `clap` to ensure structural parity with the API.
      steps:
        - uuid: 'step-1-cli-skeleton'
          status: 'todo'
          name: 'CLI Root & Auth'
          reason: |
            Entry point and config management.
          files:
            - crates/shellwego-cli/Cargo.toml
            - crates/shellwego-cli/src/main.rs
            - crates/shellwego-cli/src/config.rs
            - crates/shellwego-cli/src/client.rs
          operations:
            - 'Deps: `clap`, `reqwest`, `dirs`.'
            - 'Stub `Config` load/save (for auth tokens).'
            - 'Stub `ApiClient` wrapper.'
        - uuid: 'step-2-cli-commands'
          status: 'todo'
          name: 'Resource Commands'
          reason: |
            Subcommands for every resource type.
          files:
            - crates/shellwego-cli/src/commands/mod.rs
            - crates/shellwego-cli/src/commands/apps.rs
            - crates/shellwego-cli/src/commands/nodes.rs
            - crates/shellwego-cli/src/commands/db.rs
            - crates/shellwego-cli/src/commands/auth.rs
          operations:
            - 'Stub `clap::Subcommand` enums for `AppCommands`, `NodeCommands`, etc.'
            - 'Ensure all API actions (create, list, delete) have CLI equivalents.'
      context_files:
        compact:
          - crates/shellwego-cli/src/main.rs
        medium:
          - crates/shellwego-core/src/lib.rs
        extended: []

  conclusion: |
    Plan complete. The result will be a production-ready file tree with zero implementation drift. Every feature (Databases, Secrets, Domains) has a dedicated home in Core, API, Agent, and CLI. The use of Cargo workspaces ensures we are ready for scale from day one.
  context_files:
    compact:
      - Cargo.toml
    medium:
      - crates/shellwego-core/src/lib.rs
    extended:
      - crates/shellwego-control-plane/src/main.rs
      - crates/shellwego-agent/src/main.rs now execute part 1 plan
Kimi: 
User: Execute part 1 scaffolding plan. Use hacker news netizen language style. Direct send files I will manually copy paste. Comply with plan and intentions
Kimi: Here is the **Part 1: Workspace & Shared Kernel** scaffolding. No implementation, just the bones. Copy-paste ready.