//! Firecracker HTTP API client
//! 
//! Communicates with Firecracker process over Unix socket.
//! Implements the firecracker-go-sdk equivalent in Rust.

use std::path::PathBuf;
use hyper::{Body, Client, Method, Request};
use hyperlocal::{UnixClientExt, UnixConnector, Uri as UnixUri};
use serde::{Deserialize, Serialize};

/// Firecracker API driver for a specific VM socket
pub struct FirecrackerDriver {
    binary: PathBuf,
    socket_path: Option<PathBuf>,
}

/// Firecracker API request/response types
#[derive(Serialize)]
struct BootSource {
    kernel_image_path: String,
    boot_args: String,
}

#[derive(Serialize)]
struct MachineConfig {
    vcpu_count: i64,
    mem_size_mib: i64,
    // TODO: Add smt, cpu_template, track_dirty_pages for migration
}

#[derive(Serialize)]
struct Drive {
    drive_id: String,
    path_on_host: String,
    is_root_device: bool,
    is_read_only: bool,
}

#[derive(Serialize)]
struct NetworkInterfaceBody {
    iface_id: String,
    host_dev_name: String,
    guest_mac: String,
}

#[derive(Serialize)]
struct Action {
    action_type: String,
}

#[derive(Deserialize)]
pub struct InstanceInfo {
    pub state: String,
}

impl FirecrackerDriver {
    pub async fn new(binary: &PathBuf) -> anyhow::Result<Self> {
        // Verify binary exists
        if !binary.exists() {
            anyhow::bail!("Firecracker binary not found: {}", binary.display());
        }
        
        Ok(Self {
            binary: binary.clone(),
            socket_path: None,
        })
    }

    pub fn binary_path(&self) -> &PathBuf {
        &self.binary
    }

    /// Create driver instance bound to specific VM socket
    pub fn for_socket(&self, socket: &PathBuf) -> Self {
        Self {
            binary: self.binary.clone(),
            socket_path: Some(socket.clone()),
        }
    }

    /// Configure a fresh microVM
    pub async fn configure_vm(&self, config: &super::MicrovmConfig) -> anyhow::Result<()> {
        let client = Client::unix();
        let socket = self.socket_path.as_ref().unwrap();
        
        // 1. Configure boot source
        self.put(
            &client,
            socket,
            "/boot-source",
            &BootSource {
                kernel_image_path: config.kernel_path.to_string_lossy().to_string(),
                boot_args: config.kernel_boot_args.clone(),
            },
        ).await?;
        
        // 2. Configure machine
        self.put(
            &client,
            socket,
            "/machine-config",
            &MachineConfig {
                vcpu_count: config.cpu_shares as i64, // TODO: Convert properly
                mem_size_mib: config.memory_mb as i64,
            },
        ).await?;
        
        // 3. Add drives
        for drive in &config.drives {
            self.put(
                &client,
                socket,
                &format!("/drives/{}", drive.drive_id),
                &Drive {
                    drive_id: drive.drive_id.clone(),
                    path_on_host: drive.path_on_host.to_string_lossy().to_string(),
                    is_root_device: drive.is_root_device,
                    is_read_only: drive.is_read_only,
                },
            ).await?;
        }
        
        // 4. Add network interfaces
        for net in &config.network_interfaces {
            self.put(
                &client,
                socket,
                &format!("/network-interfaces/{}", net.iface_id),
                &NetworkInterfaceBody {
                    iface_id: net.iface_id.clone(),
                    host_dev_name: net.host_dev_name.clone(),
                    guest_mac: net.guest_mac.clone(),
                },
            ).await?;
        }
        
        // TODO: Configure vsock for agent communication
        
        Ok(())
    }

    /// Start the microVM
    pub async fn start_vm(&self) -> anyhow::Result<()> {
        let client = Client::unix();
        let socket = self.socket_path.as_ref().unwrap();
        
        self.put(
            &client,
            socket,
            "/actions",
            &Action {
                action_type: "InstanceStart".to_string(),
            },
        ).await?;
        
        Ok(())
    }

    /// Graceful shutdown via ACPI
    pub async fn stop_vm(&self) -> anyhow::Result<()> {
        let client = Client::unix();
        let socket = self.socket_path.as_ref().unwrap();
        
        self.put(
            &client,
            socket,
            "/actions",
            &Action {
                action_type: "SendCtrlAltDel".to_string(),
            },
        ).await?;
        
        Ok(())
    }

    /// Force shutdown (SIGKILL to firecracker process)
    pub async fn force_shutdown(&self) -> anyhow::Result<()> {
        // The VmmManager handles process termination directly
        // This is a placeholder for API-based force stop if needed
        Ok(())
    }

    /// Get instance info
    pub async fn describe_instance(&self) -> anyhow::Result<InstanceInfo> {
        let client = Client::unix();
        let socket = self.socket_path.as_ref().unwrap();
        
        let response = self.get(&client, socket, "/").await?;
        let info: InstanceInfo = serde_json::from_slice(&response)?;
        
        Ok(info)
    }

    /// Create snapshot
    pub async fn create_snapshot(
        &self,
        mem_path: &str,
        snapshot_path: &str,
    ) -> anyhow::Result<()> {
        let client = Client::unix();
        let socket = self.socket_path.as_ref().unwrap();
        
        #[derive(Serialize)]
        struct SnapshotConfig {
            snapshot_type: String,
            snapshot_path: String,
            mem_file_path: String,
        }
        
        self.put(
            &client,
            socket,
            "/snapshot/create",
            &SnapshotConfig {
                snapshot_type: "Full".to_string(),
                snapshot_path: snapshot_path.to_string(),
                mem_file_path: mem_path.to_string(),
            },
        ).await?;
        
        Ok(())
    }

    // === HTTP helpers ===

    async fn put<T: Serialize>(
        &self,
        client: &Client<UnixConnector>,
        socket: &PathBuf,
        path: &str,
        body: &T,
    ) -> anyhow::Result<()> {
        let uri = UnixUri::new(socket, path);
        
        let request = Request::builder()
            .method(Method::PUT)
            .uri(uri)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(Body::from(serde_json::to_vec(body)?))?;
            
        let response = client.request(request).await?;
        
        if !response.status().is_success() {
            let body = hyper::body::to_bytes(response.into_body()).await?;
            anyhow::bail!("Firecracker API error: {}", String::from_utf8_lossy(&body));
        }
        
        Ok(())
    }

    async fn get(
        &self,
        client: &Client<UnixConnector>,
        socket: &PathBuf,
        path: &str,
    ) -> anyhow::Result<bytes::Bytes> {
        let uri = UnixUri::new(socket, path);
        
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("Accept", "application/json")
            .body(Body::empty())?;
            
        let response = client.request(request).await?;
        let body = hyper::body::to_bytes(response.into_body()).await?;
        
        Ok(body)
    }
}