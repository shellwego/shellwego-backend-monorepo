use std::path::{Path, PathBuf};
use crate::models::*;
use anyhow::{Result, Context};
use hyper::{Request, Method, StatusCode};
use hyper_util::rt::TokioIo;
use http_body_util::{BodyExt, Full};
use bytes::Bytes;
use tokio::net::UnixStream;

#[derive(Clone)]
pub struct FirecrackerClient {
    socket_path: PathBuf,
}

impl FirecrackerClient {
    pub fn new(socket_path: &Path) -> Self {
        Self {
            socket_path: socket_path.to_path_buf(),
        }
    }

    /// Internal helper to perform HTTP requests over UDS
    async fn request<T: serde::Serialize>(&self, method: Method, uri: &str, body: Option<T>) -> Result<String> {
        let stream = UnixStream::connect(&self.socket_path).await
            .with_context(|| format!("Failed to connect to firecracker socket at {:?}", self.socket_path))?;
        
        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
        
        tokio::task::spawn(async move {
            if let Err(_err) = conn.await {
                // Connection errors are common when closing sockets, ignore
            }
        });

        let req_body = if let Some(b) = body {
            let json = serde_json::to_string(&b)?;
            Full::new(Bytes::from(json))
        } else {
            Full::new(Bytes::new())
        };

        let req = Request::builder()
            .method(method)
            .uri(format!("http://localhost{}", uri))
            .header("Host", "localhost")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(req_body)?;

        let res = sender.send_request(req).await?;
        let status = res.status();
        let body_bytes = res.collect().await?.to_bytes();
        let body_str = String::from_utf8(body_bytes.to_vec())?;

        if !status.is_success() && status != StatusCode::NO_CONTENT {
             // Try to parse error message from Firecracker
             if let Ok(err_obj) = serde_json::from_str::<serde_json::Value>(&body_str) {
                 if let Some(msg) = err_obj.get("fault_message").and_then(|v| v.as_str()) {
                     anyhow::bail!("Firecracker API Error ({}): {}", status, msg);
                 }
             }
             anyhow::bail!("Firecracker API Error ({}): {}", status, body_str);
        }

        Ok(body_str)
    }

    pub async fn put_guest_boot_source(&self, boot_source: BootSource) -> Result<()> {
        self.request(Method::PUT, "/boot-source", Some(boot_source)).await?;
        Ok(())
    }

    pub async fn put_machine_configuration(&self, machine_config: MachineConfig) -> Result<()> {
        self.request(Method::PUT, "/machine-config", Some(machine_config)).await?;
        Ok(())
    }

    pub async fn put_drive(&self, drive_id: &str, drive: Drive) -> Result<()> {
        self.request(Method::PUT, &format!("/drives/{}", drive_id), Some(drive)).await?;
        Ok(())
    }

    pub async fn put_network_interface(&self, iface_id: &str, net: NetworkInterface) -> Result<()> {
        self.request(Method::PUT, &format!("/network-interfaces/{}", iface_id), Some(net)).await?;
        Ok(())
    }

    pub async fn put_actions(&self, action: ActionInfo) -> Result<()> {
        self.request(Method::PUT, "/actions", Some(action)).await?;
        Ok(())
    }

    pub async fn get_vm_info(&self) -> Result<InstanceInfo> {
        let body = self.request::<()>(Method::GET, "/", None).await?;
        let info = serde_json::from_str(&body)?;
        Ok(info)
    }

    pub async fn put_snapshot_create(&self, params: SnapshotCreateParams) -> Result<()> {
        self.request(Method::PUT, "/snapshot/create", Some(params)).await?;
        Ok(())
    }

    pub async fn put_snapshot_load(&self, params: SnapshotLoadParams) -> Result<()> {
        self.request(Method::PUT, "/snapshot/load", Some(params)).await?;
        Ok(())
    }

    pub async fn patch_vm_state(&self, vm: Vm) -> Result<()> {
        self.request(Method::PATCH, "/vm", Some(vm)).await?;
        Ok(())
    }

    pub async fn put_metrics(&self, metrics: Metrics) -> Result<()> {
        self.request(Method::PUT, "/metrics", Some(metrics)).await?;
        Ok(())
    }
}