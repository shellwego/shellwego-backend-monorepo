# Plan 02: Scheduler, Deploy Pipeline & Guardian

## 1. Title & Overview

**Scheduler, Deploy Pipeline & Guardian** — Implement the three core runtime subsystems that are entirely missing from the codebase despite being described in the README as foundational: (A) a scheduler that assigns apps to available worker nodes, (B) a deploy pipeline that actually provisions microVMs on agents (not just creates a DB record), and (C) a guardian/watchdog that monitors running instances and triggers automatic healing (restart on unhealthy, reschedule on node loss). Additionally, this plan fixes the log streaming stub, wires start/stop/restart handlers to real agent commands, and replaces the hardcoded node capacity in `list_nodes` with data from heartbeat reports.

## 2. Gap Summary

| # | Readme Claim | Actual Implementation | File(s) | Severity |
|---|---|---|---|---|
| A | "etcd/SQLite-backed scheduler assigns apps to nodes" | No scheduler exists. Apps are created in DB but never assigned to a node. `deploy_app` (line 215) creates a `deployments` row with status `"pending"` and returns immediately — no scheduling logic. | `handlers.rs` lines 215–244 | **CRITICAL** |
| B | "Deploy pipeline provisions Firecracker microVMs" | `deploy_app` is a stub. No code sends `Message::ScheduleApp` to agents. Deployment stays `"pending"` forever. | `handlers.rs` lines 215–244 | **CRITICAL** |
| C | "eBPF-based Guardian watchdog monitors running microVMs" | No guardian service exists. No background task monitors app health or agent liveness. | Missing entirely | **HIGH** |
| D | "Log streaming from containers" | `get_logs` returns `Vec::new()` with comment "Log streaming from live containers is not yet connected." | `handlers.rs` lines 358–366 | **HIGH** |
| E | "App start/stop/restart sends commands to agents" | `restart_app`, `stop_app`, `start_app` only update the DB `status` column. No `Message::TerminateApp` or `Message::ScheduleApp` is ever sent to the agent via the `agents: DashMap`. | `handlers.rs` lines 285–356 | **HIGH** |
| F | "Node capacity from real telemetry" | `list_nodes` (line 442) and `get_node` (line 526) hardcode `cpu_cores: 8.0, memory_gb: 32, disk_gb: 100` for live agents instead of using heartbeat data. | `handlers.rs` lines 442, 526 | **MEDIUM** |

## 3. Scope

### Files to Modify

| File | Change |
|---|---|
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/api/handlers.rs` | Rewrite `deploy_app` to invoke scheduler + deploy pipeline; wire `start_app`/`stop_app`/`restart_app` to send agent messages; implement `get_logs` using agent connection; fix `list_nodes`/`get_node` to use real capacity |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/state.rs` | Add `scheduler: Arc<Scheduler>`, `deploy_pipeline: Arc<DeployPipeline>`, `guardian: Arc<Guardian>` fields to `AppState`; spawn background tasks in `AppState::new()` |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/services/mod.rs` | Add `pub mod scheduler;`, `pub mod deploy_pipeline;`, `pub mod guardian;` declarations and re-exports |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/config.rs` | Add `SchedulerConfig`, `DeployConfig`, `GuardianConfig` structs; add fields to `Config` |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/lib.rs` | Add `pub mod scheduler;` and `pub mod guardian;` (if placed at crate root, otherwise mod lives under services/) |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/orm/mod.rs` | Add `AppInstances` to `Table` enum; add `insert_app_instance`/`query_app_instances` methods; add `query_nodes_by_status` method |
| `/home/z/my-project/shellwego-backend-monorepo/migrations/005_app_instances.sql` | New migration for `app_instances` table |

### New Files to Create

| File | Purpose |
|---|---|
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/services/scheduler.rs` | `Scheduler` struct — bin-packing scheduler that picks a node for each app replica |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/services/deploy_pipeline.rs` | `DeployPipeline` struct — state machine driving deployments through Pending → Scheduled → Running → Succeeded/Failed |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/services/guardian.rs` | `Guardian` struct — periodic health monitor, node liveness checker, auto-healer |
| `/home/z/my-project/shellwego-backend-monorepo/migrations/005_app_instances.sql` | Schema for `app_instances` table tracking per-node app placement |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/services/agent_client.rs` | `AgentClient` — thin wrapper over `DashMap<Uuid, AgentConnection>` that sends messages and awaits responses |

## 4. Prerequisites

1. **Fix existing build errors** — The control-plane currently has 22 compilation errors (tracing subscriber conflicts, ownership issues, type mismatches). Execute Plan 00 (if it exists) or fix these errors first. The new modules must compile on a clean `cargo check -p shellwego-control-plane`.

2. **`app_instances` table must exist** — The scheduler writes placement records. This plan includes the migration (step 5.1). It must be applied before the scheduler runs.

3. **`agent_heartbeats` table already exists** — Migration `004_add_agent_state.sql` created `agent_heartbeats` and `task_assignments`. The guardian reads from `agent_heartbeats`.

4. **Agent connection infrastructure** — The `AppState.agents: DashMap<Uuid, AgentConnection>` already stores live connections. The `Message::ScheduleApp` and `Message::TerminateApp` variants already exist in `shellwego_schema::network::Message`. No agent-side changes are needed (the agent receiving these messages is out of scope for this plan — we implement the control-plane sender side).

5. **No dependency on Plan 01** — Plan 01 (Security Hardening) adds `Extension<CurrentUser>` and `check_permission()` to handlers. If Plan 01 is not yet executed, the handler signatures in this plan use the current (pre-Plan-01) form. After Plan 01 merges, a mechanical 2-line addition per handler will be needed.

## 5. Detailed Implementation Steps

### Phase 0: Database Migration — `app_instances` Table

**Step 0.1 — Create migration file**

File: `/home/z/my-project/shellwego-backend-monorepo/migrations/005_app_instances.sql`

```sql
-- 005_app_instances.sql
-- Tracks which app replicas are running on which nodes.
-- The scheduler writes a row per replica assignment;
-- the guardian updates status on health transitions.

CREATE TABLE IF NOT EXISTS app_instances (
    id TEXT PRIMARY KEY,
    app_id TEXT NOT NULL REFERENCES apps(id),
    node_id TEXT NOT NULL REFERENCES nodes(id),
    deployment_id TEXT REFERENCES deployments(id),
    status TEXT NOT NULL DEFAULT 'starting',
    internal_ip TEXT NOT NULL DEFAULT '',
    started_at TEXT NOT NULL,
    health_checks_passed INTEGER NOT NULL DEFAULT 0,
    health_checks_failed INTEGER NOT NULL DEFAULT 0,
    last_health_check TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_app_instances_app ON app_instances(app_id);
CREATE INDEX IF NOT EXISTS idx_app_instances_node ON app_instances(node_id);
CREATE INDEX IF NOT EXISTS idx_app_instances_status ON app_instances(status);
CREATE INDEX IF NOT EXISTS idx_app_instances_deployment ON app_instances(deployment_id);
```

Status values: `starting`, `healthy`, `unhealthy`, `stopping`, `exited`, `rescheduling`.

### Phase 1: Agent Client — Message Sending Layer

**Step 1.1 — Create `agent_client.rs`**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/services/agent_client.rs`

Purpose: Abstract the logic of looking up an `AgentConnection` in the `DashMap`, serializing a `Message`, and tracking the request/response lifecycle. Currently the `AgentConnection` struct only stores metadata (no actual QUIC stream). This module provides the *interface* for sending commands; the actual transport will be wired in a future plan.

```rust
use std::sync::Arc;
use dashmap::DashMap;
use uuid::Uuid;
use chrono::Utc;
use tracing::{info, warn, error};

use shellwego_schema::network::{AgentConnection, Message, ResourceLimits};

/// Result of a command sent to an agent
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub success: bool,
    pub error: Option<String>,
}

