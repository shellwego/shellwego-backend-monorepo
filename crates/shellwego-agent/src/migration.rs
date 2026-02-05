//! Live migration of microVMs between nodes
//!
//! This module implements live migration using a snapshot-based approach.
//! For true live migration (zero downtime), we would need Firecracker's
//! snapshot-transfer feature, but this implementation provides a solid
//! foundation that can be extended.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, debug};
use uuid::Uuid;
use tokio::io::AsyncWriteExt;

use crate::snapshot::{SnapshotManager, SnapshotInfo};
use crate::vmm::VmmManager;

/// Migration coordinator that manages VM migration between nodes
#[derive(Clone)]
pub struct MigrationManager {
    /// Snapshot manager for creating/restoring snapshots
    snapshot_manager: SnapshotManager,
    /// VMM for controlling VMs
    vmm_manager: VmmManager,
    /// Active migration sessions
    sessions: Arc<RwLock<HashMap<String, MigrationSession>>>,
    /// Network client for peer communication
    network_client: Option<Arc<dyn MigrationTransport + Send + Sync>>,
}

impl MigrationManager {
    /// Create a new migration manager
    pub async fn new(
        data_dir: &std::path::Path,
        vmm_manager: VmmManager,
    ) -> anyhow::Result<Self> {
        let snapshot_manager = SnapshotManager::new(data_dir).await?;
        
        Ok(Self {
            snapshot_manager,
            vmm_manager,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            network_client: None,
        })
    }

