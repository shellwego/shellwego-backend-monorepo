# Plan 06: CLI Tool Completion

## 1. Title & Overview

**CLI Tool Completion** — The `shellwego-cli` crate currently sits at 20% parity with the README feature-set and fails to compile (96 errors due to an unresolved `uuid` crate). This plan (a) fixes the build, (b) completes six existing stub commands (`build`, `logs`, `update`, `completion`, `shell`), and (c) adds seven entirely missing top-level commands (`deploy`, `init`, `health-check`, `backup`, `pricing`, `completion` subcommand registration, `compose` registration in `main.rs`). After execution, the CLI should compile cleanly and every command advertised in the README and help text should either be fully wired or have a clear mock-implementation that can be exercised end-to-end.

## 2. Gap Summary

| # | Readme/Help Claim | Actual State | File(s) | Severity |
|---|---|---|---|---|
| A | `shellwego deploy` | No `deploy` subcommand at top level. `apps deploy` exists under `apps` but only pushes an image — no `--runtime wasm` flag, no Wasm build pipeline | `src/main.rs` (Commands enum) | **HIGH** |
| B | `shellwego init --role=control-plane` | No `init` command at all | `src/main.rs` | **HIGH** |
| C | `shellwego health-check` | No `health-check` command | `src/main.rs` | **MEDIUM** |
| D | `shellwego backup` | No `backup` command. Schema has `Backup`, `RestoreJob` entities but no CLI wiring | `src/main.rs`, `src/commands/` | **MEDIUM** |
| E | `shellwego build --branding` | `build.rs` exists but prints "Build not yet implemented". No `--branding` flag, no actual Docker/buildpack execution | `src/commands/build.rs` | **MEDIUM** |
| F | `shellwego pricing set` | No `pricing` command. Schema has `BillingConfig` with pricing fields but no CLI | `src/main.rs` | **LOW** |
| G | `shellwego logs` | Exists but limited: no `--instance` filter, no `--level` filter, no `--since` parsing, no WebSocket streaming (only HTTP GET) | `src/commands/logs.rs` | **MEDIUM** |
| H | `shellwego update` | Exists but stub: prints "Already at latest version" unconditionally | `src/commands/update.rs` | **LOW** |
| I | Shell completion (`completion` module) | `completion.rs` exists with hand-rolled bash/zsh/fish/powershell stubs. Not registered as a clap subcommand. Does not use `clap_complete` crate for accuracy | `src/completion.rs`, `src/main.rs` | **LOW** |
| J | Build failure — 96 errors | `uuid` crate is a workspace dependency and declared in `Cargo.toml` but the build still fails to resolve `uuid::Uuid` across many files | `Cargo.toml`, all command files | **CRITICAL** |
| K | `compose` command not in `main.rs` | `commands/compose.rs` is fully implemented with import/export/validate but not registered in `Commands` enum or `mod.rs` | `src/main.rs`, `src/commands/mod.rs` | **HIGH** |
| L | `ssh`/`tunnel` commands not in `main.rs` | `commands/ssh.rs` and `commands/tunnel.rs` exist but are not registered | `src/main.rs` | **LOW** |

## 3. Scope

### Files to Modify

| File | Change |
|---|---|
| `crates/shellwego-cli/Cargo.toml` | Add `uuid` as a direct dependency (with `v4`, `serde` features) to fix 96 build errors; add `clap_complete`; add `tokio-tungstenite`, `futures-util` already present — verify; add `indicatif`, `dialoguer` already present |
| `crates/shellwego-cli/src/main.rs` | Register 7 new commands in `Commands` enum: `Deploy`, `Init`, `HealthCheck`, `Backup`, `Pricing`, `Completion`, `Compose`, `Ssh`, `Tunnel`. Wire dispatch. Add `mod shell; mod completion;` declarations |
| `crates/shellwego-cli/src/commands/mod.rs` | Add `pub mod deploy; pub mod init; pub mod health_check; pub mod backup; pub mod pricing; pub mod compose; pub mod ssh; pub mod tunnel;` |
| `crates/shellwego-cli/src/commands/build.rs` | Complete implementation: add `--branding`, `--runtime` flags, actual Docker/buildpack execution via `tokio::process::Command` |
| `crates/shellwego-cli/src/commands/logs.rs` | Add `--instance`, `--level`, parse `--since` to datetime, add WebSocket streaming via `tokio-tungstenite` |
| `crates/shellwego-cli/src/commands/update.rs` | Implement GitHub Releases API check, binary download, checksum verification, self-replacement |
| `crates/shellwego-cli/src/completion.rs` | Replace hand-rolled stubs with `clap_complete` generated completions |
| `crates/shellwego-cli/src/client.rs` | Add API methods: `health_check()`, `create_backup()`, `list_backups()`, `restore_backup()`, `get_pricing()`, `set_pricing()`, `stream_logs_ws()` |
| `crates/shellwego-cli/src/config.rs` | Add `role`, `region`, `provider` fields for `init` command persistence |

### New Files to Create

