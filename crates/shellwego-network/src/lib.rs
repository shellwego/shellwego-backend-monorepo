//! Network management for ShellWeGo
//! 
//! Sets up CNI-style networking for Firecracker microVMs:
//! - Bridge creation and management
//! - TAP device allocation
//! - IPAM (IP address management)
//! - eBPF-based filtering and QoS

// Re-export types from schema crate
pub use shellwego_schema::{
    NetworkConfig, NetworkSetup, NetworkError, 
    Message, QuicConfig, AgentConnection, generate_mac, parse_mac,
};

// Local modules
pub mod cni;
pub mod bridge;
pub mod tap;
pub mod ipam;
pub mod discovery;
pub mod quinn;
pub mod ebpf;
pub mod vxlan;
pub mod wireguard;

pub use cni::CniNetwork;
pub use bridge::Bridge;
pub use tap::TapDevice;
pub use ipam::Ipam;
pub use quinn::{QuinnClient, QuinnServer};