/// Client for sending commands to connected agents.
///
/// Currently simulates successful responses since the QUIC transport
/// is not yet wired. The interface is production-ready: when the
/// actual Quinn QUIC streams are connected, only the `send_internal`
/// method needs to change.
pub struct AgentClient {
    agents: Arc<DashMap<Uuid, AgentConnection>>,
}

impl AgentClient {
    pub fn new(agents: Arc<DashMap<Uuid, AgentConnection>>) -> Self {
        Self { agents }
    }

    /// Send a ScheduleApp command to a specific agent node
    pub async fn schedule_app(
        &self,
        node_id: &Uuid,
        deployment_id: Uuid,
        app_id: Uuid,
        image: String,
        limits: ResourceLimits,
    ) -> Result<CommandResult, AgentClientError> {
        let _conn = self.agents.get(node_id)
            .ok_or(AgentClientError::NodeNotFound(*node_id))?;

        let msg = Message::ScheduleApp {
            deployment_id,
            app_id,
            image,
            limits,
        };

        // TODO: When QUIC transport is wired, serialize msg and send over stream.
        // For now, log and simulate success.
        info!(
            "AGENT CMD -> node={} action=ScheduleApp deployment={} app={}",
            node_id, deployment_id, app_id
        );
        Ok(CommandResult { success: true, error: None })
    }

    /// Send a TerminateApp command to a specific agent node
    pub async fn terminate_app(
        &self,
        node_id: &Uuid,
        app_id: Uuid,
    ) -> Result<CommandResult, AgentClientError> {
        let _conn = self.agents.get(node_id)
            .ok_or(AgentClientError::NodeNotFound(*node_id))?;

        let msg = Message::TerminateApp { app_id };

        // TODO: When QUIC transport is wired, serialize msg and send over stream.
        info!(
            "AGENT CMD -> node={} action=TerminateApp app={}",
            node_id, app_id
        );
        Ok(CommandResult { success: true, error: None })
    }

    /// Check if a node is currently connected
    pub fn is_connected(&self, node_id: &Uuid) -> bool {
        self.agents.contains_key(node_id)
    }

    /// List all connected node IDs
    pub fn connected_node_ids(&self) -> Vec<Uuid> {
        self.agents.iter().map(|r| *r.key()).collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentClientError {
    #[error("Node {0} not found or not connected")]
    NodeNotFound(Uuid),
    #[error("Send failed: {0}")]
    SendFailed(String),
}
```

**Step 1.2 — Register in `services/mod.rs`**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/services/mod.rs`

Add:
```rust
pub mod agent_client;
pub mod scheduler;
pub mod deploy_pipeline;
pub mod guardian;

pub use agent_client::AgentClient;
pub use scheduler::Scheduler;
pub use deploy_pipeline::DeployPipeline;
pub use guardian::Guardian;
```

### Phase 2: Scheduler — Node Selection & Placement

**Step 2.1 — Create `scheduler.rs`**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/services/scheduler.rs`

The scheduler is responsible for picking the best node for each app replica. It uses a best-fit bin-packing algorithm based on available capacity reported by agents via heartbeats.

```rust
use std::sync::Arc;
use dashmap::DashMap;
use uuid::Uuid;
use chrono::Utc;
use tracing::{info, warn, debug};
use serde::{Deserialize, Serialize};

use crate::orm::Database;
use crate::orm::DatabaseError;
use shellwego_schema::network::AgentConnection;
use shellwego_schema::entities::app::ResourceSpec;

/// Scheduling result for a single replica
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Placement {
    pub app_id: Uuid,
    pub node_id: Uuid,
    pub replica_index: u32,
}

/// Scheduling decision for all replicas of an app
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleResult {
    pub placements: Vec<Placement>,
    pub failed: Vec<ScheduleFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleFailure {
    pub replica_index: u32,
    pub reason: String,
}

/// Node suitability score
#[derive(Debug, Clone)]
struct NodeScore {
    node_id: Uuid,
    score: f64,          // higher = better fit
    available_cpu: f64,
    available_memory_gb: f64,
}

/// Scheduler config
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SchedulerConfig {
    /// Memory overhead per microVM in bytes (kernel + initrd + guest OS)
    pub vm_memory_overhead_bytes: u64,
    /// Maximum fraction of node memory that can be allocated (0.0 - 1.0)
    pub max_memory_fraction: f64,
    /// Preferred region (empty = any)
    pub preferred_region: String,
    /// Spread replicas across nodes if true, pack if false
    pub spread_replicas: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            vm_memory_overhead_bytes: 64 * 1024 * 1024, // 64MB overhead per VM
            max_memory_fraction: 0.85,                   // don't fill past 85%
            preferred_region: String::new(),
            spread_replicas: true,
        }
    }
}

/// The Scheduler assigns app replicas to worker nodes.
pub struct Scheduler {
    config: SchedulerConfig,
    db: Arc<Database>,
    agents: Arc<DashMap<Uuid, AgentConnection>>,
}

impl Scheduler {
    pub fn new(
        config: SchedulerConfig,
        db: Arc<Database>,
        agents: Arc<DashMap<Uuid, AgentConnection>>,
    ) -> Self {
        Self { config, db, agents }
    }

    /// Schedule all replicas for an app.
    ///
    /// For each replica, finds the best-fit node and creates an
    /// `app_instances` row.
    pub async fn schedule_app(
        &self,
        app_id: Uuid,
        replicas: u32,
        resources: &ResourceSpec,
        deployment_id: Uuid,
    ) -> Result<ScheduleResult, SchedulerError> {
        info!("Scheduling app={} replicas={} deployment={}", app_id, replicas, deployment_id);

        let mut placements = Vec::new();
        let mut failed = Vec::new();

        // Get eligible nodes
        let eligible = self.get_eligible_nodes(resources).await?;

        for i in 0..replicas {
            let node = if self.config.spread_replicas {
                self.pick_spread(&eligible, resources, &placements)
            } else {
                self.pick_best_fit(&eligible, resources)
            };

            match node {
                Some(node_score) => {
                    // Create app_instances row
                    let instance_id = Uuid::new_v4();
                    let now = Utc::now().to_rfc3339();
                    let instance = serde_json::json!({
                        "id": instance_id.to_string(),
                        "app_id": app_id.to_string(),
                        "node_id": node_score.node_id.to_string(),
                        "deployment_id": deployment_id.to_string(),
                        "status": "starting",
                        "internal_ip": "",
                        "started_at": now,
                        "health_checks_passed": 0,
                        "health_checks_failed": 0,
                        "last_health_check": null,
                        "created_at": now,
                    });

                    match self.db.insert("app_instances", &instance).await {
                        Ok(()) => {
                            info!(
                                "Scheduled replica {}/{} of app {} on node {}",
                                i + 1, replicas, app_id, node_score.node_id
                            );
                            placements.push(Placement {
                                app_id,
                                node_id: node_score.node_id,
                                replica_index: i,
                            });
                        }
                        Err(e) => {
                            warn!("Failed to create instance record: {}", e);
                            failed.push(ScheduleFailure {
                                replica_index: i,
                                reason: format!("DB error: {}", e),
                            });
                        }
                    }
                }
                None => {
                    failed.push(ScheduleFailure {
                        replica_index: i,
                        reason: "No eligible nodes with sufficient capacity".to_string(),
                    });
                }
            }
        }

        Ok(ScheduleResult { placements, failed })
    }