| File | Purpose |
|---|---|
| `crates/shellwego-cli/src/commands/deploy.rs` | `shellwego deploy` — deploy app with `--runtime`, `--image`, `--env`, `--strategy` flags |
| `crates/shellwego-cli/src/commands/init.rs` | `shellwego init` — scaffold project config with `--role`, `--region`, `--provider` |
| `crates/shellwego-cli/src/commands/health_check.rs` | `shellwego health-check` — check API, node, and app health |
| `crates/shellwego-cli/src/commands/backup.rs` | `shellwego backup` — create/restore/list backups with subcommands |
| `crates/shellwego-cli/src/commands/pricing.rs` | `shellwego pricing` — get/set pricing plans with subcommands |

## 4. Prerequisites

### P1. Fix 96 build errors (CRITICAL — must be first)

**Root cause:** The `uuid` crate is declared as a workspace dependency in `Cargo.toml` and listed in `crates/shellwego-cli/Cargo.toml`:
```toml
uuid = { workspace = true }
```
However, the workspace `uuid` entry specifies features `["v4", "serde"]` but `Cargo.lock` may be stale, or the resolver may not propagate correctly to the CLI crate.

**Fix:** Ensure `crates/shellwego-cli/Cargo.toml` has:
```toml
uuid = { workspace = true, features = ["v4", "serde"] }
```
This explicitly re-declares the features. If the workspace entry is `uuid = { version = "1.6", features = ["v4", "serde"] }`, the explicit local override guarantees resolution.

**Verification:** Run `cargo check -p shellwego-cli` — must produce 0 errors before proceeding.

### P2. `clap_complete` dependency

Add to `crates/shellwego-cli/Cargo.toml`:
```toml
clap_complete = "4.4"
```

### P3. `self_update` dependency (for real update mechanism)

Add to `crates/shellwego-cli/Cargo.toml`:
```toml
self_update = { version = "0.40", features = ["archive-tar", "archive-zip", "compression-flate2"] }
```
> Alternative: implement manually using `reqwest` + `tempfile` + `std::fs::rename` if `self_update` crate introduces too many transitive dependencies. The manual approach is recommended for lean binaries.

### P4. WebSocket streaming dependency

`tokio-tungstenite = "0.21"` and `futures-util = "0.3"` are already in `Cargo.toml`. No change needed.

## 5. Detailed Implementation Steps

### Phase 0: Build Fix & Module Registration

**Step 0.1 — Fix `Cargo.toml`**

File: `crates/shellwego-cli/Cargo.toml`

Update the `uuid` line to explicitly declare features:
```toml
uuid = { workspace = true, features = ["v4", "serde"] }
```

Add new dependencies:
```toml
clap_complete = "4.4"
```

**Step 0.2 — Register all modules in `mod.rs`**

File: `crates/shellwego-cli/src/commands/mod.rs`

Add module declarations:
```rust
pub mod backup;
pub mod compose;
pub mod deploy;
pub mod health_check;
pub mod init;
pub mod pricing;
pub mod ssh;
pub mod tunnel;
```

> `ssh` and `tunnel` modules already exist on disk but are not declared in `mod.rs`.

**Step 0.3 — Register all commands in `main.rs`**

File: `crates/shellwego-cli/src/main.rs`

Add module declarations at top:
```rust
mod completion;
mod shell;
```

Extend the `Commands` enum:
```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing variants ...

    /// Deploy an application (shortcut for apps deploy)
    Deploy(commands::deploy::DeployArgs),

    /// Initialize a new ShellWeGo project or control-plane node
    Init(commands::init::InitArgs),

    /// Check health of API, nodes, and apps
    #[command(name = "health-check")]
    HealthCheck(commands::health_check::HealthCheckArgs),

    /// Manage backups (create, restore, list)
    Backup(commands::backup::BackupArgs),

    /// Manage pricing plans (requires admin)
    Pricing(commands::pricing::PricingArgs),

    /// Generate shell completion scripts
    Completion(CompletionArgs),

    /// Import/export Docker Compose files
    #[command(alias = "compose")]
    Compose(commands::compose::ComposeArgs),

    /// SSH into a running app instance
    Ssh(commands::ssh::SshArgs),

    /// Create tunnel to a service
    Tunnel(commands::tunnel::TunnelArgs),
}
```

Add the `CompletionArgs` struct:
```rust
#[derive(clap::Args)]
struct CompletionArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    shell: clap_complete::Shell,
}
```

Extend the dispatch `match` block:
```rust
Commands::Deploy(args) => commands::deploy::handle(args, &config, cli.output).await,
Commands::Init(args) => commands::init::handle(args, &mut config).await,
Commands::HealthCheck(args) => commands::health_check::handle(args, &config).await,
Commands::Backup(args) => commands::backup::handle(args, &config, cli.output).await,
Commands::Pricing(args) => commands::pricing::handle(args, &config, cli.output).await,
Commands::Completion(args) => {
    let mut app = Cli::command();
    clap_complete::generate(args.shell, &mut app, "shellwego", &mut std::io::stdout());
    Ok(())
}
Commands::Compose(args) => commands::compose::handle(args, &config).await,
Commands::Ssh(args) => commands::ssh::handle(args, &config).await,
Commands::Tunnel(args) => commands::tunnel::handle(args, &config).await,
```

> Note: `compose`, `ssh`, and `tunnel` files already exist. We only need to ensure their `handle()` signatures match. If they take different argument patterns, adjust the dispatch accordingly.

**Step 0.4 — Verify build**

Run `cargo check -p shellwego-cli` — must pass with 0 errors.

---

### Phase 1: `shellwego deploy` (New Command)

