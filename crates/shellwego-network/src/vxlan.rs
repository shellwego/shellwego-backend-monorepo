//! VXLAN overlay networking for multi-node clusters

use crate::NetworkError;

/// VXLAN network manager
pub struct VxlanNetwork {
    pub vni: u32,
    pub local_ip: String,
    pub peers: Vec<String>,
}

impl VxlanNetwork {
    /// Create VXLAN interface
    pub async fn create(vni: u32, local_ip: &str) -> Result<Self, NetworkError> {
        // TODO: Create vxlan interface via netlink
        Ok(Self {
            vni,
            local_ip: local_ip.to_string(),
            peers: Vec::new(),
        })
    }

    /// Add remote peer
    pub async fn add_peer(&mut self, remote_ip: &str) -> Result<(), NetworkError> {
        self.peers.push(remote_ip.to_string());
        Ok(())
    }

    /// Remove peer
    pub async fn remove_peer(&mut self, remote_ip: &str) -> Result<(), NetworkError> {
        self.peers.retain(|p| p != remote_ip);
        Ok(())
    }

    /// Attach to bridge
    pub async fn attach_to_bridge(&self, _bridge_name: &str) -> Result<(), NetworkError> {
        // TODO: Add vxlan interface to bridge
        Ok(())
    }

    /// Set MTU for VXLAN overhead
    pub async fn set_mtu(&self, _mtu: u32) -> Result<(), NetworkError> {
        // TODO: Set interface MTU (typically 50 less than physical)
        Ok(())
    }
}

/// VXLAN tunnel endpoint information
#[derive(Debug, Clone)]
pub struct VtepInfo {
    pub vni: u32,
    pub local_ip: String,
    pub remote_ips: Vec<String>,
    pub port: u16,
}