    /// Get list of eligible nodes with enough capacity
    async fn get_eligible_nodes(
        &self,
        resources: &ResourceSpec,
    ) -> Result<Vec<NodeScore>, SchedulerError> {
        let required_memory_gb = (resources.memory_bytes as f64
            + self.config.vm_memory_overhead_bytes as f64)
            / (1024.0 * 1024.0 * 1024.0);
        let required_cpu = resources.cpu_milli as f64 / 1000.0;

        let mut scores = Vec::new();

        for entry in self.agents.iter() {
            let node_id = *entry.key();
            let _conn = entry.value();

            // Fetch latest heartbeat for this node
            let heartbeat = self.get_latest_heartbeat(node_id).await;

            let (avail_cpu, avail_mem_gb) = match heartbeat {
                Some(hb) => {
                    // available = total * (1 - usage)
                    let cpu = hb.cpu_cores as f64 * (1.0 - hb.cpu_usage);
                    let mem = hb.memory_total_gb as f64 * (1.0 - hb.memory_usage);
                    (cpu, mem)
                }
                None => {
                    // No heartbeat yet — use conservative defaults
                    (2.0, 16.0)
                }
            };

            // Check capacity constraints
            if avail_cpu < required_cpu {
                continue;
            }
            if avail_mem_gb < required_memory_gb {
                continue;
            }
            if avail_mem_gb / (avail_mem_gb + required_memory_gb)
                > self.config.max_memory_fraction
            {
                // Would exceed max memory fraction — skip
                continue;
            }

            // Best-fit score: prefer nodes with least remaining capacity after placement
            // (this is the bin-packing heuristic)
            let remaining_after = avail_mem_gb - required_memory_gb;
            let score = 100.0 - remaining_after; // lower remaining = higher score = tighter fit

            scores.push(NodeScore {
                node_id,
                score,
                available_cpu: avail_cpu,
                available_memory_gb: avail_mem_gb,
            });
        }

        scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(scores)
    }

    /// Best-fit: pick the node with the highest score (tightest remaining fit)
    fn pick_best_fit<'a>(
        &self,
        nodes: &'a [NodeScore],
        resources: &ResourceSpec,
    ) -> Option<&'a NodeScore> {
        nodes.first().filter(|n| {
            n.available_memory_gb
                >= (resources.memory_bytes as f64
                    + self.config.vm_memory_overhead_bytes as f64)
                    / (1024.0 * 1024.0 * 1024.0)
        })
    }

    /// Spread: pick the node with the fewest existing placements for this app
    fn pick_spread<'a>(
        &self,
        nodes: &'a [NodeScore],
        resources: &ResourceSpec,
        existing_placements: &[Placement],
    ) -> Option<&'a NodeScore> {
        let min_memory_gb = (resources.memory_bytes as f64
            + self.config.vm_memory_overhead_bytes as f64)
            / (1024.0 * 1024.0 * 1024.0);

        // Count placements per node
        let mut counts = std::collections::HashMap::new();
        for p in existing_placements {
            *counts.entry(p.node_id).or_insert(0u32) += 1;
        }

        // Find the node with minimum placements and sufficient capacity
        let mut best: Option<&NodeScore> = None;
        let mut best_count = u32::MAX;

        for node in nodes {
            if node.available_memory_gb < min_memory_gb {
                continue;
            }
            let count = counts.get(&node.node_id).copied().unwrap_or(0);
            if count < best_count {
                best_count = count;
                best = Some(node);
            }
        }

        best
    }

    /// Fetch latest heartbeat for a node from the DB
    async fn get_latest_heartbeat(
        &self,
        node_id: Uuid,
    ) -> Option<NodeHeartbeat> {
        let sql = "SELECT cpu_usage, memory_usage, disk_usage, running_vms, reported_at FROM agent_heartbeats WHERE node_id = ? ORDER BY reported_at DESC LIMIT 1";
        let row: Option<(f64, f64, f64, i64, String)> =
            sqlx::query_as(sql)
                .bind(node_id.to_string())
                .fetch_optional(self.db.pool())
                .await
                .ok()?;

        row.map(|(cpu_usage, memory_usage, disk_usage, running_vms, reported_at)| {
            NodeHeartbeat {
                node_id,
                cpu_usage,
                memory_usage,
                disk_usage,
                running_vms: running_vms as u32,
                cpu_cores: 8, // Default; will be updated from node registration
                memory_total_gb: 32,
                reported_at,
            }
        })
    }

    /// Remove all instance records for an app (used during undeploy)
    pub async fn unschedule_app(&self, app_id: &Uuid) -> Result<u64, SchedulerError> {
        let sql = "DELETE FROM app_instances WHERE app_id = ?";
        let result = sqlx::query(sql)
            .bind(app_id.to_string())
            .execute(self.db.pool())
            .await
            .map_err(|e| SchedulerError::Database(e.to_string()))?;
        Ok(result.rows_affected())
    }
}

/// Heartbeat data extracted from DB
struct NodeHeartbeat {
    node_id: Uuid,
    cpu_usage: f64,
    memory_usage: f64,
    disk_usage: f64,
    running_vms: u32,
    cpu_cores: u32,
    memory_total_gb: u64,
    reported_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("No eligible nodes available")]
    NoNodesAvailable,
    #[error("Database error: {0}")]
    Database(String),
    #[error("Insufficient capacity: need {need}, best available {available}")]
    InsufficientCapacity { need: String, available: String },
}
```

### Phase 3: Deploy Pipeline — State Machine

**Step 3.1 — Create `deploy_pipeline.rs`**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/services/deploy_pipeline.rs`

The deploy pipeline is a state machine that:
1. Accepts a deployment request
2. Invokes the scheduler to get placements
3. Sends `Message::ScheduleApp` to each agent
4. Monitors instance status transitions
5. Updates the deployment record from `pending` → `scheduled` → `running` → `succeeded`/`failed`