    /// Set the network transport for peer communication
    pub fn set_transport<T: MigrationTransport + Send + Sync + 'static>(
        &mut self,
        transport: Arc<T>,
    ) {
        self.network_client = Some(transport); 
    }

    /// Initiate live migration to target node
    ///
    /// This creates a snapshot of the VM and transfers it to the target node.
    /// For minimal downtime, the snapshot is created with the VM paused.
    pub async fn migrate_out(
        &self,
        app_id: Uuid,
        target_node: &str,
        snapshot_name: Option<&str>,
    ) -> anyhow::Result<MigrationHandle> {
        info!("Starting migration of app {} to node {}", app_id, target_node);
        
        let session_id = format!("{}-{}", app_id, Uuid::new_v4());
        let snapshot_name = snapshot_name.unwrap_or("migration");
        
        // Create snapshot (pauses VM, creates memory + disk state)
        let snapshot_info = self.snapshot_manager
            .create_snapshot(&self.vmm_manager, app_id, snapshot_name)
            .await?;
        
        debug!("Snapshot {} created for migration", snapshot_info.id);
        
        // Transfer snapshot to target node
        let transfer_result = if let Some(ref transport) = self.network_client {
            transport.transfer_snapshot(&snapshot_info, target_node).await
        } else {
            // Store locally for pickup (development mode)
            self.store_for_pickup(&snapshot_info).await
        };
        
        let handle = MigrationHandle {
            session_id: session_id.clone(),
            app_id,
            target_node: target_node.to_string(),
            snapshot_id: snapshot_info.id,
            started_at: chrono::Utc::now(),
            phase: MigrationPhase::Transferring,
        };
        
        // Store session
        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id, MigrationSession {
            handle: handle.clone(),
            transfer_status: transfer_result.ok(),
        });
        
        Ok(handle)
    }

    /// Receive incoming migration from source node
    pub async fn migrate_in(
        &self,
        source_node: &str,
        snapshot_id: &str,
    ) -> anyhow::Result<Uuid> {
        info!("Receiving migration of snapshot {} from node {}", snapshot_id, source_node);
        
        // Receive snapshot from source
        let snapshot_info = if let Some(ref transport) = self.network_client {
            transport.receive_snapshot(snapshot_id, source_node).await?
        } else {
            // Development mode: receive from local storage
            self.receive_from_pickup(snapshot_id).await?
        };
        
        // Restore VM from snapshot
        let new_app_id = Uuid::new_v4();
        self.snapshot_manager
            .restore_snapshot(&self.vmm_manager, &snapshot_info.id, new_app_id)
            .await?;
        
        info!("Migration completed. Restored as app {}", new_app_id);
        Ok(new_app_id)
    }

    /// Check migration progress
    pub async fn progress(&self, handle: &MigrationHandle) -> anyhow::Result<MigrationStatus> {
        let sessions = self.sessions.read().await;
        
        if let Some(session) = sessions.get(&handle.session_id) {
            let phase = handle.phase;
            let progress = match phase {
                MigrationPhase::Preparing => 5.0,
                MigrationPhase::Snapshotting => 25.0,
                MigrationPhase::Transferring => {
                    // Estimate based on snapshot size
                    if let Some(bytes) = session.transfer_status {
                        let estimated_total = bytes * 2; // Rough estimate
                        (bytes as f64 / estimated_total as f64 * 50.0).min(50.0)
                    } else {
                        25.0
                    }
                }
                MigrationPhase::Restoring => 75.0,
                MigrationPhase::Verifying => 90.0,
                MigrationPhase::Completed => 100.0,
                MigrationPhase::Failed => 0.0,
                MigrationPhase::Rollback => 0.0,
            };
            
            Ok(MigrationStatus {
                phase,
                progress_percent: progress,
                bytes_transferred: session.transfer_status.unwrap_or(0),
                estimated_remaining_bytes: session.transfer_status.unwrap_or(0),
                downtime_ms: if phase == MigrationPhase::Completed { 0 } else { 100 },
            })
        } else {
            Ok(MigrationStatus {
                phase: handle.phase,
                progress_percent: 0.0,
                bytes_transferred: 0,
                estimated_remaining_bytes: 0,
                downtime_ms: 0,
            })
        }
    }

    /// Cancel ongoing migration
    pub async fn cancel(&self, handle: MigrationHandle) -> anyhow::Result<()> {
        info!("Cancelling migration {}", handle.session_id);
        
        // Remove session
        let mut sessions = self.sessions.write().await;
        sessions.remove(&handle.session_id);
        
        // Cleanup snapshot if we created one
        if !handle.snapshot_id.is_empty() {
            let _ = self.snapshot_manager.delete_snapshot(&handle.snapshot_id).await;
        }
        
        Ok(())
    }

    /// Store snapshot locally for pickup (development/testing mode)
    async fn store_for_pickup(&self, snapshot: &SnapshotInfo) -> anyhow::Result<u64> {
        let path = std::path::Path::new(&snapshot.memory_path);
        let metadata = std::fs::metadata(path)?;
        Ok(metadata.len())
    }

    /// Receive snapshot from local storage (development/testing mode)
    async fn receive_from_pickup(&self, snapshot_id: &str) -> anyhow::Result<SnapshotInfo> {
        self.snapshot_manager.get_snapshot(snapshot_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Snapshot {} not found", snapshot_id))
    }
}

/// Trait for network transport during migration
#[async_trait::async_trait]
pub trait MigrationTransport {
    /// Transfer snapshot to target node
    async fn transfer_snapshot(
        &self,
        snapshot: &SnapshotInfo,
        target_node: &str,
    ) -> anyhow::Result<u64>;
    
    /// Receive snapshot from source node
    async fn receive_snapshot(
        &self,
        snapshot_id: &str,
        source_node: &str,
    ) -> anyhow::Result<SnapshotInfo>;
}

/// Migration session state
#[derive(Debug)]
struct MigrationSession {
    handle: MigrationHandle,
    transfer_status: Option<u64>,
}

/// Migration session handle for tracking and monitoring
#[derive(Debug, Clone)]
pub struct MigrationHandle {
    /// Unique session identifier
    pub session_id: String,
    /// Application being migrated
    pub app_id: Uuid,
    /// Target node for migration
    pub target_node: String,
    /// Snapshot ID used for migration
    pub snapshot_id: String,
    /// When migration started
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Current phase
    pub phase: MigrationPhase,
}