**File:** `crates/shellwego-cli/src/commands/deploy.rs`

**Arguments:**
```rust
#[derive(Args)]
pub struct DeployArgs {
    /// App ID or name to deploy
    app: String,

    /// Container image to deploy
    #[arg(short, long)]
    image: Option<String>,

    /// Runtime type (container, wasm)
    #[arg(short, long, default_value = "container", value_enum)]
    runtime: RuntimeType,

    /// Environment variables (KEY=VALUE, can be repeated)
    #[arg(short = 'e', long = "env")]
    env_vars: Vec<String>,

    /// Deployment strategy
    #[arg(short, long, default_value = "rolling", value_enum)]
    strategy: DeployStrategy,

    /// Wait for deployment to complete
    #[arg(short, long)]
    wait: bool,

    /// Timeout in seconds (for --wait)
    #[arg(short, long, default_value = "300")]
    timeout: u64,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum RuntimeType {
    Container,
    Wasm,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum DeployStrategy {
    Rolling,
    BlueGreen,
    Canary,
    Immediate,
}
```

**Behavior:**
1. Resolve `app` — if it looks like a UUID (36 chars with dashes), use as ID; otherwise search apps by name via `client.list_apps()` and pick first match.
2. If `--image` not provided and runtime is `container`, prompt via `dialoguer::Input` or error.
3. If runtime is `wasm`:
   - Build the Wasm module locally using `cargo build --target wasm32-unknown-unknown --release` (spawn `tokio::process::Command`)
   - Upload the `.wasm` file to the API at `POST /v1/apps/{id}/deploy` with `Content-Type: application/wasm` and header `X-Runtime: wasm`
4. If runtime is `container`, call `client.deploy_app(id, image)`.
5. If `--wait`, poll `GET /v1/apps/{id}` every 2 seconds until `status` is `Running` or timeout expires.
6. If `--env`, parse each `KEY=VALUE` and send as `env` in the deploy request body.

**API Calls:**
- `POST /v1/apps/{id}/deploy` — body: `{ image, runtime, env, strategy }`
- `GET /v1/apps/{id}` — polling (if `--wait`)

**Output:**
```
→ Deploying my-app (runtime: wasm, strategy: rolling)
→ Building Wasm module...
→ Uploading 2.3 MB Wasm artifact...
→ Deployment queued (ID: dep-abc123)
✓ Deployment complete in 12s
```

---

### Phase 2: `shellwego init` (New Command)

**File:** `crates/shellwego-cli/src/commands/init.rs`

**Arguments:**
```rust
#[derive(Args)]
pub struct InitArgs {
    /// Role for this installation
    #[arg(short, long, value_enum, default_value = "app")]
    role: InitRole,

    /// Cloud provider for control-plane
    #[arg(short, long)]
    provider: Option<Provider>,

    /// Region for control-plane nodes
    #[arg(short, long)]
    region: Option<String>,

    /// Project name (creates shellwego.toml)
    #[arg(short, long)]
    name: Option<String>,

    /// Force overwrite existing config
    #[arg(short, long)]
    force: bool,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum InitRole {
    App,
    ControlPlane,
    Agent,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Provider {
    Aws,
    Gcp,
    Azure,
    Hetzner,
    Baremetal,
}
```

**Behavior:**

**For `--role=app`:**
1. Check if `shellwego.toml` exists in current directory. If yes and `--force` not set, error.
2. Prompt for project name if not provided.
3. Prompt for default region.
4. Write `shellwego.toml`:
```toml
[project]
name = "my-app"
api_url = "http://localhost:8080"
region = "us-east-1"

[build]
runtime = "container"
dockerfile = "Dockerfile"

[deploy]
strategy = "rolling"
replicas = 1
```
5. Create `.shellwego/` directory (gitignored).

**For `--role=control-plane`:**
1. Require `--provider` and `--region`. Prompt if missing.
2. Generate a control-plane config directory structure:
   ```
   shellwego-config/
   ├── config.toml
   ├── jwt-private.pem  (generated)
   ├── jwt-public.pem   (generated)
   └── nodes/
   ```
3. Write `config.toml` with provider-specific defaults:
   - AWS: default instance type `t3.medium`, AMI pattern
   - GCP: default machine type `e2-medium`
   - Hetzner: default `cx22`
4. Print next steps:
```
✓ Initialized control-plane config in ./shellwego-config/
  Next steps:
  1. Review and edit shellwego-config/config.toml
  2. Set environment variables (see .env.example)
  3. Run: shellwego control-plane start
```

**For `--role=agent`:**
1. Generate agent config `shellwego-agent.toml` with control-plane URL, node name, and capabilities.
2. Print join token generation instructions.

**API Calls:** None (local file generation only).

**Config changes in `config.rs`:**
Add fields to `CliConfig`:
```rust
pub role: Option<String>,
pub region: Option<String>,
pub provider: Option<String>,
```

---

### Phase 3: `shellwego health-check` (New Command)

**File:** `crates/shellwego-cli/src/commands/health_check.rs`

**Arguments:**
```rust
#[derive(Args)]
pub struct HealthCheckArgs {
    /// Specific component to check (api, nodes, apps, all)
    #[arg(short, long, default_value = "all", value_enum)]
    component: HealthComponent,

    /// App ID to check (only with --component=apps)
    #[arg(short, long)]
    app: Option<uuid::Uuid>,

    /// Output as JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum HealthComponent {
    All,
    Api,
    Nodes,
    Apps,
}
```