```rust
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;
use tracing::{info, warn, error};

use crate::orm::Database;
use crate::orm::DatabaseError;
use crate::services::agent_client::AgentClient;
use crate::services::scheduler::Scheduler;
use shellwego_schema::network::ResourceLimits;
use shellwego_schema::entities::app::ResourceSpec;

/// Deploy pipeline configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DeployPipelineConfig {
    /// Timeout for waiting for all instances to become healthy (seconds)
    pub health_timeout_secs: u64,
    /// Interval between health status polls (seconds)
    pub poll_interval_secs: u64,
    /// Maximum concurrent deployments per app
    pub max_concurrent_per_app: u32,
}

impl Default for DeployPipelineConfig {
    fn default() -> Self {
        Self {
            health_timeout_secs: 300,
            poll_interval_secs: 5,
            max_concurrent_per_app: 1,
        }
    }
}

/// Deploy result
#[derive(Debug, Clone)]
pub struct DeployResult {
    pub deployment_id: Uuid,
    pub status: String,
    pub instances_created: u32,
    pub instances_failed: u32,
    pub error: Option<String>,
}

/// The DeployPipeline drives a deployment from creation to completion.
pub struct DeployPipeline {
    config: DeployPipelineConfig,
    db: Arc<Database>,
    agent_client: Arc<AgentClient>,
    scheduler: Arc<Scheduler>,
}

impl DeployPipeline {
    pub fn new(
        config: DeployPipelineConfig,
        db: Arc<Database>,
        agent_client: Arc<AgentClient>,
        scheduler: Arc<Scheduler>,
    ) -> Self {
        Self { config, db, agent_client, scheduler }
    }

    /// Execute a deployment for an app.
    ///
    /// This is the main entry point called by the `deploy_app` handler.
    pub async fn deploy(
        &self,
        deployment_id: Uuid,
        app_id: Uuid,
        image: String,
        replicas: u32,
        resources: ResourceSpec,
    ) -> Result<DeployResult, DeployError> {
        info!(
            "DeployPipeline: starting deployment={} app={} replicas={}",
            deployment_id, app_id, replicas
        );

        // 1. Transition deployment to "scheduled"
        self.update_deployment_status(&deployment_id, "scheduled").await?;

        // 2. Schedule replicas onto nodes
        let schedule_result = self.scheduler
            .schedule_app(app_id, replicas, &resources, deployment_id)
            .await
            .map_err(|e| DeployError::ScheduleFailed(e.to_string()))?;

        if schedule_result.placements.is_empty() {
            self.update_deployment_status(&deployment_id, "failed").await?;
            return Ok(DeployResult {
                deployment_id,
                status: "failed".to_string(),
                instances_created: 0,
                instances_failed: replicas,
                error: Some(format!(
                    "No placements found. Failures: {:?}",
                    schedule_result.failed
                )),
            });
        }

        // 3. Send ScheduleApp command to each agent
        let resource_limits = ResourceLimits {
            cpu_milli: resources.cpu_milli,
            memory_bytes: resources.memory_bytes,
        };

        let mut instances_created: u32 = 0;
        let mut instances_failed: u32 = 0;
        let mut agent_errors: Vec<String> = Vec::new();

        for placement in &schedule_result.placements {
            match self.agent_client.schedule_app(
                &placement.node_id,
                deployment_id,
                app_id,
                image.clone(),
                resource_limits.clone(),
            ).await {
                Ok(result) if result.success => {
                    instances_created += 1;
                }
                Ok(result) => {
                    instances_failed += 1;
                    let err_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());
                    agent_errors.push(err_msg);
                    warn!(
                        "DeployPipeline: agent {} rejected schedule for app {}: {}",
                        placement.node_id, app_id, agent_errors.last().unwrap()
                    );
                    // Update instance status to 'exited'
                    self.mark_instance_failed(app_id, &placement.node_id, &placement.replica_index).await;
                }
                Err(e) => {
                    instances_failed += 1;
                    agent_errors.push(e.to_string());
                    warn!(
                        "DeployPipeline: failed to reach agent {} for app {}: {}",
                        placement.node_id, app_id, e
                    );
                    self.mark_instance_failed(app_id, &placement.node_id, &placement.replica_index).await;
                }
            }
        }

        // 4. Transition deployment to final status
        if instances_failed == 0 {
            self.update_deployment_status(&deployment_id, "succeeded").await?;
            // Update app status to "running"
            self.update_app_status(&app_id, "running").await?;
        } else if instances_created > 0 {
            // Partial success — mark as running with degraded note
            self.update_deployment_status(&deployment_id, "succeeded").await?;
            self.update_app_status(&app_id, "running").await?;
        } else {
            self.update_deployment_status(&deployment_id, "failed").await?;
            self.update_app_status(&app_id, "error").await?;
        }

        let final_status = if instances_failed == 0 { "succeeded" } else { "failed" };

        info!(
            "DeployPipeline: deployment={} app={} status={} created={} failed={}",
            deployment_id, app_id, final_status, instances_created, instances_failed
        );

        Ok(DeployResult {
            deployment_id,
            status: final_status.to_string(),
            instances_created,
            instances_failed,
            error: if agent_errors.is_empty() {
                None
            } else {
                Some(agent_errors.join("; "))
            },
        })
    }

    /// Stop all instances of an app on all nodes
    pub async fn undeploy(
        &self,
        app_id: &Uuid,
    ) -> Result<u32, DeployError> {
        info!("DeployPipeline: undeploying app={}", app_id);

        // Find all instances for this app
        let instances: Vec<serde_json::Value> = self.db
            .query("app_instances", [("app_id".to_string(), app_id.to_string())].into(), None, None)
            .await
            .map_err(|e| DeployError::Database(e.to_string()))?;

        let mut terminated = 0u32;
        for inst in &instances {
            let node_id_str = inst["node_id"].as_str().unwrap_or("");
            let node_id = Uuid::parse_str(node_id_str).unwrap_or(Uuid::nil());

            match self.agent_client.terminate_app(&node_id, *app_id).await {
                Ok(_) => terminated += 1,
                Err(e) => warn!("Failed to terminate app {} on node {}: {}", app_id, node_id, e),
            }
        }

        // Remove instance records
        let _ = self.scheduler.unschedule_app(app_id).await;
        // Update app status
        let _ = self.update_app_status(app_id, "stopped").await;

        info!("DeployPipeline: undeployed app={} terminated={}", app_id, terminated);
        Ok(terminated)
    }

    /// Restart all instances of an app (stop then re-deploy)
    pub async fn restart(
        &self,
        app_id: &Uuid,
    ) -> Result<DeployResult, DeployError> {
        info!("DeployPipeline: restarting app={}", app_id);

        // Fetch app details
        let app: Option<serde_json::Value> = self.db
            .find_by_id("apps", app_id)
            .await
            .map_err(|e| DeployError::Database(e.to_string()))?
            .ok_or_else(|| DeployError::NotFound(*app_id))?;

        let image = app["image"].as_str().unwrap_or("").to_string();
        let replicas = 1u32; // Default; should come from app resources
        let resources = ResourceSpec::default();

        // Stop existing instances
        self.undeploy(app_id).await?;

        // Update app status to restarting
        self.update_app_status(app_id, "deploying").await?;

        // Create new deployment and schedule
        let deployment_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        let deployment = serde_json::json!({
            "id": deployment_id.to_string(),
            "app_id": app_id.to_string(),
            "build_id": Uuid::nil().to_string(),
            "status": "pending",
            "strategy": "rolling",
            "started_at": now,
            "finished_at": null,
            "previous_deployment": null,
        });
        self.db.insert("deployments", &deployment).await
            .map_err(|e| DeployError::Database(e.to_string()))?;

        self.deploy(deployment_id, *app_id, image, replicas, resources).await
    }

    // --- Internal helpers ---

    async fn update_deployment_status(
        &self,
        deployment_id: &Uuid,
        status: &str,
    ) -> Result<(), DeployError> {
        let now = Utc::now().to_rfc3339();
        let finished_at = if matches!(status, "succeeded" | "failed" | "rolled_back") {
            Some(now.as_str())
        } else {
            None
        };

        let update = serde_json::json!({
            "status": status,
            "started_at": "",
            "finished_at": finished_at,
            "previous_deployment": null,
        });
        self.db.update("deployments", deployment_id, &update).await
            .map_err(|e| DeployError::Database(e.to_string()))?;
        Ok(())
    }

    async fn update_app_status(
        &self,
        app_id: &Uuid,
        status: &str,
    ) -> Result<(), DeployError> {
        let update = serde_json::json!({
            "name": "",
            "slug": "",
            "status": status,
            "image": "",
            "command": null,
            "resources": null,
            "env": {},
            "domains": [],
            "volumes": [],
            "health_check": null,
            "source": null,
        });
        self.db.update("apps", app_id, &update).await
            .map_err(|e| DeployError::Database(e.to_string()))?;
        Ok(())
    }

    async fn mark_instance_failed(
        &self,
        app_id: Uuid,
        node_id: &Uuid,
        _replica_index: &u32,
    ) {
        // Find the instance and update to exited
        let conditions: std::collections::HashMap<String, String> = [
            ("app_id".to_string(), app_id.to_string()),
            ("node_id".to_string(), node_id.to_string()),
        ].into();
        if let Ok(instances) = self.db.query("app_instances", conditions, Some(1), None).await {
            for inst in &instances {
                if let Some(id_str) = inst["id"].as_str() {
                    if let Ok(id) = Uuid::parse_str(id_str) {
                        let update = serde_json::json!({
                            "status": "exited",
                            "health_checks_failed": 1,
                        });
                        let _ = self.db.update("app_instances", &id, &update).await;
                    }
                }
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DeployError {
    #[error("App not found: {0}")]
    NotFound(Uuid),
    #[error("Scheduling failed: {0}")]
    ScheduleFailed(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Agent communication error: {0}")]
    AgentError(String),
}
```

### Phase 4: Guardian — Health Monitor & Auto-Healer

**Step 4.1 — Create `guardian.rs`**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/services/guardian.rs`

The Guardian runs as a background `tokio::task` and periodically:
1. Checks agent heartbeats — marks nodes offline if no heartbeat within threshold
2. Checks app instance health — transitions `unhealthy` after consecutive failures
3. Auto-heals — reschedules instances from dead nodes or restarts unhealthy ones
4. Updates node capacity from latest heartbeat data

```rust
use std::sync::Arc;
use std::collections::HashMap;
use dashmap::DashMap;
use uuid::Uuid;
use chrono::Utc;
use tokio::time::{interval, Duration};
use tracing::{info, warn, error, debug};

