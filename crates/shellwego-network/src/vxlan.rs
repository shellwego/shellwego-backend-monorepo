//! VXLAN overlay networking for multi-node clusters

use crate::NetworkError;

/// VXLAN network manager
pub struct VxlanNetwork {
    // TODO: Add vni, local_ip, peers
}

impl VxlanNetwork {
    /// Create VXLAN interface
    pub async fn create(vni: u32, local_ip: &str) -> Result<Self, NetworkError> {
        // TODO: Create vxlan interface via netlink
        // TODO: Set VNI and local tunnel endpoint
        unimplemented!("VxlanNetwork::create")
    }

    /// Add remote peer
    pub async fn add_peer(&self, remote_ip: &str) -> Result<(), NetworkError> {
        // TODO: Add FDB entry for multicast or unicast
        unimplemented!("VxlanNetwork::add_peer")
    }

    /// Remove peer
    pub async fn remove_peer(&self, remote_ip: &str) -> Result<(), NetworkError> {
        // TODO: Remove FDB entry
        unimplemented!("VxlanNetwork::remove_peer")
    }

    /// Attach to bridge
    pub async fn attach_to_bridge(&self, bridge_name: &str) -> Result<(), NetworkError> {
        // TODO: Add vxlan interface to bridge
        unimplemented!("VxlanNetwork::attach_to_bridge")
    }

    /// Set MTU for VXLAN overhead
    pub async fn set_mtu(&self, mtu: u32) -> Result<(), NetworkError> {
        // TODO: Set interface MTU (typically 50 less than physical)
        unimplemented!("VxlanNetwork::set_mtu")
    }
}

/// VXLAN tunnel endpoint information
#[derive(Debug, Clone)]
pub struct VtepInfo {
    // TODO: Add vni, local_ip, remote_ips, port
}