**Behavior:**
1. **API health:** `GET /v1/health` — measure response time, check status code 200, parse response body for version/uptime.
2. **Node health:** `GET /v1/nodes` — for each node, check `status` field, report count of healthy/unhealthy/draining.
3. **App health:** If `--app` specified, `GET /v1/apps/{id}` — check status is `Running`. If no `--app`, list all apps and report status distribution.
4. **All:** Run all three checks, produce summary table.

**Output:**
```
ShellWeGo Health Check
═══════════════════════

API     ✓ Healthy  (42ms, v0.1.0-alpha.1, uptime: 3d 14h)
Nodes   ✓ 3/3 healthy
Apps    ⚠ 4/5 running (1 failed: web-frontend)

Overall: DEGRADED
```

**API Calls:**
- `GET /v1/health`
- `GET /v1/nodes`
- `GET /v1/apps` or `GET /v1/apps/{id}`

**Client addition in `client.rs`:**
```rust
pub async fn health_check(&self) -> anyhow::Result<serde_json::Value> {
    self.get("/v1/health").await
}
```

---

### Phase 4: `shellwego backup` (New Command)

**File:** `crates/shellwego-cli/src/commands/backup.rs`

**Arguments:**
```rust
#[derive(Args)]
pub struct BackupArgs {
    #[command(subcommand)]
    command: BackupCommands,
}

#[derive(Subcommand)]
enum BackupCommands {
    /// Create a new backup
    Create {
        /// Resource type to backup
        #[arg(short, long, value_enum)]
        resource_type: BackupResourceType,

        /// Resource ID
        #[arg(short, long)]
        resource_id: uuid::Uuid,

        /// Backup description
        #[arg(short, long)]
        description: Option<String>,

        /// Encryption key ID
        #[arg(long)]
        encryption_key: Option<String>,
    },

    /// Restore from a backup
    Restore {
        /// Backup ID to restore from
        backup_id: uuid::Uuid,

        /// Target resource ID (defaults to original)
        #[arg(short, long)]
        target: Option<uuid::Uuid>,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },

    /// List backups
    List {
        /// Filter by resource type
        #[arg(short, long, value_enum)]
        resource_type: Option<BackupResourceType>,

        /// Filter by resource ID
        #[arg(short, long)]
        resource_id: Option<uuid::Uuid>,

        /// Show expired backups
        #[arg(long)]
        show_expired: bool,
    },

    /// Delete a backup
    Delete {
        backup_id: uuid::Uuid,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum BackupResourceType {
    App,
    Database,
    Volume,
}
```

**Behavior:**

**`create`:**
1. Call `POST /v1/backups` with body `{ resource_type, resource_id, description, encryption_key_id }`.
2. Display backup ID and status.
3. If `--wait` desired, poll `GET /v1/backups/{id}` until `status == Completed`.

**`restore`:**
1. If `--yes` not set, confirm via `dialoguer::Confirm` with warning message.
2. Call `POST /v1/backups/{id}/restore` with body `{ target_resource_id }`.
3. Display restore job ID.

**`list`:**
1. Call `GET /v1/backups?resource_type=...&resource_id=...`.
2. Display in table format:
```
ID          Type      Status     Size     Created
abc123...   Database  Completed  2.3 GB   2026-04-01 14:30
def456...   App       Pending    —        2026-04-05 09:15
```

**API Calls:**
- `POST /v1/backups` — create
- `GET /v1/backups` — list (with query params)
- `GET /v1/backups/{id}` — get status
- `POST /v1/backups/{id}/restore` — restore
- `DELETE /v1/backups/{id}` — delete

**Client additions in `client.rs`:**
```rust
pub async fn create_backup(&self, req: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    self.post("/v1/backups", req).await
}

pub async fn list_backups(&self, resource_type: Option<&str>, resource_id: Option<uuid::Uuid>) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut path = "/v1/backups".to_string();
    let mut params = vec![];
    if let Some(rt) = resource_type { params.push(format!("resource_type={}", rt)); }
    if let Some(rid) = resource_id { params.push(format!("resource_id={}", rid)); }
    if !params.is_empty() { path = format!("{}?{}", path, params.join("&")); }
    self.get(&path).await
}

pub async fn restore_backup(&self, backup_id: uuid::Uuid, target_id: Option<uuid::Uuid>) -> anyhow::Result<serde_json::Value> {
    self.post(&format!("/v1/backups/{}/restore", backup_id), &serde_json::json!({ "target_resource_id": target_id })).await
}

pub async fn delete_backup(&self, backup_id: uuid::Uuid) -> anyhow::Result<()> {
    self.delete(&format!("/v1/backups/{}", backup_id)).await
}
```

---

### Phase 5: `shellwego build --branding` (Complete Existing Stub)

**File:** `crates/shellwego-cli/src/commands/build.rs`

**Updated Arguments:**
```rust
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

    /// Runtime to build for
    #[arg(short, long, default_value = "container", value_enum)]
    runtime: BuildRuntime,

    /// Custom branding (HTML, CSS, logo URL)
    #[arg(long)]
    branding: Option<String>,

    /// Registry to push to
    #[arg(long)]
    registry: Option<String>,

    /// Dockerfile path (default: Dockerfile)
    #[arg(long)]
    dockerfile: Option<String>,

    /// No cache
    #[arg(long)]
    no_cache: bool,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum BuildRuntime {
    Container,
    Wasm,
}
```

