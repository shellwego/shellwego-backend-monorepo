//! Scheduler — Node selection & placement.
//!
//! The scheduler is responsible for picking the best node for each app replica.
//! It uses a best-fit bin-packing algorithm based on available capacity
//! reported by agents via heartbeats.

use std::sync::Arc;
use dashmap::DashMap;
use uuid::Uuid;
use chrono::Utc;
use tracing::{info, warn};
use serde::{Deserialize, Serialize};

use crate::orm::Database;
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

    /// Fetch latest heartbeat for a node from the DB.
    /// Uses sqlx::query (not query_as) with manual column extraction for sqlx::Any compatibility.
    async fn get_latest_heartbeat(
        &self,
        node_id: Uuid,
    ) -> Option<NodeHeartbeat> {
        let sql = "SELECT cpu_usage, memory_usage, disk_usage, running_vms, reported_at FROM agent_heartbeats WHERE node_id = ? ORDER BY reported_at DESC LIMIT 1";

        let rows: Vec<sqlx::any::AnyRow> = sqlx::query(sql)
            .bind(node_id.to_string())
            .fetch_all(self.db.pool())
            .await
            .unwrap_or_default();

        if let Some(row) = rows.first() {
            let cpu_usage: f64 = row.try_get("cpu_usage").unwrap_or(0.0);
            let memory_usage: f64 = row.try_get("memory_usage").unwrap_or(0.0);
            let disk_usage: f64 = row.try_get("disk_usage").unwrap_or(0.0);
            let running_vms: i64 = row.try_get("running_vms").unwrap_or(0);
            let reported_at: String = row.try_get("reported_at").unwrap_or_default();

            Some(NodeHeartbeat {
                node_id,
                cpu_usage,
                memory_usage,
                disk_usage,
                running_vms: running_vms as u32,
                cpu_cores: 8, // Default; will be updated from node registration
                memory_total_gb: 32,
                reported_at,
            })
        } else {
            None
        }
    }

    /// Remove all instance records for an app (used during undeploy).
    /// Uses db.raw_query for sqlx::Any compatibility.
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