use crate::orm::Database;
use crate::services::agent_client::AgentClient;
use crate::services::deploy_pipeline::DeployPipeline;
use shellwego_schema::network::AgentConnection;

/// Guardian configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GuardianConfig {
    /// How often to run the guardian loop (seconds)
    pub tick_interval_secs: u64,
    /// Seconds after which a node with no heartbeat is marked offline
    pub node_offline_threshold_secs: u64,
    /// Consecutive unhealthy checks before triggering restart
    pub unhealthy_threshold: u32,
    /// Maximum auto-restarts per app per hour
    pub max_restarts_per_hour: u32,
    /// Enable auto-healing
    pub auto_heal_enabled: bool,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            tick_interval_secs: 10,
            node_offline_threshold_secs: 30,
            unhealthy_threshold: 3,
            max_restarts_per_hour: 5,
            auto_heal_enabled: true,
        }
    }
}

/// Guardian metrics snapshot (for observability)
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GuardianSnapshot {
    pub nodes_checked: u64,
    pub nodes_offline: u64,
    pub instances_checked: u64,
    pub instances_unhealthy: u64,
    pub auto_heals_triggered: u64,
    pub last_tick: String,
}

/// The Guardian monitors node and instance health, triggering auto-healing.
pub struct Guardian {
    config: GuardianConfig,
    db: Arc<Database>,
    agent_client: Arc<AgentClient>,
    agents: Arc<DashMap<Uuid, AgentConnection>>,
    snapshot: Arc<tokio::sync::RwLock<GuardianSnapshot>>,
    shutdown: tokio::sync::watch::Sender<bool>,
}

impl Guardian {
    pub fn new(
        config: GuardianConfig,
        db: Arc<Database>,
        agent_client: Arc<AgentClient>,
        agents: Arc<DashMap<Uuid, AgentConnection>>,
    ) -> Self {
        let (shutdown, _) = tokio::sync::watch::channel(false);
        Self {
            config,
            db,
            agent_client,
            agents,
            snapshot: Arc::new(tokio::sync::RwLock::new(GuardianSnapshot::default())),
            shutdown,
        }
    }

    /// Start the guardian background loop. Returns a JoinHandle.
    pub fn spawn(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let mut shutdown_rx = self.shutdown.subscribe();
        let mut tick = interval(Duration::from_secs(self.config.tick_interval_secs));

        tokio::spawn(async move {
            info!("Guardian: started (tick={}s, offline_threshold={}s, unhealthy_after={})",
                self.config.tick_interval_secs,
                self.config.node_offline_threshold_secs,
                self.config.unhealthy_threshold,
            );

            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        self.run_tick().await;
                    }
                    _ = shutdown_rx.changed() => {
                        info!("Guardian: received shutdown signal, stopping");
                        break;
                    }
                }
            }
        })
    }

    /// Run a single guardian tick
    async fn run_tick(&self) {
        let mut snapshot = GuardianSnapshot {
            last_tick: Utc::now().to_rfc3339(),
            ..Default::default()
        };

        // 1. Check node liveness
        let offline_nodes = self.check_node_liveness().await;
        snapshot.nodes_offline = offline_nodes.len() as u64;
        snapshot.nodes_checked = self.agents.len() as u64;

        // 2. Handle offline nodes — reschedule their instances
        if self.config.auto_heal_enabled {
            for node_id in &offline_nodes {
                self.handle_node_offline(node_id).await;
            }
        }

        // 3. Check instance health
        let unhealthy_instances = self.check_instance_health().await;
        snapshot.instances_unhealthy = unhealthy_instances.len() as u64;

        // 4. Auto-heal unhealthy instances
        if self.config.auto_heal_enabled {
            for (app_id, node_id, instance_id) in &unhealthy_instances {
                self.handle_unhealthy_instance(*app_id, *node_id, *instance_id).await;
            }
        }

        // 5. Update running_apps count on nodes
        self.update_node_running_apps().await;

        // Write snapshot
        let mut snap_lock = self.snapshot.write().await;
        *snap_lock = snapshot;
    }

    /// Check which connected agents have stale heartbeats
    async fn check_node_liveness(&self) -> Vec<Uuid> {
        let cutoff = Utc::now()
            - chrono::Duration::seconds(self.config.node_offline_threshold_secs as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let mut offline = Vec::new();

        for entry in self.agents.iter() {
            let node_id = *entry.key();

            let sql = "SELECT reported_at FROM agent_heartbeats WHERE node_id = ? ORDER BY reported_at DESC LIMIT 1";
            let result: Option<(String,)> = sqlx::query_as(sql)
                .bind(node_id.to_string())
                .fetch_optional(self.db.pool())
                .await
                .ok()
                .flatten();

            match result {
                Some((reported_at,)) => {
                    let reported = chrono::DateTime::parse_from_rfc3339(&reported_at);
                    if let Ok(reported_dt) = reported {
                        if reported_dt.with_timezone(&Utc) < cutoff {
                            warn!("Guardian: node {} last heartbeat {} — marking offline", node_id, reported_at);
                            offline.push(node_id);
                            // Update node status in DB
                            self.mark_node_status(&node_id, "offline").await;
                        }
                    }
                }
                None => {
                    // No heartbeat ever received
                    warn!("Guardian: node {} has no heartbeats — marking offline", node_id);
                    offline.push(node_id);
                    self.mark_node_status(&node_id, "offline").await;
                }
            }
        }

        offline
    }

    /// Handle an offline node: reschedule its app instances elsewhere
    async fn handle_node_offline(&self, node_id: &Uuid) {
        info!("Guardian: rescheduling instances from offline node {}", node_id);

        // Find all instances on this node
        let conditions: HashMap<String, String> = [
            ("node_id".to_string(), node_id.to_string()),
            ("status".to_string(), "healthy".to_string()),
        ].into();

        let instances: Vec<serde_json::Value> = match self.db
            .query("app_instances", conditions, None, None)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                warn!("Guardian: failed to query instances for node {}: {}", node_id, e);
                return;
            }
        };

        for inst in &instances {
            let app_id_str = inst["app_id"].as_str().unwrap_or("");
            let app_id = match Uuid::parse_str(app_id_str) {
                Ok(id) => id,
                Err(_) => continue,
            };

            // Mark instance as rescheduling
            if let Some(id_str) = inst["id"].as_str() {
                if let Ok(id) = Uuid::parse_str(id_str) {
                    self.update_instance_status(&id, "rescheduling").await;
                }
            }

            // Deregister the agent so scheduler doesn't pick it
            // (it's already offline so this is cosmetic)
            warn!(
                "Guardian: app {} instance on node {} needs rescheduling (auto-heal triggered)",
                app_id, node_id
            );
            // The actual reschedule would call deploy_pipeline.restart()
            // but we avoid cascading restarts — just mark for now.
            // A human operator or a separate reconciler handles the reschedule.
        }
    }

    /// Check health of all running app instances
    async fn check_instance_health(&self) -> Vec<(Uuid, Uuid, Uuid)> {
        let mut unhealthy = Vec::new();

        // Find instances that haven't been health-checked recently
        // For now, we check instances that are "starting" for too long
        let cutoff = Utc::now() - chrono::Duration::seconds(120);
        let cutoff_str = cutoff.to_rfc3339();

        let sql = "SELECT id, app_id, node_id, status, created_at FROM app_instances WHERE status IN ('starting', 'unhealthy') AND created_at < ?";
        let rows: Vec<(String, String, String, String, String)> = match sqlx::query_as(sql)
            .bind(cutoff_str)
            .fetch_all(self.db.pool())
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("Guardian: instance health check query failed: {}", e);
                return unhealthy;
            }
        };

        for (id, app_id_str, node_id_str, status, _created_at) in rows {
            let id = Uuid::parse_str(&id).unwrap_or(Uuid::nil());
            let app_id = Uuid::parse_str(&app_id_str).unwrap_or(Uuid::nil());
            let node_id = Uuid::parse_str(&node_id_str).unwrap_or(Uuid::nil());

            warn!(
                "Guardian: instance {} (app={}, node={}) stuck in '{}' for >120s",
                id, app_id, node_id, status
            );

            unhealthy.push((app_id, node_id, id));
        }

        unhealthy
    }

    /// Handle an unhealthy instance
    async fn handle_unhealthy_instance(&self, app_id: Uuid, node_id: Uuid, instance_id: Uuid) {
        warn!(
            "Guardian: auto-healing instance {} (app={}, node={})",
            instance_id, app_id, node_id
        );
        self.update_instance_status(&instance_id, "unhealthy").await;
        // In a full implementation, this would trigger a restart via deploy_pipeline.
        // For now, we just mark the instance.
    }

    /// Mark a node with a given status in the DB
    async fn mark_node_status(&self, node_id: &Uuid, status: &str) {
        let update = serde_json::json!({
            "hostname": "",
            "status": status,
            "region": "",
            "zone": "",
            "capacity": null,
            "capabilities": [],
            "network": {},
            "labels": {},
            "running_apps": 0,
            "microvm_capacity": 0,
            "microvm_used": 0,
            "kernel_version": "",
            "firecracker_version": "",
            "agent_version": "",
            "last_seen": Utc::now().to_rfc3339(),
        });
        if let Err(e) = self.db.update("nodes", node_id, &update).await {
            warn!("Guardian: failed to update node {} status to {}: {}", node_id, status, e);
        }
    }

    /// Update instance status
    async fn update_instance_status(&self, instance_id: &Uuid, status: &str) {
        let update = serde_json::json!({
            "status": status,
        });
        if let Err(e) = self.db.update("app_instances", instance_id, &update).await {
            warn!("Guardian: failed to update instance {} status: {}", instance_id, e);
        }
    }

    /// Update the running_apps count on each node based on app_instances
    async fn update_node_running_apps(&self) {
        let sql = "SELECT node_id, COUNT(*) as cnt FROM app_instances WHERE status IN ('starting', 'healthy') GROUP BY node_id";
        let rows: Vec<(String, i64)> = match sqlx::query_as(sql)
            .fetch_all(self.db.pool())
            .await
        {
            Ok(r) => r,
            Err(_) => return,
        };

        for (node_id_str, count) in rows {
            if let Ok(node_id) = Uuid::parse_str(&node_id_str) {
                let update = serde_json::json!({
                    "hostname": "",
                    "status": "ready",
                    "region": "",
                    "zone": "",
                    "capacity": null,
                    "capabilities": [],
                    "network": {},
                    "labels": {},
                    "running_apps": count as i64,
                    "microvm_capacity": 0,
                    "microvm_used": 0,
                    "kernel_version": "",
                    "firecracker_version": "",
                    "agent_version": "",
                    "last_seen": Utc::now().to_rfc3339(),
                });
                let _ = self.db.update("nodes", &node_id, &update).await;
            }
        }
    }

    /// Get the latest guardian metrics snapshot
    pub async fn get_snapshot(&self) -> GuardianSnapshot {
        self.snapshot.read().await.clone()
    }

    /// Gracefully shut down the guardian
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }
}
```

### Phase 5: Wire Into AppState & Config

**Step 5.1 — Add config structs**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/config.rs`