**Behavior:**

**For `--runtime=container`:**
1. Detect Dockerfile at `args.path/Dockerfile` (or `--dockerfile` override).
2. Construct Docker command: `docker build` with `--build-arg` for each `--build-arg`, `--tag`, `--no-cache` if set.
3. Use `indicatif::ProgressBar` to show build progress (parse Docker output for step indicators).
4. If `--push`, run `docker push {tag}`.
5. If `--registry` specified, prefix tag: `{registry}/{tag}`.

**For `--runtime=wasm`:**
1. Check `Cargo.toml` exists in `args.path`.
2. Run `cargo build --target wasm32-unknown-unknown --release --manifest-path {path}/Cargo.toml`.
3. Locate output at `target/wasm32-unknown-unknown/release/{name}.wasm`.
4. Run `wasm-opt -Oz` if `wasm-opt` is on PATH (optional optimization step, non-fatal).
5. Report output size.

**For `--branding`:**
1. Accept a JSON file path: `{ "primary_color": "#...", "logo_url": "...", "custom_css": "..." }`.
2. Inject branding into the build context as a build arg: `--build-arg BRANDING_CONFIG={json_content}`.
3. Print confirmation of applied branding.

**Output:**
```
→ Building from . (runtime: container)
  Step 1/12 : FROM node:20-alpine
  Step 2/12 : COPY package*.json ./
  ...
✓ Build complete (image: my-registry/my-app:latest, 127 MB, 34s)
```

---

### Phase 6: `shellwego logs` (Enhance Existing)

**File:** `crates/shellwego-cli/src/commands/logs.rs`

**Updated Arguments:**
```rust
#[derive(Args)]
pub struct LogArgs {
    /// App ID to stream logs from
    app_id: uuid::Uuid,

    /// Follow log stream (live tail)
    #[arg(short, long)]
    follow: bool,

    /// Number of lines to show from the end
    #[arg(short, long, default_value = "100")]
    tail: usize,

    /// Show logs since timestamp (RFC 3339) or duration (e.g., 1h, 30m)
    #[arg(short, long)]
    since: Option<String>,

    /// Filter by instance ID
    #[arg(long)]
    instance: Option<uuid::Uuid>,

    /// Filter by log level
    #[arg(long, value_enum)]
    level: Option<LogLevel>,

    /// Show timestamps
    #[arg(short, long)]
    timestamps: bool,

    /// Filter by pattern (substring match)
    #[arg(short, long)]
    grep: Option<String>,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}
```

**Behavior:**

1. **Parse `--since`:** Accept either RFC 3339 (`2026-04-05T14:30:00Z`) or human duration (`1h`, `30m`, `2d`). Convert to Unix timestamp or ISO string for API query param.

2. **Initial fetch:** `GET /v1/apps/{id}/logs?tail={tail}&since={since}&instance={instance}&level={level}`
   - Parse response as newline-delimited JSON: `{ "timestamp": "...", "level": "info", "message": "...", "instance_id": "..." }`
   - If `--grep` specified, filter lines client-side.
   - If `--timestamps` specified, prefix each line with the timestamp.

3. **Follow mode (`--follow`):** Upgrade to WebSocket at `ws://{api_url}/v1/apps/{id}/logs/ws`:
   ```rust
   let (ws_stream, _) = tokio_tungstenite::connect_async(
       format!("ws://{}/v1/apps/{}/logs/ws?instance={}&level={}",
               base_url, id,
               instance.map_or(String::new(), |i| i.to_string()),
               level.map_or(String::new(), |l| format!("{:?}", l).to_lowercase()))
   ).await?;

   let (_, read) = ws_stream.split();
   futures_util::pin_mut!(read);

   while let Some(msg) = read.next().await {
       let msg = msg?;
       let text = msg.to_text()?;
       // Parse, filter, print
   }
   ```
   - Handle Ctrl+C gracefully (print `\n[Stream closed]` and exit).

**Client addition in `client.rs`:**
```rust
pub async fn get_logs_filtered(
    &self,
    app_id: uuid::Uuid,
    tail: usize,
    since: Option<&str>,
    instance: Option<uuid::Uuid>,
    level: Option<&str>,
) -> anyhow::Result<String> {
    let mut params = vec![format!("tail={}", tail)];
    if let Some(s) = since { params.push(format!("since={}", s)); }
    if let Some(i) = instance { params.push(format!("instance={}", i)); }
    if let Some(l) = level { params.push(format!("level={}", l)); }
    let path = format!("/v1/apps/{}/logs?{}", app_id, params.join("&"));
    self.get(&path).await
}

pub async fn logs_ws_url(&self, app_id: uuid::Uuid) -> String {
    format!("{}/v1/apps/{}/logs/ws", self.base_url.replace("http", "ws"), app_id)
}
```

---

### Phase 7: `shellwego update` (Complete Existing Stub)

**File:** `crates/shellwego-cli/src/commands/update.rs`

**Behavior (manual implementation — no `self_update` crate dependency):**

