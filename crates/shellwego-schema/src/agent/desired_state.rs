//! Desired state types for ShellWeGo agent
//!
//! Types for representing the desired state of applications and volumes
//! that the agent should reconcile to.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Desired state of applications and volumes for the agent to reconcile.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct DesiredState {
    /// Applications that should be running
    pub apps: Vec<DesiredApp>,
    /// Volumes that should be available
    pub volumes: Vec<DesiredVolume>,
}

/// Desired application configuration for scheduling.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct DesiredApp {
    /// Application ID
    pub app_id: Uuid,
    /// Container image to run
    pub image: String,
    /// Command to execute (overrides image entrypoint)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    /// Memory allocation in MB
    pub memory_mb: u64,
    /// CPU shares (1024 = 1 vCPU)
    pub cpu_shares: u64,
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Volume mounts
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
}

/// Volume mount configuration for an application.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct VolumeMount {
    /// Volume ID to mount
    pub volume_id: Uuid,
    /// Path inside the container where the volume is mounted
    pub mount_path: String,
    /// Device name on the host
    pub device: String,
}

/// Desired volume configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct DesiredVolume {
    /// Volume ID
    pub volume_id: Uuid,
    /// ZFS dataset path
    pub dataset: String,
    /// Optional snapshot to restore from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desired_state_default() {
        let state = DesiredState::default();
        assert!(state.apps.is_empty());
        assert!(state.volumes.is_empty());
    }

    #[test]
    fn test_desired_app_serialization() {
        let app = DesiredApp {
            app_id: Uuid::nil(),
            image: "nginx:latest".to_string(),
            command: Some(vec!["nginx".to_string(), "-g".to_string(), "daemon off;".to_string()]),
            memory_mb: 256,
            cpu_shares: 1024,
            env: HashMap::new(),
            volumes: vec![],
        };

        let json = serde_json::to_string(&app).unwrap();
        let decoded: DesiredApp = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.app_id, app.app_id);
        assert_eq!(decoded.image, app.image);
        assert_eq!(decoded.memory_mb, app.memory_mb);
    }

    #[test]
    fn test_volume_mount() {
        let mount = VolumeMount {
            volume_id: Uuid::new_v4(),
            mount_path: "/data".to_string(),
            device: "/dev/vdb".to_string(),
        };

        let json = serde_json::to_string(&mount).unwrap();
        let decoded: VolumeMount = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.mount_path, mount.mount_path);
    }

    #[test]
    fn test_desired_volume() {
        let volume = DesiredVolume {
            volume_id: Uuid::new_v4(),
            dataset: "pool/apps/myapp".to_string(),
            snapshot: Some("pool/apps/myapp@backup".to_string()),
        };

        let json = serde_json::to_string(&volume).unwrap();
        let decoded: DesiredVolume = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.dataset, volume.dataset);
        assert!(decoded.snapshot.is_some());
    }
}
