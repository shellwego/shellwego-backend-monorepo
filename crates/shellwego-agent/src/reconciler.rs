//! Desired state reconciler
//! 
//! Continuously compares actual state (running VMs) with desired state
//! (from control plane) and converges them. Kubernetes-style but lighter.

use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{info, debug, error};

use shellwego_network::{CniNetwork, NetworkConfig};
use shellwego_storage::zfs::ZfsManager;
use crate::vmm::{VmmManager, MicrovmConfig};
use crate::daemon::{StateClient, DesiredApp};

/// Reconciler enforces desired state
#[derive(Clone)]
pub struct Reconciler {
    vmm: VmmManager,
    network: Arc<CniNetwork>,
    storage: Arc<ZfsManager>,
    state_client: StateClient,
}

impl Reconciler {
    pub fn new(
        vmm: VmmManager,
        network: Arc<CniNetwork>,
        storage: Arc<ZfsManager>,
        state_client: StateClient
    ) -> Self {
        Self { vmm, network, storage, state_client }
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
                }
            }
        }
    }

    /// Single reconciliation pass
    async fn reconcile(&self) -> anyhow::Result<usize> {
        let desired = self.state_client.get_desired_state().await?;
        let actual = self.vmm.list_running().await?;
        
        let mut changes = 0;
        
        for app in &desired.apps {
            if !actual.iter().any(|vm| vm.app_id == app.app_id) {
                info!("Creating microVM for app {}", app.app_id);
                if let Err(e) = self.create_microvm(app).await {
                    error!("Failed to create VM for app {}: {}", app.app_id, e);
                } else {
                    changes += 1;
                }
            }
        }
        
        for vm in &actual {
            if !desired.apps.iter().any(|a| a.app_id == vm.app_id) {
                info!("Removing microVM for app {}", vm.app_id);
                let _ = self.vmm.stop(vm.app_id).await;
                changes += 1;
            }
        }
        
        Ok(changes)
    }

    async fn create_microvm(&self, app: &DesiredApp) -> anyhow::Result<()> {
        let mut drives = vec![];
        
        // Root drive (container image as ext4)
        let rootfs_path = self.storage.prepare_rootfs(app.app_id, &app.image).await?;
        drives.push(crate::vmm::DriveConfig {
            drive_id: "rootfs".to_string(),
            path_on_host: rootfs_path,
            is_root_device: true,
            is_read_only: true,
        });
        
        // Add volume mounts
        for vol in &app.volumes {
            drives.push(crate::vmm::DriveConfig {
                drive_id: format!("vol-{}", vol.volume_id),
                path_on_host: vol.device.clone().into(),
                is_root_device: false,
                is_read_only: false,
            });
        }

        let secret_drive = self.setup_secrets_tmpfs(app).await?;
        drives.push(secret_drive);

        let net_setup = self.network.setup(&NetworkConfig {
            app_id: app.app_id,
            vm_id: uuid::Uuid::new_v4(),
            bridge_name: self.network.bridge_name().to_string(),
            tap_name: format!("tap-{}", &app.app_id.to_string()[..8]),
            guest_mac: shellwego_network::generate_mac(&app.app_id),
            guest_ip: std::net::Ipv4Addr::UNSPECIFIED,
            host_ip: std::net::Ipv4Addr::UNSPECIFIED,
            subnet: "10.0.0.0/16".parse().unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            mtu: 1500,
            bandwidth_limit_mbps: Some(100),
        }).await?;
        
        let config = MicrovmConfig {
            app_id: app.app_id,
            vm_id: uuid::Uuid::new_v4(),
            memory_mb: app.memory_mb,
            cpu_shares: app.cpu_shares,
            kernel_path: "/var/lib/shellwego/vmlinux".into(),
            kernel_boot_args: format!(
                "console=ttyS0 reboot=k panic=1 pci=off \
                 ip={}::{}:255.255.255.0::eth0:off",
                net_setup.guest_ip, net_setup.host_ip
            ),
            drives,
            network_interfaces: vec![crate::vmm::NetworkInterface {
                iface_id: "eth0".into(),
                host_dev_name: net_setup.tap_device,
                guest_mac: shellwego_network::generate_mac(&app.app_id),
                guest_ip: net_setup.guest_ip.to_string(),
                host_ip: net_setup.host_ip.to_string(),
            }],
            vsock_path: format!("/var/run/shellwego/{}.sock", app.app_id),
        };
        
        self.vmm.start(config).await?;
        Ok(())
    }

    async fn setup_secrets_tmpfs(&self, app: &DesiredApp) -> anyhow::Result<crate::vmm::DriveConfig> {
        let run_dir = format!("/run/shellwego/secrets/{}", app.app_id);
        tokio::fs::create_dir_all(&run_dir).await?;
        let secrets_path = std::path::Path::new(&run_dir).join("env.json");
        let content = serde_json::to_vec(&app.env)?;
        tokio::fs::write(&secrets_path, content).await?;

        Ok(crate::vmm::DriveConfig {
            drive_id: "secrets".to_string(),
            path_on_host: secrets_path,
            is_root_device: false,
            is_read_only: true,
        })
    }
}
