use std::path::Path;
use crate::models::*;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct FirecrackerClient {
    socket_path: std::path::PathBuf,
}

impl FirecrackerClient {
    pub fn new(socket_path: &Path) -> Self {
        Self {
            socket_path: socket_path.to_path_buf(),
        }
    }

    pub async fn put_guest_boot_source(&self, boot_source: BootSource) -> Result<()> {
        // Mock implementation
        Ok(())
    }

    pub async fn put_machine_configuration(&self, machine_config: MachineConfig) -> Result<()> {
        Ok(())
    }

    pub async fn put_drive(&self, drive_id: &str, drive: Drive) -> Result<()> {
        Ok(())
    }

    pub async fn put_network_interface(&self, iface_id: &str, net: NetworkInterface) -> Result<()> {
        Ok(())
    }

    pub async fn put_actions(&self, action: ActionInfo) -> Result<()> {
        Ok(())
    }

    pub async fn get_vm_info(&self) -> Result<InstanceInfo> {
        Ok(InstanceInfo::default())
    }

    pub async fn put_snapshot_create(&self, params: SnapshotCreateParams) -> Result<()> {
        Ok(())
    }

    pub async fn put_snapshot_load(&self, params: SnapshotLoadParams) -> Result<()> {
        Ok(())
    }

    pub async fn patch_vm_state(&self, vm: Vm) -> Result<()> {
        Ok(())
    }

    pub async fn put_metrics(&self, metrics: Metrics) -> Result<()> {
        Ok(())
    }
}