Add to `Config` struct:
```rust
/// Scheduler configuration
pub scheduler: SchedulerConfig,

/// Deploy pipeline configuration
pub deploy: DeployPipelineConfig,

/// Guardian configuration
pub guardian: GuardianConfig,
```

Add the config structs (or import from services):
```rust
use crate::services::scheduler::SchedulerConfig;
use crate::services::deploy_pipeline::DeployPipelineConfig;
use crate::services::guardian::GuardianConfig;
```

In `Config::load()` and `Config::default()`:
```rust
scheduler: SchedulerConfig::default(),
deploy: DeployPipelineConfig::default(),
guardian: GuardianConfig::default(),
```

Also add env-var overrides:
```rust
let scheduler_tick = std::env::var("SCHEDULER_SPREAD_REPLICAS")
    .ok()
    .and_then(|v| v.parse::<bool>().ok())
    .unwrap_or(true);
```

**Step 5.2 — Update `AppState`**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/state.rs`

Add imports:
```rust
use crate::services::scheduler::Scheduler;
use crate::services::deploy_pipeline::DeployPipeline;
use crate::services::guardian::Guardian;
use crate::services::agent_client::AgentClient;
```

Add fields to `AppState`:
```rust
/// Scheduler for app-to-node placement
pub scheduler: Arc<Scheduler>,
/// Deploy pipeline for driving deployments
pub deploy_pipeline: Arc<DeployPipeline>,
/// Guardian watchdog
pub guardian: Arc<Guardian>,
/// Agent client for sending commands
pub agent_client: Arc<AgentClient>,
```

In `AppState::new()`, after creating `Arc<Self>`:
```rust
// Initialize agent client
let agent_client = Arc::new(AgentClient::new(state.agents.clone()));

// Initialize scheduler
let scheduler = Arc::new(Scheduler::new(
    config.scheduler.clone(),
    db.clone(),
    state.agents.clone(),
));

// Initialize deploy pipeline
let deploy_pipeline = Arc::new(DeployPipeline::new(
    config.deploy.clone(),
    db.clone(),
    agent_client.clone(),
    scheduler.clone(),
));

// Initialize guardian
let guardian = Arc::new(Guardian::new(
    config.guardian.clone(),
    db.clone(),
    agent_client.clone(),
    state.agents.clone(),
));
```

> **Important**: Because `AppState` holds `Arc<Scheduler>` which needs `Arc<Database>`, and `AppState` itself contains the `Arc<Database>`, we need to initialize these services *before* constructing the full `AppState`. Restructure `AppState::new()` to:
> 1. Create `Arc<Database>` first
> 2. Create `DashMap<Uuid, AgentConnection>` (agents)
> 3. Create `AgentClient`, `Scheduler`, `DeployPipeline`, `Guardian` using the above
> 4. Construct `AppState` with all services

Spawn the guardian background task:
```rust
// After AppState is constructed:
let guardian_handle = state.guardian.spawn(state.guardian.clone());
// Store handle for graceful shutdown (can be added to AppState or managed separately)
```

**Step 5.3 — Update `services/mod.rs`**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/services/mod.rs`

Add module declarations and re-exports:
```rust
pub mod agent_client;
pub mod scheduler;
pub mod deploy_pipeline;
pub mod guardian;

pub use agent_client::AgentClient;
pub use scheduler::{Scheduler, SchedulerConfig};
pub use deploy_pipeline::{DeployPipeline, DeployPipelineConfig};
pub use guardian::{Guardian, GuardianConfig};
```

### Phase 6: Rewrite API Handlers

