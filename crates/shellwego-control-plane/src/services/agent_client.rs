//! Agent client for sending commands to connected agents.
//!
//! Abstracts the logic of looking up an AgentConnection in the DashMap,
//! serializing a Message, and tracking the request/response lifecycle.
//! Currently simulates successful responses since the QUIC transport
//! is not yet wired.

use std::sync::Arc;
use dashmap::DashMap;
use uuid::Uuid;
use tracing::{info, warn};

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

        let _msg = Message::ScheduleApp {
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

        let _msg = Message::TerminateApp { app_id };

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
