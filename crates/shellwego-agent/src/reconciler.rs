//! Desired state reconciler
//! 
//! Continuously compares actual state (running VMs) with desired state
//! (from control plane) and converges them. Kubernetes-style but lighter.

use std::sync::Arc;
use tokio::time::{interval, Duration, sleep};
use tracing::{info, debug, warn, error};

use crate::vmm::{VmmManager, MicrovmConfig, MicrovmState};
use crate::daemon::{StateClient, DesiredState, DesiredApp};

/// Reconciler enforces desired state
#[derive(Clone)]
pub struct Reconciler {
    vmm: VmmManager,
    state_client: StateClient,
    // TODO: Add metrics (reconciliation latency, drift count)
}

impl Reconciler {
    pub fn new(vmm: VmmManager, state_client: StateClient) -> Self {
        Self { vmm, state_client }
    }

    /// Main reconciliation loop
    pub async fn run(&self) -> anyhow::Result<()> {
        let mut ticker = interval(Duration::from_secs(10));
        
        loop {
            ticker.tick().await;
            
            match self.reconcile().await {
                Ok(changes) => {
                    if changes > 0 {
                        debug!("Reconciliation complete: {} changes applied", changes);
                    }
                }
                Err(e) => {
                    error!("Reconciliation failed: {}", e);
                    // Continue looping, don't crash
                }
            }
        }
    }

    /// Single reconciliation pass
    async fn reconcile(&self) -> anyhow::Result<usize> {
        // Fetch desired state from control plane
        let desired = self.state_client.get_desired_state().await?;
        
        // Get actual state from VMM
        let actual = self.vmm.list_running().await?;
        
        let mut changes = 0;
        
        // 1. Create missing apps
        for app in &desired.apps {
            if !actual.iter().any(|vm| vm.app_id == app.app_id) {
                info!("Creating microVM for app {}", app.app_id);
                self.create_microvm(app).await?;
                changes += 1;
            } else {
                // Check for updates (image change, resource change)
                // TODO: Implement rolling update logic
            }
        }
        
        // 2. Remove extraneous apps
        for vm in &actual {
            if !desired.apps.iter().any(|a| a.app_id == vm.app_id) {
                info!("Removing microVM for app {}", vm.app_id);
                self.vmm.stop(vm.app_id).await?;
                changes += 1;
            }
        }
        
        // 3. Reconcile volumes
        // TODO: Attach/detach volumes as needed
        // TODO: Create missing ZFS datasets
        
        Ok(changes)
    }

    async fn create_microvm(&self, app: &DesiredApp) -> anyhow::Result<()> {
        // Prepare volume mounts
        let mut drives = vec![];
        
        // Root drive (container image as ext4)
        let rootfs_path = self.prepare_rootfs(&app.image).await?;
        drives.push(vmm::DriveConfig {
            drive_id: "rootfs".to_string(),
            path_on_host: rootfs_path,
            is_root_device: true,
            is_read_only: true, // Overlay writes to tmpfs or volume
        });
        
        // Add volume mounts
        for vol in &app.volumes {
            drives.push(vmm::DriveConfig {
                drive_id: format!("vol-{}", vol.volume_id),
                path_on_host: vol.device.clone(),
                is_root_device: false,
                is_read_only: false,
            });
        }
        
        // Network setup
        let network = self.setup_networking(app.app_id).await?;
        
        let config = MicrovmConfig {
            app_id: app.app_id,
            vm_id: uuid::Uuid::new_v4(),
            memory_mb: app.memory_mb,
            cpu_shares: app.cpu_shares,
            kernel_path: "/var/lib/shellwego/vmlinux".into(), // TODO: Configurable
            kernel_boot_args: format!(
                "console=ttyS0 reboot=k panic=1 pci=off \
                 ip={}::{}:255.255.255.0::eth0:off",
                network.guest_ip, network.host_ip
            ),
            drives,
            network_interfaces: vec![network],
            vsock_path: format!("/var/run/shellwego/{}.sock", app.app_id),
        };
        
        self.vmm.start(config).await?;
        
        // TODO: Wait for health check before marking ready
        
        Ok(())
    }

    async fn prepare_rootfs(&self, image: &str) -> anyhow::Result<std::path::PathBuf> {
        // TODO: Pull container image if not cached
        // TODO: Convert to ext4 rootfs via buildah or custom tool
        // TODO: Cache layer via ZFS snapshot
        
        Ok(std::path::PathBuf::from("/var/lib/shellwego/rootfs/base.ext4"))
    }

    async fn setup_networking(&self, app_id: uuid::Uuid) -> anyhow::Result<vmm::NetworkInterface> {
        // TODO: Allocate IP from node CIDR
        // TODO: Create TAP device
        // TODO: Setup bridge and iptables/eBPF rules
        // TODO: Configure port forwarding if public
        
        Ok(vmm::NetworkInterface {
            iface_id: "eth0".to_string(),
            host_dev_name: format!("tap-{}", app_id.to_string().split('-').next().unwrap()),
            guest_mac: generate_mac(app_id),
            guest_ip: "10.0.4.2".to_string(), // TODO: Allocate properly
            host_ip: "10.0.4.1".to_string(),
        })
    }

    /// Check for image updates and rolling restart
    pub async fn check_image_updates(&self) -> anyhow::Result<()> {
        // TODO: Poll registry for new digests
        // TODO: Compare with running VMs
        // TODO: Trigger rolling update if changed
        unimplemented!("check_image_updates")
    }

    /// Handle volume attachment requests
    pub async fn reconcile_volumes(&self) -> anyhow::Result<()> {
        // TODO: List desired volumes from state
        // TODO: Check current attachments
        // TODO: Attach/detach as needed via ZFS
        unimplemented!("reconcile_volumes")
    }

    /// Sync network policies
    pub async fn reconcile_network_policies(&self) -> anyhow::Result<()> {
        // TODO: Fetch policies from control plane
        // TODO: Apply eBPF rules via Cilium
        unimplemented!("reconcile_network_policies")
    }

    /// Health check all running VMs
    pub async fn health_check_loop(&self) -> anyhow::Result<()> {
        // TODO: Periodic health checks
        // TODO: Restart failed VMs
        // TODO: Report status to control plane
        unimplemented!("health_check_loop")
    }

    /// Handle graceful shutdown signal
    pub async fn prepare_shutdown(&self) -> anyhow::Result<()> {
        // TODO: Stop accepting new work
        // TODO: Wait for running VMs or migrate
        // TODO: Flush state
        unimplemented!("prepare_shutdown")
    }
}

fn generate_mac(app_id: uuid::Uuid) -> String {
    // Generate deterministic MAC from app_id
    let bytes = app_id.as_bytes();
    format!("02:00:00:{:02x}:{:02x}:{:02x}", bytes[0], bytes[1], bytes[2])
}