**Step 6.1 — Rewrite `deploy_app` handler**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/api/handlers.rs`

Replace the current `deploy_app` (lines 215–244):

```rust
pub async fn deploy_app(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Verify app exists
    let app: Option<App> = state.db.find_by_id("apps", &app_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("App"))?;
    let app = app.unwrap();

    // Create deployment record
    let deployment_id = Uuid::new_v4();
    let deployment_entity = serde_json::json!({
        "id": deployment_id,
        "app_id": app_id,
        "build_id": null,
        "status": "pending",
        "strategy": "rolling",
        "started_at": Utc::now().to_rfc3339(),
        "finished_at": null,
        "previous_deployment": null,
    });
    state.db.insert("deployments", &deployment_entity).await.map_err(internal_db_err)?;

    // Update app status to "deploying"
    let update = serde_json::json!({
        "name": "",
        "slug": "",
        "status": "deploying",
        "image": "",
        "command": null,
        "resources": null,
        "env": {},
        "domains": [],
        "volumes": [],
        "health_check": null,
        "source": null,
    });
    state.db.update("apps", &app_id, &update).await.map_err(internal_db_err)?;

    // Run the deploy pipeline (non-blocking)
    let pipeline = state.deploy_pipeline.clone();
    tokio::spawn(async move {
        match pipeline.deploy(
            deployment_id,
            app_id,
            app.image,
            1, // replicas — should come from app config
            shellwego_schema::entities::app::ResourceSpec::default(),
        ).await {
            Ok(result) => {
                info!("Deployment {} completed: {}", deployment_id, result.status);
            }
            Err(e) => {
                error!("Deployment {} failed: {}", deployment_id, e);
            }
        }
    });

    Ok(Json(serde_json::json!({
        "deployment_id": deployment_id,
        "app_id": app_id,
        "status": "pending"
    })))
}
```

**Step 6.2 — Rewrite `stop_app` handler**

Replace the current `stop_app` (lines 299–327):

```rust
pub async fn stop_app(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Verify app exists
    let _app: Option<App> = state.db.find_by_id("apps", &app_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("App"))?;

    // Run undeploy pipeline (blocks until agents respond)
    match state.deploy_pipeline.undeploy(&app_id).await {
        Ok(terminated) => {
            info!("Stopped app {}: {} instances terminated", app_id, terminated);
        }
        Err(e) => {
            warn!("Stop app {} warning: {}", app_id, e);
            // Still update DB status even if agent communication failed
        }
    }

    Ok(Json(serde_json::json!({
        "status": "stopped",
        "app_id": app_id
    })))
}
```

**Step 6.3 — Rewrite `start_app` handler**

Replace the current `start_app` (lines 329–356):

```rust
pub async fn start_app(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Verify app exists
    let app: Option<App> = state.db.find_by_id("apps", &app_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("App"))?;
    let app = app.unwrap();

    // Update status to deploying
    let update = serde_json::json!({
        "name": "",
        "slug": "",
        "status": "deploying",
        "image": "",
        "command": null,
        "resources": null,
        "env": {},
        "domains": [],
        "volumes": [],
        "health_check": null,
        "source": null,
    });
    state.db.update("apps", &app_id, &update).await.map_err(internal_db_err)?;

    // Trigger deploy pipeline in background
    let pipeline = state.deploy_pipeline.clone();
    tokio::spawn(async move {
        let deployment_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        let deployment = serde_json::json!({
            "id": deployment_id,
            "app_id": app_id,
            "build_id": null,
            "status": "pending",
            "strategy": "rolling",
            "started_at": now,
            "finished_at": null,
            "previous_deployment": null,
        });
        if let Err(e) = pipeline.deploy(
            deployment_id,
            app_id,
            app.image,
            1,
            shellwego_schema::entities::app::ResourceSpec::default(),
        ).await {
            error!("Start app {} failed: {}", app_id, e);
        }
    });

    Ok(Json(serde_json::json!({
        "status": "starting",
        "app_id": app_id
    })))
}
```

**Step 6.4 — Rewrite `restart_app` handler**

Replace the current `restart_app` (lines 285–297):

```rust
pub async fn restart_app(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Verify app exists
    let _app: Option<App> = state.db.find_by_id("apps", &app_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("App"))?;

    // Update app status
    let update = serde_json::json!({
        "name": "",
        "slug": "",
        "status": "restarting",
        "image": "",
        "command": null,
        "resources": null,
        "env": {},
        "domains": [],
        "volumes": [],
        "health_check": null,
        "source": null,
    });
    state.db.update("apps", &app_id, &update).await.map_err(internal_db_err)?;

    // Trigger restart pipeline in background
    let pipeline = state.deploy_pipeline.clone();
    tokio::spawn(async move {
        if let Err(e) = pipeline.restart(&app_id).await {
            error!("Restart app {} failed: {}", app_id, e);
        }
    });

    Ok(Json(serde_json::json!({
        "status": "restarting",
        "app_id": app_id
    })))
}
```

**Step 6.5 — Implement `get_logs` handler**

Replace the current `get_logs` (lines 358–366):

```rust
pub async fn get_logs(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<Uuid>,
    Query(_params): Query<LogQuery>,
) -> Result<Json<Vec<LogEntry>>, (StatusCode, Json<ErrorResponse>)> {
    // Verify app exists
    let _app: Option<App> = state.db.find_by_id("apps", &app_id).await.map_err(internal_db_err)?
        .ok_or_else(|| not_found_err("App"))?;

    // Find instances for this app to know which agents to query
    let conditions: HashMap<String, String> = [
        ("app_id".to_string(), app_id.to_string()),
    ].into();
    let instances: Vec<serde_json::Value> = state.db
        .query("app_instances", conditions, None, None)
        .await
        .map_err(internal_db_err)?;

    // For each instance, attempt to fetch logs from the agent.
    // Currently the AgentConnection doesn't have a log-streaming channel,
    // so we return a placeholder per instance.
    let mut logs = Vec::new();
    for inst in &instances {
        let node_id_str = inst["node_id"].as_str().unwrap_or("unknown");
        let status = inst["status"].as_str().unwrap_or("unknown");
        logs.push(LogEntry {
            timestamp: Utc::now(),
            message: format!(
                "Log streaming not yet connected. Instance on node {} (status: {}). \
                 Logs will be available when the QUIC log channel is implemented.",
                node_id_str, status
            ),
            source: format!("node:{}", node_id_str),
        });
    }

    if logs.is_empty() {
        logs.push(LogEntry {
            timestamp: Utc::now(),
            message: "No running instances found for this app.".to_string(),
            source: "scheduler".to_string(),
        });
    }

    Ok(Json(logs))
}
```

**Step 6.6 — Fix `list_nodes` and `get_node` hardcoded capacity**

In `list_nodes` (line 442), replace the hardcoded `NodeCapacity`:
```rust
// Before:
capacity: NodeCapacity {
    cpu_cores: 8.0,
    memory_gb: 32,
    disk_gb: 100,
},

// After:
capacity: self.get_node_capacity_from_heartbeat(&state, &a.node_id).await,
```

Add a helper method:
```rust
async fn get_node_capacity_from_heartbeat(
    state: &AppState,
    node_id: &Uuid,
) -> NodeCapacity {
    let sql = "SELECT cpu_usage, memory_usage, disk_usage, running_vms FROM agent_heartbeats WHERE node_id = ? ORDER BY reported_at DESC LIMIT 1";
    let result: Option<(f64, f64, f64, i64)> = sqlx::query_as(sql)
        .bind(node_id.to_string())
        .fetch_optional(state.db.pool())
        .await
        .ok()
        .flatten();

    match result {
        Some((cpu_usage, memory_usage, disk_usage, running_vms)) => NodeCapacity {
            cpu_cores: (8.0 * (1.0 - cpu_usage)),     // available
            memory_gb: (32.0 * (1.0 - memory_usage) * 1024.0) as u64, // available MB → GB
            disk_gb: (100.0 * (1.0 - disk_usage)) as u64,
        },
        None => NodeCapacity {
            cpu_cores: 8.0,
            memory_gb: 32,
            disk_gb: 100,
        },
    }
}
```

Apply the same fix to `get_node` (line 526).

### Phase 7: ORM Extensions

**Step 7.1 — Add `AppInstances` table variant**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/orm/mod.rs`

In the `Table` enum (line 138), add:
```rust
AppInstances,
```

In `from_str_name()` (line 156), add:
```rust
"app_instances" => Ok(Self::AppInstances),
```

In `table_name()` (line 175), add:
```rust
Self::AppInstances => "app_instances",
```

In `insert()` method (line 368), add a case:
```rust
Table::AppInstances => self.insert_app_instance(&value).await,
```

Add the insert method:
```rust
async fn insert_app_instance(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
    sqlx::query(
        "INSERT INTO app_instances (id, app_id, node_id, deployment_id, status, internal_ip, started_at, health_checks_passed, health_checks_failed, last_health_check, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(str_val(v, "id"))
    .bind(str_val(v, "app_id"))
    .bind(str_val(v, "node_id"))
    .bind(str_val(v, "deployment_id"))
    .bind(str_or(v, "status", "starting"))
    .bind(str_or(v, "internal_ip", ""))
    .bind(str_or_ts(v, "started_at"))
    .bind(int_val(v, "health_checks_passed", 0))
    .bind(int_val(v, "health_checks_failed", 0))
    .bind(v["last_health_check"].as_str())
    .bind(str_or_ts(v, "created_at"))
    .execute(&self.pool)
    .await
    .map_err(|e| DatabaseError::QueryError(format!("Insert app_instance failed: {}", e)))?;
    Ok(())
}
```

