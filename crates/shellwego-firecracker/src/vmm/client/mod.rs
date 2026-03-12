//! Firecracker API Client
//!
//! HTTP client for communicating with Firecracker over Unix Domain Sockets.
//! Based on Firecracker API specification v1.16.0-dev.

use std::path::{Path, PathBuf};
use crate::models::*;
use anyhow::{Result, Context};
use hyper::{Request, Method, StatusCode};
use hyper_util::rt::TokioIo;
use http_body_util::{BodyExt, Full};
use bytes::Bytes;
use tokio::net::UnixStream;

/// Firecracker API client for communicating over Unix Domain Sockets.
#[derive(Clone)]
pub struct FirecrackerClient {
    socket_path: PathBuf,
}

impl FirecrackerClient {
    /// Create a new Firecracker client.
    pub fn new(socket_path: &Path) -> Self {
        Self {
            socket_path: socket_path.to_path_buf(),
        }
    }

    // ========================================================================
    // Internal HTTP Methods
    // ========================================================================

    async fn request<T: serde::Serialize>(
        &self,
        method: Method,
        uri: &str,
        body: Option<T>,
    ) -> Result<String> {
        let stream = UnixStream::connect(&self.socket_path).await
            .with_context(|| format!("Failed to connect to firecracker socket at {:?}", self.socket_path))?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

        tokio::task::spawn(async move {
            let _ = conn.await;
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
            if let Ok(err_obj) = serde_json::from_str::<serde_json::Value>(&body_str) {
                if let Some(msg) = err_obj.get("fault_message").and_then(|v| v.as_str()) {
                    anyhow::bail!("Firecracker API Error ({}): {}", status, msg);
                }
            }
            anyhow::bail!("Firecracker API Error ({}): {}", status, body_str);
        }

        Ok(body_str)
    }

    async fn get(&self, uri: &str) -> Result<String> {
        self.request::<()>(Method::GET, uri, None).await
    }

    async fn put<T: serde::Serialize>(&self, uri: &str, body: T) -> Result<()> {
        self.request(Method::PUT, uri, Some(body)).await?;
        Ok(())
    }

    async fn patch<T: serde::Serialize>(&self, uri: &str, body: T) -> Result<()> {
        self.request(Method::PATCH, uri, Some(body)).await?;
        Ok(())
    }

    // ========================================================================
    // Instance Info & Version
    // ========================================================================

    pub async fn describe_instance(&self) -> Result<InstanceInfo> {
        let body = self.get("/").await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn get_version(&self) -> Result<FirecrackerVersion> {
        let body = self.get("/version").await?;
        Ok(serde_json::from_str(&body)?)
    }

    // ========================================================================
    // Boot Source
    // ========================================================================

    pub async fn put_boot_source(&self, boot_source: BootSource) -> Result<()> {
        self.put("/boot-source", boot_source).await
    }

    // ========================================================================
    // Machine Configuration
    // ========================================================================

    pub async fn get_machine_config(&self) -> Result<MachineConfiguration> {
        let body = self.get("/machine-config").await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn put_machine_config(&self, config: MachineConfiguration) -> Result<()> {
        self.put("/machine-config", config).await
    }

    pub async fn patch_machine_config(&self, config: MachineConfiguration) -> Result<()> {
        self.patch("/machine-config", config).await
    }

    // ========================================================================
    // CPU Configuration
    // ========================================================================

    pub async fn put_cpu_config(&self, config: CpuConfig) -> Result<()> {
        self.put("/cpu-config", config).await
    }

    // ========================================================================
    // Drives (Block Devices)
    // ========================================================================

    pub async fn put_drive(&self, drive_id: &str, drive: Drive) -> Result<()> {
        self.put(&format!("/drives/{}", drive_id), drive).await
    }

    pub async fn patch_drive(&self, drive_id: &str, drive: PartialDrive) -> Result<()> {
        self.patch(&format!("/drives/{}", drive_id), drive).await
    }

    // ========================================================================
    // Persistent Memory (PMEM)
    // ========================================================================

    pub async fn put_pmem(&self, id: &str, pmem: Pmem) -> Result<()> {
        self.put(&format!("/pmem/{}", id), pmem).await
    }

    // ========================================================================
    // Network Interfaces
    // ========================================================================

    pub async fn put_network_interface(&self, iface_id: &str, net: NetworkInterface) -> Result<()> {
        self.put(&format!("/network-interfaces/{}", iface_id), net).await
    }

    pub async fn patch_network_interface(&self, iface_id: &str, net: PartialNetworkInterface) -> Result<()> {
        self.patch(&format!("/network-interfaces/{}", iface_id), net).await
    }

    // ========================================================================
    // Balloon Device
    // ========================================================================

    pub async fn describe_balloon(&self) -> Result<Balloon> {
        let body = self.get("/balloon").await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn put_balloon(&self, balloon: Balloon) -> Result<()> {
        self.put("/balloon", balloon).await
    }

    pub async fn patch_balloon(&self, update: BalloonUpdate) -> Result<()> {
        self.patch("/balloon", update).await
    }

    pub async fn describe_balloon_stats(&self) -> Result<BalloonStats> {
        let body = self.get("/balloon/statistics").await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn patch_balloon_stats_interval(&self, update: BalloonStatsUpdate) -> Result<()> {
        self.patch("/balloon/statistics", update).await
    }

    // ========================================================================
    // Vsock Device
    // ========================================================================

    pub async fn put_vsock(&self, vsock: Vsock) -> Result<()> {
        self.put("/vsock", vsock).await
    }

    // ========================================================================
    // Entropy Device
    // ========================================================================

    pub async fn put_entropy(&self, entropy: EntropyDevice) -> Result<()> {
        self.put("/entropy", entropy).await
    }

    // ========================================================================
    // Serial Device
    // ========================================================================

    pub async fn put_serial(&self, serial: SerialDevice) -> Result<()> {
        self.put("/serial", serial).await
    }

    // ========================================================================
    // Logger & Metrics
    // ========================================================================

    pub async fn put_logger(&self, logger: Logger) -> Result<()> {
        self.put("/logger", logger).await
    }

    pub async fn put_metrics(&self, metrics: Metrics) -> Result<()> {
        self.put("/metrics", metrics).await
    }

    // ========================================================================
    // Actions
    // ========================================================================

    pub async fn put_actions(&self, action: InstanceActionInfo) -> Result<()> {
        self.put("/actions", action).await
    }

    pub async fn start_instance(&self) -> Result<()> {
        self.put_actions(InstanceActionInfo {
            action_type: ActionType::InstanceStart,
        }).await
    }

    pub async fn flush_metrics(&self) -> Result<()> {
        self.put_actions(InstanceActionInfo {
            action_type: ActionType::FlushMetrics,
        }).await
    }

    pub async fn send_ctrl_alt_del(&self) -> Result<()> {
        self.put_actions(InstanceActionInfo {
            action_type: ActionType::SendCtrlAltDel,
        }).await
    }

    // ========================================================================
    // VM State
    // ========================================================================

    pub async fn patch_vm(&self, vm: Vm) -> Result<()> {
        self.patch("/vm", vm).await
    }

    pub async fn pause_vm(&self) -> Result<()> {
        self.patch_vm(Vm { state: VmState::Paused }).await
    }

    pub async fn resume_vm(&self) -> Result<()> {
        self.patch_vm(Vm { state: VmState::Resumed }).await
    }

    pub async fn get_vm_config(&self) -> Result<FullVmConfiguration> {
        let body = self.get("/vm/config").await?;
        Ok(serde_json::from_str(&body)?)
    }

    // ========================================================================
    // Snapshot
    // ========================================================================

    pub async fn create_snapshot(&self, params: SnapshotCreateParams) -> Result<()> {
        self.put("/snapshot/create", params).await
    }

    pub async fn load_snapshot(&self, params: SnapshotLoadParams) -> Result<()> {
        self.put("/snapshot/load", params).await
    }

    // ========================================================================
    // Memory Hotplug
    // ========================================================================

    pub async fn put_memory_hotplug(&self, config: MemoryHotplugConfig) -> Result<()> {
        self.put("/hotplug/memory", config).await
    }

    pub async fn patch_memory_hotplug(&self, update: MemoryHotplugSizeUpdate) -> Result<()> {
        self.patch("/hotplug/memory", update).await
    }

    pub async fn get_memory_hotplug(&self) -> Result<MemoryHotplugStatus> {
        let body = self.get("/hotplug/memory").await?;
        Ok(serde_json::from_str(&body)?)
    }

    // ========================================================================
    // MMDS
    // ========================================================================

    pub async fn put_mmds(&self, contents: MmdsContentsObject) -> Result<()> {
        self.put("/mmds", contents).await
    }

    pub async fn patch_mmds(&self, contents: MmdsContentsObject) -> Result<()> {
        self.patch("/mmds", contents).await
    }

    pub async fn get_mmds(&self) -> Result<MmdsContentsObject> {
        let body = self.get("/mmds").await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn put_mmds_config(&self, config: MmdsConfig) -> Result<()> {
        self.put("/mmds/config", config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = FirecrackerClient::new(Path::new("/tmp/test.sock"));
        assert_eq!(client.socket_path, PathBuf::from("/tmp/test.sock"));
    }

    #[test]
    fn test_action_type_serialization() {
        let action = InstanceActionInfo {
            action_type: ActionType::InstanceStart,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, r#"{"action_type":"InstanceStart"}"#);
    }

    #[test]
    fn test_vm_state_serialization() {
        let vm = Vm { state: VmState::Paused };
        let json = serde_json::to_string(&vm).unwrap();
        assert_eq!(json, r#"{"state":"Paused"}"#);
    }

    #[test]
    fn test_machine_config_default() {
        let config = MachineConfiguration::default();
        assert_eq!(config.vcpu_count, 1);
        assert_eq!(config.mem_size_mib, 128);
    }

    #[test]
    fn test_drive_serialization() {
        let drive = Drive {
            drive_id: "rootfs".to_string(),
            is_root_device: true,
            is_read_only: Some(false),
            path_on_host: Some("/path/to/disk.img".to_string()),
            partuuid: None,
            cache_type: Some(CacheType::Unsafe),
            io_engine: Some(IoEngine::Sync),
            rate_limiter: None,
            socket: None,
        };
        let json = serde_json::to_string(&drive).unwrap();
        assert!(json.contains("rootfs"));
    }

    #[test]
    fn test_snapshot_type_serialization() {
        let params = SnapshotCreateParams {
            mem_file_path: "/mem".to_string(),
            snapshot_path: "/snap".to_string(),
            snapshot_type: Some(SnapshotType::Diff),
        };
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("Diff"));
    }
}
