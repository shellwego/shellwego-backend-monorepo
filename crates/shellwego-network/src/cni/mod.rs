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
};

/// CNI network manager
pub struct CniNetwork {
    bridge: Bridge,
    ipam: Ipam,
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
        
        // Setup NAT for outbound traffic
        setup_nat(&subnet).await?;
        
        info!("CNI initialized: bridge {} on {}", bridge_name, node_cidr);
        
        Ok(Self {
            bridge,
            ipam,
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
        
        // Setup bandwidth limiting if requested
        if let Some(limit_mbps) = config.bandwidth_limit_mbps {
            setup_tc_bandwidth(&config.tap_name, limit_mbps).await?;
        }
        
        // TODO: Setup firewall rules (nftables or eBPF)
        // TODO: Port forwarding if public IP
        
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

async fn setup_nat(subnet: &ipnetwork::Ipv4Network) -> Result<(), NetworkError> {
    // Use nftables or iptables for NAT
    // Prefer nftables on modern systems
    
    let rule = format!(
        "ip saddr {} oifname != \"{}\" masquerade",
        subnet, "shellwego0" // TODO: Use actual bridge name
    );
    
    // Check if nftables is available
    let nft_check = tokio::process::Command::new("nft")
        .arg("list")
        .output()
        .await;
        
    if nft_check.is_ok() && nft_check.unwrap().status.success() {
        // Use nftables
        setup_nftables_nat(subnet).await?;
    } else {
        // Fallback to iptables
        setup_iptables_nat(subnet).await?;
    }
    
    Ok(())
}

async fn setup_nftables_nat(subnet: &ipnetwork::Ipv4Network) -> Result<(), NetworkError> {
    // TODO: Create table if not exists
    // TODO: Add masquerade rule for subnet
    
    let _ = tokio::process::Command::new("nft")
        .args([
            "add", "rule", "ip", "nat", "postrouting",
            "ip", "saddr", &subnet.to_string(),
            "masquerade",
        ])
        .output()
        .await?;
        
    Ok(())
}

async fn setup_iptables_nat(subnet: &ipnetwork::Ipv4Network) -> Result<(), NetworkError> {
    let _ = tokio::process::Command::new("iptables")
        .args([
            "-t", "nat", "-A", "POSTROUTING",
            "-s", &subnet.to_string(),
            "!", "-o", "shellwego0", // TODO
            "-j", "MASQUERADE",
        ])
        .output()
        .await?;
        
    Ok(())
}

async fn setup_tc_bandwidth(iface: &str, limit_mbps: u32) -> Result<(), NetworkError> {
    // Setup traffic control (tc) for bandwidth limiting
    // HTB (Hierarchical Token Bucket) qdisc
    
    // Delete existing
    let _ = tokio::process::Command::new("tc")
        .args(["qdisc", "del", "dev", iface, "root"])
        .output()
        .await;
        
    // Add HTB
    let output = tokio::process::Command::new("tc")
        .args([
            "qdisc", "add", "dev", iface, "root",
            "handle", "1:", "htb", "default", "10",
        ])
        .output()
        .await?;
        
    if !output.status.success() {
        return Err(NetworkError::BridgeError(
            String::from_utf8_lossy(&output.stderr).to_string()
        ));
    }
    
    // Add class with rate limit
    let kbit = limit_mbps * 1000;
    let output = tokio::process::Command::new("tc")
        .args([
            "class", "add", "dev", iface, "parent", "1:",
            "classid", "1:10", "htb",
            "rate", &format!("{}kbit", kbit),
            "ceil", &format!("{}kbit", kbit),
        ])
        .output()
        .await?;
        
    if !output.status.success() {
        return Err(NetworkError::BridgeError(
            String::from_utf8_lossy(&output.stderr).to_string()
        ));
    }
    
    Ok(())
}