//! CNI (Container Network Interface) implementation
//! 
//! Sets up networking for microVMs using Linux bridge + TAP devices.
//! Compatible with standard CNI plugins but optimized for Firecracker.

use std::net::Ipv4Addr;
use tracing::{info, debug, warn};

use crate::{
    NetworkConfig, NetworkSetup, NetworkError,
    bridge::Bridge,
    tap::TapDevice,
    ipam::Ipam,
    ebpf::EbpfManager,
};

/// CNI network manager
pub struct CniNetwork {
    bridge: Bridge,
    ipam: Ipam,
    ebpf: EbpfManager,
    mtu: u32,
}

impl CniNetwork {
    /// Initialize CNI for a node
    pub async fn new(
        bridge_name: &str,
        node_cidr: &str,
    ) -> Result<Self, NetworkError> {
        let subnet: ipnetwork::Ipv4Network = node_cidr.parse()
            .map_err(|e| NetworkError::InvalidConfig(format!("Invalid CIDR: {}", e)))?;
            
        // Ensure bridge exists
        let bridge = Bridge::create_or_get(bridge_name).await?;
        
        // Setup IPAM for this subnet
        let ipam = Ipam::new(subnet);
        
        // Configure bridge IP (first usable)
        let bridge_ip = subnet.nth(1)
            .ok_or_else(|| NetworkError::InvalidConfig("CIDR too small".to_string()))?;
        bridge.set_ip(bridge_ip, subnet).await?;
        bridge.set_up().await?;
        
        // Enable IP forwarding
        enable_ip_forwarding().await?;
        
        info!("CNI initialized: bridge {} on {}", bridge_name, node_cidr);
        
        Ok(Self {
            bridge,
            ipam,
            ebpf: EbpfManager::new().await?,
            mtu: 1500,
        })
    }

    /// Setup network for a microVM
    pub async fn setup(&self, config: &NetworkConfig) -> Result<NetworkSetup, NetworkError> {
        debug!("Setting up network for VM {}", config.vm_id);
        
        // Allocate IP if not specified
        let guest_ip = if config.guest_ip == Ipv4Addr::UNSPECIFIED {
            self.ipam.allocate(config.app_id)?
        } else {
            self.ipam.allocate_specific(config.app_id, config.guest_ip)?
        };
        
        let host_ip = self.ipam.gateway();
        
        // Create TAP device
        let tap = TapDevice::create(&config.tap_name).await?;
        tap.set_owner(std::process::id()).await?; // Firecracker runs as same user
        tap.set_mtu(self.mtu).await?;
        tap.attach_to_bridge(&self.bridge.name()).await?;
        tap.set_up().await?;
        
        // Replaced legacy tc-htb with eBPF-QoS
        if let Some(limit_mbps) = config.bandwidth_limit_mbps {
            self.ebpf.apply_qos(&config.tap_name, limit_mbps).await?;
        }
        self.ebpf.attach_firewall(&config.tap_name).await?;
        
        info!(
            "Network ready for {}: TAP {} with IP {}/{}",
            config.app_id, config.tap_name, guest_ip, self.ipam.subnet().prefix()
        );
        
        Ok(NetworkSetup {
            tap_device: config.tap_name.clone(),
            guest_ip,
            host_ip,
            veth_pair: None,
        })
    }

    /// Teardown network for a microVM
    pub async fn teardown(&self, app_id: uuid::Uuid, tap_name: &str) -> Result<(), NetworkError> {
        debug!("Tearing down network for {}", app_id);
        
        // Release IP
        self.ipam.release(app_id);
        
        // Delete TAP device
        TapDevice::delete(tap_name).await?;
        
        // TODO: Clean up tc rules
        // TODO: Clean up firewall rules
        
        Ok(())
    }

    /// Get bridge interface name
    pub fn bridge_name(&self) -> &str {
        &self.bridge.name()
    }
}

async fn enable_ip_forwarding() -> Result<(), NetworkError> {
    tokio::fs::write("/proc/sys/net/ipv4/ip_forward", "1").await
        .map_err(|e| NetworkError::Io(e))?;
        
    tokio::fs::write("/proc/sys/net/ipv6/conf/all/forwarding", "1").await
        .map_err(|e| NetworkError::Io(e))?;
        
    Ok(())
}