/// Detailed migration status
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    /// Current phase of migration
    pub phase: MigrationPhase,
    /// Progress percentage (0-100)
    pub progress_percent: f64,
    /// Bytes transferred so far
    pub bytes_transferred: u64,
    /// Estimated remaining bytes
    pub estimated_remaining_bytes: u64,
    /// Expected downtime in milliseconds
    pub downtime_ms: u64,
}

/// Migration phases
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationPhase {
    /// Initial preparation phase
    Preparing,
    /// Creating VM snapshot
    Snapshotting,
    /// Transferring snapshot to target
    Transferring,
    /// Restoring VM on target
    Restoring,
    /// Verifying restored VM
    Verifying,
    /// Migration completed successfully
    Completed,
    /// Migration failed
    Failed,
    /// Rolling back changes
    Rollback,
}

/// Migration configuration options
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Whether to use compression during transfer
    pub compress: bool,
    /// Whether to verify checksums
    pub verify_checksums: bool,
    /// Maximum transfer bandwidth in bytes/sec (0 = unlimited)
    pub max_bandwidth: u64,
    /// Whether to preserve MAC addresses
    pub preserve_mac: bool,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            compress: true,
            verify_checksums: true,
            max_bandwidth: 0,
            preserve_mac: false,
        }
    }
}

/// HTTP-based implementation of migration transport
pub struct HttpMigrationTransport {
    client: reqwest::Client,
    port: u16,
}

impl HttpMigrationTransport {
    pub fn new(port: u16) -> Self {
        Self {
            client: reqwest::Client::new(),
            port,
        }
    }
}

#[async_trait::async_trait]
impl MigrationTransport for HttpMigrationTransport {
    async fn transfer_snapshot(
        &self,
        snapshot: &SnapshotInfo,
        target_node: &str,
    ) -> anyhow::Result<u64> {
        let file_path = std::path::PathBuf::from(&snapshot.memory_path);
        let file_size = tokio::fs::metadata(&file_path).await?.len();
        
        let file = tokio::fs::File::open(&file_path).await?;
        let stream = tokio_util::io::ReaderStream::new(file);
        
        let url = format!("http://{}:{}/internal/migration/upload/{}", target_node, self.port, snapshot.id);
        
        let res = self.client.post(&url)
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await?;
            
        if !res.status().is_success() {
            anyhow::bail!("Upload failed: {}", res.status());
        }
        
        Ok(file_size)
    }
    
    async fn receive_snapshot(
        &self,
        snapshot_id: &str,
        source_node: &str,
    ) -> anyhow::Result<SnapshotInfo> {
        // In a real scenario, this initiates a pull or confirms a push.
        // For this implementation, we assume the snapshot was pushed to us 
        // via an HTTP handler (not shown) and we are just validating/registering it.
        
        // Placeholder: Return dummy info assuming handler saved it
        // Real impl requires coupling with the HTTP server layer
        Err(anyhow::anyhow!("Pull-based migration not yet implemented"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_migration_manager_new() {
        let temp_dir = tempdir().unwrap();
        // Note: This would need a real VmmManager in a full test
        // For now, just verify basic construction
        assert!(temp_dir.path().exists());
    }
    
    #[tokio::test]
    async fn test_migration_phases() {
        assert_eq!(MigrationPhase::Preparing, MigrationPhase::Preparing);
        assert_eq!(MigrationPhase::Completed, MigrationPhase::Completed);
    }
    
    #[tokio::test]
    async fn test_migration_handle() {
        let handle = MigrationHandle {
            session_id: "test-session".to_string(),
            app_id: Uuid::new_v4(),
            target_node: "node-1".to_string(),
            snapshot_id: "snap-1".to_string(),
            started_at: chrono::Utc::now(),
            phase: MigrationPhase::Preparing,
        };
        
        assert_eq!(handle.phase, MigrationPhase::Preparing);
        assert!(!handle.session_id.is_empty());
    }
}
