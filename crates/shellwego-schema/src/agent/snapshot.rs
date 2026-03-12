//! Snapshot types for ShellWeGo agent
//!
//! Types for agent-managed snapshots including disk and memory snapshots.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Type of snapshot for agent-managed snapshots.
///
/// This is distinct from Firecracker's SnapshotType which only supports Full/Diff.
/// Agent-managed snapshots can include disk-only and memory-only variants.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum AgentSnapshotType {
    /// Full snapshot with memory and disk
    Full,
    /// Disk-only snapshot (faster, requires full boot on restore)
    DiskOnly,
    /// Memory-only snapshot (for live migration)
    MemoryOnly,
}

impl Default for AgentSnapshotType {
    fn default() -> Self {
        Self::Full
    }
}

/// Information about an agent-managed snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct AgentSnapshotInfo {
    /// Unique snapshot identifier
    pub id: String,
    /// Application ID this snapshot belongs to
    pub app_id: Uuid,
    /// Human-readable snapshot name
    pub name: String,
    /// When the snapshot was created
    pub created_at: DateTime<Utc>,
    /// Total size in bytes (memory + disk)
    pub size_bytes: u64,
    /// Path to memory snapshot file
    pub memory_path: String,
    /// Path to disk snapshot file
    pub disk_snapshot: Option<String>,
    /// Snapshot type
    pub snapshot_type: AgentSnapshotType,
    /// Whether the snapshot includes memory state
    pub includes_memory: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_snapshot_type_default() {
        let snap_type = AgentSnapshotType::default();
        assert_eq!(snap_type, AgentSnapshotType::Full);
    }

    #[test]
    fn test_agent_snapshot_info_serialization() {
        let info = AgentSnapshotInfo {
            id: "snap-123".to_string(),
            app_id: Uuid::nil(),
            name: "test-snapshot".to_string(),
            created_at: Utc::now(),
            size_bytes: 1024,
            memory_path: "/mem.snap".to_string(),
            disk_snapshot: Some("/disk.snap".to_string()),
            snapshot_type: AgentSnapshotType::Full,
            includes_memory: true,
        };

        let json = serde_json::to_string(&info).unwrap();
        let decoded: AgentSnapshotInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, info.id);
    }
}