```rust
pub async fn handle() -> anyhow::Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    println!("{} Checking for updates... (current: {})", "→".blue(), current_version);

    // 1. Query GitHub Releases API
    let client = reqwest::Client::builder()
        .user_agent("shellwego-cli")
        .build()?;

    let resp: serde_json::Value = client
        .get("https://api.github.com/repos/shellwego/shellwego/releases/latest")
        .send()
        .await?
        .json()
        .await?;

    let latest_version = resp["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v');

    // 2. Compare versions
    if version_is_newer(current_version, latest_version) {
        println!("{} New version available: {} → {}", "✓".green(), current_version, latest_version);

        // 3. Ask for confirmation
        if !Confirm::new()
            .with_prompt("Update now?")
            .default(true)
            .interact()?
        {
            println!("Skipped. Run `curl -fsSL https://shellwego.com/install-cli.sh | bash` to update manually.");
            return Ok(());
        }

        // 4. Determine platform and download URL
        let asset = find_asset_for_platform(&resp)?;
        let download_url = asset["browser_download_url"].as_str().unwrap_or("");

        println!("{} Downloading {}...", "→".blue(), asset["name"].as_str().unwrap_or("binary"));

        // 5. Download to temp file
        let tmp_path = std::env::temp_dir().join("shellwego-update");
        let mut resp = client.get(download_url).send().await?;
        let bytes = resp.bytes().await?;
        std::fs::write(&tmp_path, &bytes)?;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))?;

        // 6. Self-replace
        let current_exe = std::env::current_exe()?;
        std::fs::rename(&tmp_path, &current_exe)?;

        println!("{} Updated to version {}", "✓".green().bold(), latest_version);
    } else {
        println!("{} Already at latest version ({})", "✓".green(), current_version);
    }

    Ok(())
}

fn version_is_newer(current: &str, latest: &str) -> bool {
    // Parse semver and compare
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };
    parse(latest) > parse(current)
}

fn find_asset_for_platform(release: &serde_json::Value) -> anyhow::Result<&serde_json::Value> {
    let os = std::env::consts::OS;     // "linux", "macos", "windows"
    let arch = std::env::consts::ARCH; // "x86_64", "aarch64"

    let target = match (os, arch) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc.exe",
        other => other.0,
    };

    release["assets"].as_array()
        .and_then(|arr| arr.iter().find(|a| {
            a["name"].as_str().unwrap_or("").contains(target)
        }))
        .ok_or_else(|| anyhow::anyhow!("No binary found for platform: {}-{}", os, arch))
}
```

> **Fallback:** If the GitHub API is unreachable, print: `"Unable to check for updates. Update manually: curl -fsSL https://shellwego.com/install-cli.sh | bash"`

---

### Phase 8: `shellwego pricing` (New Command)

**File:** `crates/shellwego-cli/src/commands/pricing.rs`

**Arguments:**
```rust
#[derive(Args)]
pub struct PricingArgs {
    #[command(subcommand)]
    command: PricingCommands,
}

#[derive(Subcommand)]
enum PricingCommands {
    /// Show current pricing plans
    List {
        /// Filter by resource type
        #[arg(short, long, value_enum)]
        resource: Option<PricingResource>,
    },

