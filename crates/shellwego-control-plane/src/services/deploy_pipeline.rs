//! Deploy pipeline — Deployment state machine.
//!
//! The deploy pipeline is a state machine that:
//! 1. Accepts a deployment request
//! 2. Invokes the scheduler to get placements
//! 3. Sends ScheduleApp to each agent
//! 4. Monitors instance status transitions
//! 5. Updates the deployment record from pending → scheduled → running → succeeded/failed

use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;
use tracing::{info, warn, error};

use crate::orm::Database;
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
    #[allow(dead_code)]
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
            .query("app_instances", HashMap::from([("app_id".to_string(), app_id.to_string())]), None, None)
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
        let conditions: HashMap<String, String> = HashMap::from([
            ("app_id".to_string(), app_id.to_string()),
            ("node_id".to_string(), node_id.to_string()),
        ]);
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
