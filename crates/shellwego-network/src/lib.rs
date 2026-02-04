//! Network management for ShellWeGo
//! 
//! Sets up CNI-style networking for Firecracker microVMs:
//! - Bridge creation and management
//! - TAP device allocation
//! - IPAM (IP address management)
//! - eBPF-based filtering and QoS (future)

use std::net::Ipv4Addr;
use thiserror::Error;

pub mod cni;
pub mod bridge;
pub mod tap;
pub mod ipam;
pub mod quinn;

pub use cni::CniNetwork;
pub use bridge::Bridge;
pub use tap::TapDevice;
pub use ipam::Ipam;
pub use quinn::{QuinnClient, QuinnServer, Message, QuicConfig};

/// Network configuration for a microVM
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub app_id: uuid::Uuid,
    pub vm_id: uuid::Uuid,
    pub bridge_name: String,
    pub tap_name: String,
    pub guest_mac: String,
    pub guest_ip: Ipv4Addr,
    pub host_ip: Ipv4Addr,
    pub subnet: ipnetwork::Ipv4Network,
    pub gateway: Ipv4Addr,
    pub mtu: u16,
    pub bandwidth_limit_mbps: Option<u32>,
}

/// Network setup result
#[derive(Debug, Clone)]
pub struct NetworkSetup {
    pub tap_device: String,
    pub guest_ip: Ipv4Addr,
    pub host_ip: Ipv4Addr,
    pub veth_pair: Option<(String, String)>, // If using veth instead of tap
}

/// Network operation errors
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Interface not found: {0}")]
    InterfaceNotFound(String),
    
    #[error("Interface already exists: {0}")]
    InterfaceExists(String),
    
    #[error("IP allocation failed: {0}")]
    IpAllocationFailed(String),
    
    #[error("Subnet exhausted: {0}")]
    SubnetExhausted(String),
    
    #[error("Bridge error: {0}")]
    BridgeError(String),
    
    #[error("Netlink error: {0}")]
    Netlink(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Nix error: {0}")]
    Nix(#[from] nix::Error),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Generate deterministic MAC address from UUID
pub fn generate_mac(uuid: &uuid::Uuid) -> String {
    let bytes = uuid.as_bytes();
    // Locally administered unicast MAC
    format!(
        "02:00:00:{:02x}:{:02x}:{:02x}",
        bytes[0], bytes[1], bytes[2]
    )
}

/// Parse MAC address string to bytes
pub fn parse_mac(mac: &str) -> Result<[u8; 6], NetworkError> {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() != 6 {
        return Err(NetworkError::InvalidConfig("Invalid MAC format".to_string()));
    }
    
    let mut bytes = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        bytes[i] = u8::from_str_radix(part, 16)
            .map_err(|_| NetworkError::InvalidConfig("Invalid MAC hex".to_string()))?;
    }
    
    Ok(bytes)
}