    /// Set pricing for a resource type (admin only)
    Set {
        /// Resource type
        #[arg(short, long, value_enum)]
        resource: PricingResource,

        /// Price per unit (e.g., "0.000231" per vCPU-second)
        #[arg(short, long)]
        price: f64,

        /// Currency code
        #[arg(short, long, default_value = "USD")]
        currency: String,

        /// Billing unit (per-hour, per-GB, per-request)
        #[arg(short, long, default_value = "per-hour")]
        unit: BillingUnit,

        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum PricingResource {
    Cpu,
    Memory,
    Storage,
    Bandwidth,
    Database,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum BillingUnit {
    PerHour,
    PerGB,
    PerRequest,
    PerMilliCpuSecond,
}
```

**Behavior:**

**`list`:**
1. Call `GET /v1/pricing` — returns list of pricing entries.
2. Display in table:
```
Resource    Price           Unit              Currency
CPU         $0.0000417      per-milliCPU-s     USD
Memory      $0.00000445     per-GB             USD
Storage     $0.0000556      per-GB-hour        USD
Bandwidth   $0.09           per-GB             USD
```

**`set`:**
1. Require admin authentication (check `config.get_token()` and verify admin role via API).
2. If `--yes` not set, confirm: `"Set CPU pricing to $0.0000417 per-milliCPU-s (USD)? [y/N]"`.
3. Call `PUT /v1/pricing/{resource}` with body `{ price, currency, unit }`.
4. Print confirmation.

**API Calls:**
- `GET /v1/pricing` — list all pricing
- `PUT /v1/pricing/{resource}` — set pricing

**Client additions:**
```rust
pub async fn get_pricing(&self) -> anyhow::Result<Vec<serde_json::Value>> {
    self.get("/v1/pricing").await
}

pub async fn set_pricing(&self, resource: &str, price: f64, currency: &str, unit: &str) -> anyhow::Result<serde_json::Value> {
    self.put(&format!("/v1/pricing/{}", resource), &serde_json::json!({
        "price": price,
        "currency": currency,
        "unit": unit,
    })).await
}
```
> Note: `ApiClient` currently has no `put()` method. Add it following the same pattern as `patch()`.

---

### Phase 9: Shell Completion (Rewrite Existing)

**File:** `crates/shellwego-cli/src/completion.rs`

Replace the entire file. The `Completion` command in `main.rs` already uses `clap_complete::generate()` directly. The standalone `completion.rs` module is no longer needed for the subcommand path, but keep it for programmatic use (e.g., from the REPL shell):

```rust
//! Shell completion generation
//!
//! Uses clap_complete for accurate, auto-derived completions.
//! This module is kept for programmatic access (e.g., from the REPL).

use clap::Command;

/// Generate shell completion script for the given shell
pub fn generate(shell: clap_complete::Shell, app: &mut Command) -> String {
    let mut buf = Vec::new();
    clap_complete::generate(shell, app, "shellwego", &mut buf);
    String::from_utf8(buf).unwrap_or_default()
}

/// Generate completion for a shell name string
pub fn generate_by_name(shell_name: &str, app: &mut Command) -> anyhow::Result<String> {
    let shell = match shell_name {
        "bash" => clap_complete::Shell::Bash,
        "zsh" => clap_complete::Shell::Zsh,
        "fish" => clap_complete::Shell::Fish,
        "powershell" => clap_complete::Shell::PowerShell,
        "elvish" => clap_complete::Shell::Elvish,
        other => anyhow::bail!("Unsupported shell: {}. Supported: bash, zsh, fish, powershell, elvish", other),
    };
    Ok(generate(shell, app))
}
```

---

### Phase 10: Existing Command Verification & Wiring

**Step 10.1 — Verify `compose` registration**

File: `crates/shellwego-cli/src/commands/compose.rs`

The `handle()` signature is:
```rust
pub async fn handle(args: ComposeArgs, config: &CliConfig) -> anyhow::Result<()>
```
This matches the dispatch in `main.rs`. No changes needed in the file itself — just register in `mod.rs` and `main.rs`.

**Step 10.2 — Verify `ssh` registration**

Read `crates/shellwego-cli/src/commands/ssh.rs` to confirm `handle()` signature matches. If it takes additional arguments (e.g., an app name), ensure the dispatch passes them correctly.

**Step 10.3 — Verify `tunnel` registration**

Read `crates/shellwego-cli/src/commands/tunnel.rs` to confirm `handle()` signature matches.

**Step 10.4 — Verify `client()` helper in `main.rs`**

The `client()` helper function at the bottom of `main.rs` uses `config.token.clone()` but `config` is immutable in most dispatch branches (only `Auth` gets `&mut config`). The `token` field is `Option<String>`, and `get_token()` is the proper accessor. Update:
```rust
fn client(config: &CliConfig) -> anyhow::Result<ApiClient> {
    let token = config.get_token()
        .ok_or_else(|| anyhow::anyhow!("Not authenticated. Run `shellwego auth login`"))?;
    ApiClient::new(&config.api_url, &token)
}
```

---

## 6. Dependencies on Other Plans

| Plan ID | Dependency | Notes |
|---|---|---|
| **01** (Security Hardening) | Partial — RBAC | `pricing set` and `backup restore` require admin permissions. If Plan 01 wires RBAC, these commands must include permission checks. If not yet done, these commands should still function (admin check will pass/fail based on current middleware). |
| **03** (API Endpoints) | High — API routes must exist | All new CLI commands call API endpoints (`/v1/health`, `/v1/backups`, `/v1/pricing`, `/v1/apps/{id}/deploy` with wasm support). If the control-plane does not implement these routes, the CLI will get 404s. Plan should include graceful error messages: `"The server does not support this endpoint. You may need to update the control-plane."` |
| **05** (Schema Consolidation) | Medium | The `Backup`, `RestoreJob`, `Build`, `Deployment` schema entities already exist but may move during schema consolidation. Use re-exports from `shellwego_schema::entities::*`. |
| **02** (Agent) | Low — Wasm deploy | The `--runtime wasm` deploy path sends a Wasm module to the control-plane, which distributes it to agents. If the agent doesn't support Wasm execution, the deploy will fail at runtime — acceptable. |

**Recommended execution order:** This plan can execute in parallel with Plans 01–05 but should be tested after Plan 03 (API endpoints) to verify end-to-end behavior. Phase 0 (build fix + module registration) should be done first and can land independently.

## 7. Acceptance Criteria

### Build & Compilation
- [ ] `cargo check -p shellwego-cli` passes with 0 errors, 0 unresolved imports
- [ ] `cargo build -p shellwego-cli` succeeds (release build)
- [ ] Binary is `shellwego` and runs `shellwego --help` without error
- [ ] `shellwego --help` lists all 20 commands: auth, apps, nodes, volumes, domains, databases, secrets, logs, exec, status, top, update, deploy, init, health-check, backup, pricing, completion, compose, ssh, tunnel

### New Commands
- [ ] `shellwego deploy my-app --image nginx:latest` succeeds (or returns clear API error if server not running)
- [ ] `shellwego deploy my-app --runtime wasm` attempts local Wasm build and uploads
- [ ] `shellwego init --role=app --name my-app` creates `shellwego.toml` in CWD
- [ ] `shellwego init --role=control-plane --provider aws --region us-east-1` creates config directory
- [ ] `shellwego health-check` reports API/node/app health status with exit code 0 (healthy) or 1 (degraded)
- [ ] `shellwego backup create --resource-type database --resource-id <uuid>` calls API
- [ ] `shellwego backup list` displays table of backups
- [ ] `shellwego backup restore <id> --yes` calls API
- [ ] `shellwego pricing list` displays pricing table
- [ ] `shellwego completion bash > /tmp/shellwego.bash` generates valid bash completion
- [ ] `shellwego completion zsh` generates valid zsh completion

### Enhanced Commands
- [ ] `shellwego build --runtime container ./my-app` runs `docker build` and reports progress
- [ ] `shellwego build --runtime wasm ./my-app` runs `cargo build --target wasm32-unknown-unknown`
- [ ] `shellwego build --branding branding.json` injects branding config as build arg
- [ ] `shellwego logs <app-id> --follow --level error --timestamps` streams logs with filtering
- [ ] `shellwego logs <app-id> --since 1h` correctly parses duration and filters
- [ ] `shellwego logs <app-id> --grep "error"` filters client-side
- [ ] `shellwego update` checks GitHub Releases API and reports version status
- [ ] `shellwego compose import docker-compose.yml` (existing) still works after registration

### CLI UX
- [ ] All commands show `--help` with accurate descriptions and examples
- [ ] Global flags `--config`, `--api-url`, `--output`, `--quiet` work on all commands
- [ ] Error messages are actionable (not just "API error: 404")
- [ ] `shellwego status` reports connectivity to API server

### Tests
- [ ] `cargo test -p shellwego-cli` passes with 0 failures
- [ ] Unit test for `version_is_newer()` function (semver comparison)
- [ ] Unit test for `parse_since()` duration parser
- [ ] Unit test for `find_asset_for_platform()` release asset resolution
- [ ] Integration test: `assert_cmd` for `shellwego --help`, `shellwego init --help`, etc.

## 8. Estimated Complexity

**L** (Large)

Rationale:
- **Phase 0** (build fix + registration): ~80 lines across 3 files. Mechanical but essential. Low complexity.
- **Phase 1** (`deploy`): ~120 lines new. Medium complexity (Wasm build subprocess + deploy API).
- **Phase 2** (`init`): ~150 lines new. Medium complexity (file generation, provider templates).
- **Phase 3** (`health-check`): ~100 lines new. Low complexity (API calls + table formatting).
- **Phase 4** (`backup`): ~180 lines new. Medium complexity (4 subcommands, confirmation prompts).
- **Phase 5** (`build` complete): ~150 lines changed. Medium complexity (Docker subprocess, progress parsing, Wasm build).
- **Phase 6** (`logs` enhance): ~120 lines changed. Medium complexity (WebSocket streaming, time parsing, filtering).
- **Phase 7** (`update` complete): ~100 lines changed. Medium complexity (GitHub API, self-replacement).
- **Phase 8** (`pricing`): ~100 lines new. Low complexity (2 subcommands, admin check).
- **Phase 9** (completion): ~30 lines. Low complexity (clap_complete integration).
- **Phase 10** (wiring verification): ~20 lines. Trivial.

**Total: ~1,130 lines of production code + ~80 lines of test code.**

The large surface area is spread across many small, independent commands — most are straightforward CRUD wrappers around API calls. The highest complexity items are the `deploy --runtime wasm` path (subprocess management), `logs --follow` (WebSocket streaming), and `update` (self-replacement).

## 9. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **Build fix may reveal cascading errors** — fixing `uuid` may expose other compilation issues in existing command files | Medium | Medium — could add significant rework | Run `cargo check` immediately after uuid fix before proceeding to new code. Fix any cascade errors first. |
| **API endpoints don't exist** — new commands call endpoints that the control-plane hasn't implemented | High | Medium — commands return 404 | Each command handler should check HTTP status and print a helpful message: `"This feature requires control-plane v0.2.0 or later. Your server returned: 404 Not Found."` |
| **Docker not available** — `shellwego build --runtime container` spawns `docker` | High (on dev machines without Docker) | Low — build command fails with clear error | Check `docker --version` before attempting build. If missing, print: `"Docker not found. Install Docker or use --runtime wasm."` |
| **Self-update race condition** — replacing the running binary may fail on some OS | Medium | Low — user can always re-install | On Windows, rename to `.old` first, then move. On Linux/macOS, `std::fs::rename` is atomic on same filesystem. Catch errors and print manual update instructions as fallback. |
| **WebSocket log streaming incompatible with API** — `tokio-tungstenite` may not match the server's WS implementation | Medium | Medium — `--follow` fails | Fall back to HTTP polling if WS connection fails. Implement: try WS, catch error, warn user, retry with HTTP polling every 2s. |
| **`clap_complete` version mismatch** — crate version must be compatible with `clap 4.4` | Low | Low — compilation error | Pin `clap_complete = "4.4"` explicitly (same major version as clap). |
| **Wasm toolchain not installed** — `wasm32-unknown-unknown` target may not be present | High (first-time users) | Low — build fails with clear error | Check `rustup target list --installed` for `wasm32-unknown-unknown`. If missing, print: `"Wasm target not installed. Run: rustup target add wasm32-unknown-unknown"` |
| **Large number of files to modify** — 15+ files touched | Medium | Low — merge conflicts with parallel plans | Keep changes minimal and mechanical. Phase 0 (build fix) can land independently. Other phases are additive (new files). |
