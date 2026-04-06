//! Guardian — Health monitor & auto-healer.
//!
//! The Guardian runs as a background tokio::task and periodically:
//! 1. Checks agent heartbeats — marks nodes offline if no heartbeat within threshold
//! 2. Checks app instance health — transitions unhealthy after consecutive failures
//! 3. Auto-heals — reschedules instances from dead nodes or restarts unhealthy ones
//! 4. Updates node capacity from latest heartbeat data

use std::sync::Arc;
use std::collections::HashMap;
use dashmap::DashMap;
use uuid::Uuid;
use chrono::Utc;
use tokio::time::{interval, Duration};
use tracing::{info, warn, debug};

use crate::orm::Database;
use crate::services::agent_client::AgentClient;
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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

    /// Check which connected agents have stale heartbeats.
    /// Uses sqlx::query with manual column extraction for sqlx::Any compatibility.
    async fn check_node_liveness(&self) -> Vec<Uuid> {
        let cutoff = Utc::now()
            - chrono::Duration::seconds(self.config.node_offline_threshold_secs as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let mut offline = Vec::new();

        for entry in self.agents.iter() {
            let node_id = *entry.key();

            let sql = "SELECT reported_at FROM agent_heartbeats WHERE node_id = ? ORDER BY reported_at DESC LIMIT 1";

            let rows: Vec<sqlx::any::AnyRow> = sqlx::query(sql)
                .bind(node_id.to_string())
                .fetch_all(self.db.pool())
                .await
                .unwrap_or_default();

            match rows.first() {
                Some(row) => {
                    let reported_at: String = row.try_get("reported_at").unwrap_or_default();
                    let reported = chrono::DateTime::parse_from_rfc3339(&reported_at);
                    if let Ok(reported_dt) = reported {
                        if reported_dt.with_timezone(&Utc) < cutoff {
                            warn!("Guardian: node {} last heartbeat {} — marking offline", node_id, reported_at);
                            offline.push(node_id);
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
        let conditions: HashMap<String, String> = HashMap::from([
            ("node_id".to_string(), node_id.to_string()),
            ("status".to_string(), "healthy".to_string()),
        ]);

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

            // The actual reschedule would call deploy_pipeline.restart()
            // but we avoid cascading restarts — just mark for now.
            // A human operator or a separate reconciler handles the reschedule.
            warn!(
                "Guardian: app {} instance on node {} needs rescheduling (auto-heal triggered)",
                app_id, node_id
            );
        }
    }

    /// Check health of all running app instances.
    /// Uses sqlx::query with manual column extraction for sqlx::Any compatibility.
    async fn check_instance_health(&self) -> Vec<(Uuid, Uuid, Uuid)> {
        let mut unhealthy = Vec::new();

        // Find instances that haven't been health-checked recently
        // For now, we check instances that are "starting" for too long
        let cutoff = Utc::now() - chrono::Duration::seconds(120);
        let cutoff_str = cutoff.to_rfc3339();

        let sql = "SELECT id, app_id, node_id, status, created_at FROM app_instances WHERE status IN ('starting', 'unhealthy') AND created_at < ?";

        let rows: Vec<sqlx::any::AnyRow> = match sqlx::query(sql)
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

        for row in &rows {
            let id_str: String = row.try_get("id").unwrap_or_default();
            let app_id_str: String = row.try_get("app_id").unwrap_or_default();
            let node_id_str: String = row.try_get("node_id").unwrap_or_default();
            let status: String = row.try_get("status").unwrap_or_default();

            let id = Uuid::parse_str(&id_str).unwrap_or(Uuid::nil());
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

    /// Update the running_apps count on each node based on app_instances.
    /// Uses sqlx::query with manual column extraction for sqlx::Any compatibility.
    async fn update_node_running_apps(&self) {
        let sql = "SELECT node_id, COUNT(*) as cnt FROM app_instances WHERE status IN ('starting', 'healthy') GROUP BY node_id";

        let rows: Vec<sqlx::any::AnyRow> = match sqlx::query(sql)
            .fetch_all(self.db.pool())
            .await
        {
            Ok(r) => r,
            Err(_) => return,
        };

        for row in &rows {
            let node_id_str: String = row.try_get("node_id").unwrap_or_default();
            let count: i64 = row.try_get("cnt").unwrap_or(0);

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