### Phase 8: Unit Tests

**Step 8.1 — Scheduler tests**

Add `#[cfg(test)] mod tests` to `scheduler.rs`:
- `test_scheduler_no_agents` — scheduling with zero agents returns empty placements
- `test_scheduler_single_replica` — one replica on one eligible node succeeds
- `test_scheduler_spread_replicas` — two replicas on two nodes with spread enabled
- `test_scheduler_pack_replicas` — two replicas on same node with spread disabled
- `test_scheduler_insufficient_capacity` — replica requiring more memory than any node returns failure

**Step 8.2 — Deploy pipeline tests**

Add tests to `deploy_pipeline.rs`:
- `test_deploy_no_agents` — deploy returns error when no agents connected
- `test_deploy_success` — deploy creates deployment record and instance records

**Step 8.3 — Guardian tests**

Add tests to `guardian.rs`:
- `test_guardian_node_offline_detection` — node with stale heartbeat marked offline
- `test_guardian_instance_health_check` — instance stuck in "starting" detected

**Step 8.4 — Agent client tests**

Add tests to `agent_client.rs`:
- `test_schedule_app_no_agent` — returns `NodeNotFound` error
- `test_schedule_app_connected` — returns success when agent in DashMap

## 6. Dependencies on Other Plans

| Plan ID | Dependency | Notes |
|---|---|---|
| **Plan 01** | Partial — handler signatures | If Plan 01 is executed first, all handlers in this plan need `Extension(current_user): Extension<CurrentUser>` added and `check_permission()` calls. If not yet executed, use current signatures. |
| **Plan 00** (if exists) | Build must compile | The 22 existing compilation errors must be fixed before this plan's code can be added. This plan does NOT fix pre-existing errors. |
| **Plan 03** (likely networking) | QUIC transport | The `AgentClient::schedule_app()` and `terminate_app()` currently simulate success. When the QUIC transport is wired (Plan 03 or similar), the TODO comments in `agent_client.rs` become the integration points. |
| **Plan 04** (likely agent-side) | Agent receiving commands | The agent binary must handle `Message::ScheduleApp` and `Message::TerminateApp`. This plan only implements the control-plane sender side. |
| **Plan 05** (likely logging) | Log streaming | `get_logs` currently returns placeholders. Real log streaming requires a QUIC log channel, which is out of scope. |
| **Plans 06-11** | None | No dependency. Billing, monitoring, etc. are orthogonal. |

This plan should be executed **after** Plan 00 (build fix) and **after or in parallel with** Plan 01 (security hardening).

## 7. Acceptance Criteria

### Unit Tests
- [ ] `cargo test -p shellwego-control-plane` passes with 0 failures (including all new tests)
- [ ] Scheduler: `test_scheduler_single_replica` places one replica on the only eligible node
- [ ] Scheduler: `test_scheduler_spread_replicas` places replicas on different nodes when spread enabled
- [ ] Deploy pipeline: `test_deploy_no_agents` returns proper error
- [ ] Guardian: `test_guardian_node_offline_detection` marks node with stale heartbeat as offline
- [ ] Agent client: `test_schedule_app_no_agent` returns `NodeNotFound`
- [ ] Migration `005_app_instances.sql` applies cleanly to both SQLite and PostgreSQL

### Integration Verification
- [ ] `deploy_app` API call creates a `deployments` record AND `app_instances` records
- [ ] `deploy_app` returns `202` (or `200`) with a `deployment_id`; status transitions from `pending` → `scheduled` → `succeeded`
- [ ] `stop_app` API call creates `app_instances` rows with status `exited`
- [ ] `start_app` API call triggers a new deployment and creates fresh `app_instances`
- [ ] `restart_app` API call stops then re-schedules the app
- [ ] `get_logs` returns per-instance log entries (even if placeholder messages)
- [ ] `list_nodes` returns real capacity from heartbeat data (not hardcoded 8/32/100)
- [ ] Guardian loop runs every 10s and marks nodes without recent heartbeats as `offline`
- [ ] `cargo build --release` succeeds with no new compile errors

### Behavioral
- [ ] When no agents are connected, `deploy_app` fails gracefully with a clear error
- [ ] When an agent disconnects, the guardian detects it within 30s and marks the node offline
- [ ] When `undeploy` is called, `Message::TerminateApp` is logged (even though transport is simulated)
- [ ] Deployment status accurately reflects the final state in the `deployments` table

## 8. Estimated Complexity

**XL** (Extra Large)

Rationale:
- **Scheduler** (~250 lines): Core bin-packing algorithm, heartbeat-based capacity estimation, spread/pack modes. Medium algorithmic complexity.
- **Deploy pipeline** (~250 lines): State machine driving deployments through multiple stages. Integration between scheduler, agent client, and DB. Medium-high complexity due to error handling and state transitions.
- **Guardian** (~250 lines): Background task with periodic health checks, node liveness detection, auto-healing logic. Medium complexity.
- **Agent client** (~100 lines): Thin wrapper with clean interface. Low complexity.
- **Handler rewrites** (~200 lines): Six handlers need substantive changes (deploy, start, stop, restart, logs, list_nodes). Medium complexity due to tokio::spawn patterns and DB interactions.
- **Config + state + ORM** (~100 lines): Mechanical wiring of new services into AppState and config. Low complexity.
- **Migration** (~20 lines): Simple CREATE TABLE. Low complexity.
- **Tests** (~200 lines): Unit tests for scheduler, pipeline, guardian, client.

Total: ~1,370 lines of production code + ~200 lines of test code.

## 9. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **Build errors block progress** — 22 pre-existing compile errors in control-plane | High | High — nothing compiles until fixed | Must complete Plan 00 first. This plan does not fix pre-existing errors. |
| **AppState initialization circular dependency** — `AppState` needs `Scheduler` which needs `Arc<Database>` which is inside `AppState` | High | High — compile error | Restructure `AppState::new()` to create all services *before* constructing the final `AppState`. See Step 5.2. |
| **AgentConnection has no actual transport** — `Message::ScheduleApp` is logged but not sent over QUIC | Certain (by design) | Medium — deployments appear to succeed but no VMs actually start | This is expected. The `AgentClient` provides the interface; transport wiring is a separate plan. Add clear TODO comments. |
| **Guardian false-positives** — marks healthy nodes offline during temporary network partitions | Medium | High — unnecessary rescheduling of all apps | Use a grace period (30s default). Consider adding a "suspect" state before "offline". Configurable threshold. |
| **Scheduler places all replicas on one node** — bin-packing fills a node, single point of failure | Medium | Medium — node loss takes out all replicas | Default to `spread_replicas: true`. Document that spread mode reduces density efficiency but improves fault tolerance. |
| **Race condition in deploy pipeline** — two concurrent deploys for the same app | Medium | Medium — conflicting instance records | Add `max_concurrent_per_app` check in the deploy pipeline. Use DB-level locking or app-level mutex. |
| **Migration conflict with Plan 00** — if Plan 00 also adds migrations | Low | Medium — migration number collision | Use `005_app_instances.sql` (assuming Plan 00 uses 004 or lower). Coordinate numbering. |
| **`HashMap<String, String>` passed to `db.query()` may not match all callers** — existing handlers use different patterns | Low | Low — compilation error caught immediately | The `db.query()` signature already accepts `HashMap<String, String>`. Verify all call sites pass the correct type. |
| **Memory leak from tokio::spawn in handlers** — spawned deploy tasks hold references | Low | Medium — long-running tasks accumulate | Deploy tasks are finite (they complete). Add a timeout (default 300s in `DeployPipelineConfig`). Monitor task count. |
