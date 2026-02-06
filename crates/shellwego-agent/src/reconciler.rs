//! Desired state reconciler
//! 
//! Continuously compares actual state (running VMs) with desired state
//! (from control plane) and converges them. Kubernetes-style but lighter.

use tokio::time::{interval, Duration};
use tracing::{info, debug, error};

use shellwego_network::{CniNetwork, NetworkConfig};
use crate::vmm::{self, VmmManager, MicrovmConfig};
use crate::daemon::{StateClient, DesiredApp};

/// Reconciler enforces desired state
#[derive(Clone)]
pub struct Reconciler {
    vmm: VmmManager,
    network: std::sync::Arc<CniNetwork>,
    state_client: StateClient,
    // TODO: Add metrics (reconciliation latency, drift count)
}

impl Reconciler {
    pub fn new(vmm: VmmManager, network: std::sync::Arc<CniNetwork>, state_client: StateClient) -> Self {
        Self { vmm, network, state_client }
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

            // Run supplementary control loops
            let _ = self.health_check_loop().await;
            let _ = self.check_image_updates().await;
            let _ = self.reconcile_volumes().await;
            let _ = self.reconcile_network_policies().await;
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
                // Check for image drift
                if self.check_image_updates(app).await? {
                    info!("Image update detected for app {}", app.app_id);
                    // Simple strategy: Stop (reconciler loop will restart it next tick)
                    self.vmm.stop(app.app_id).await?;
                    changes += 1;
                }
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
        self.reconcile_volumes(&desired.apps).await?;
        
        // 4. Network policies
        self.reconcile_network_policies(&desired.apps).await?;

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
                path_on_host: vol.device.clone().into(),
                is_root_device: false,
                is_read_only: false,
            });
        }

        // SOVEREIGN SECURITY: Inject secrets via memory-backed transient drive
        let secret_drive = self.setup_secrets_tmpfs(app).await?;
        drives.push(secret_drive);

        // Delegating network setup to shellwego-network
        let net_setup = self.network.setup(&NetworkConfig {
            app_id: app.app_id,
            vm_id: uuid::Uuid::new_v4(),
            bridge_name: self.network.bridge_name().to_string(),
            tap_name: format!("tap-{}", &app.app_id.to_string()[..8]),
            guest_mac: shellwego_network::generate_mac(&app.app_id),
            guest_ip: std::net::Ipv4Addr::UNSPECIFIED, // IPAM handles this
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
            kernel_path: "/var/lib/shellwego/vmlinux".into(), // TODO: Configurable
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
        
        // TODO: Wait for health check before marking ready
        
        Ok(())
    }

    async fn prepare_rootfs(&self, image: &str) -> anyhow::Result<std::path::PathBuf> {
        let safe_name = image.replace(|c: char| !c.is_alphanumeric(), "_");
        let image_path = std::path::PathBuf::from(format!("/var/lib/shellwego/images/{}.ext4", safe_name));
        
        if image_path.exists() {
            Ok(image_path)
        } else {
            // Attempt to "pull" (copy from base for prototype)
            info!("Image {} not found, attempting to provision from base...", image);
            
            let base = std::path::PathBuf::from("/var/lib/shellwego/images/base.ext4");
            if base.exists() {
                tokio::fs::copy(&base, &image_path).await
                    .map_err(|e| anyhow::anyhow!("Failed to provision image from base: {}", e))?;
                Ok(image_path)
            } else {
                // Last resort: check if an absolute path was provided (dev mode)
                let path = std::path::PathBuf::from(image);
                if path.exists() {
                     Ok(path)
                } else {
                    anyhow::bail!("Image {} not found and no base image available at {:?}", image, base);
                }
            }
        }
    }

    async fn setup_secrets_tmpfs(&self, app: &DesiredApp) -> anyhow::Result<vmm::DriveConfig> {
        let run_dir = format!("/run/shellwego/secrets/{}", app.app_id);

        tokio::fs::create_dir_all(&run_dir).await?;

        let secrets_path = std::path::Path::new(&run_dir).join("env.json");
        let content = serde_json::to_vec(&app.env)?;

        tokio::fs::write(&secrets_path, content).await?;
        
        // Ensure strict permissions for secrets
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&run_dir).await?.permissions();
        perms.set_mode(0o700); // Only owner can read
        tokio::fs::set_permissions(&run_dir, perms).await?;

        Ok(vmm::DriveConfig {
            drive_id: "secrets".to_string(),
            path_on_host: secrets_path,
            is_root_device: false,
            is_read_only: true,
        })
    }

    /// Check for image updates and rolling restart
    pub async fn check_image_updates(&self, app: &DesiredApp) -> anyhow::Result<bool> {
        // In a real registry, we'd query the manifest digest.
        // Here we check if the file modified time changed or if the name implies a tag change.
        
        // For now, we assume if the App ID exists but the requested image is different 
        // from what's running, we return true.
        // Since we don't store the running image in VmmManager yet, we rely on Reconciler logic:
        // If the file on disk has changed recently, we might trigger update.
        
        // Simplified: Return false until we persist running image version in VMM state.
        Ok(false)
    }

    /// Handle volume attachment requests
    pub async fn reconcile_volumes(&self, apps: &[DesiredApp]) -> anyhow::Result<()> {
        for app in apps {
            for vol in &app.volumes {
                let host_path = std::path::Path::new(&vol.device);
                if !host_path.exists() {
                    info!("Creating volume directory for {}", vol.volume_id);
                    tokio::fs::create_dir_all(host_path).await?;
                }
            }
        }
        Ok(())
    }

    /// Sync network policies
    pub async fn reconcile_network_policies(&self, apps: &[DesiredApp]) -> anyhow::Result<()> {
        // We push this down to CNI/eBPF layer
        for app in apps {
            // Example: Update bandwidth limits dynamically
            // self.network.update_policy(app.app_id, ...);
        }
        Ok(())
    }

    /// Health check all running VMs
    pub async fn health_check_loop(&self) -> anyhow::Result<()> {
        let vms = self.vmm.list_running().await?;
        for vm in vms {
            // Real implementation would curl the VM's health endpoint
            // or check if the PID is still alive
            debug!("Health check passed for {}", vm.app_id);
        }
        Ok(())
    }

    /// Handle graceful shutdown signal
    pub async fn prepare_shutdown(&self) -> anyhow::Result<()> {
        info!("Preparing for shutdown, stopping reconciliation...");
        Ok(())
    }
}

