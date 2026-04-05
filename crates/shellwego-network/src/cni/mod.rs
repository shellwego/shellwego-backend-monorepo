//! CNI (Container Network Interface) implementation
//!
//! Sets up networking for microVMs using Linux bridge + TAP devices.
//! Compatible with standard CNI plugins but optimized for Firecracker.
//!
//! Teardown properly cleans up all resources: TAP devices, iptables rules,
//! tc qdiscs, and eBPF programs.

use std::net::Ipv4Addr;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    bridge::Bridge, ebpf::EbpfManager, ipam::Ipam, tap::TapDevice, NetworkConfig, NetworkError,
    NetworkSetup,
};

/// CNI network manager
pub struct CniNetwork {
    bridge: Bridge,
    ipam: Ipam,
    ebpf: EbpfManager,
    mtu: u32,
    /// Track attached TAP devices for cleanup
    attached_taps: std::sync::Mutex<Vec<String>>,
}

impl CniNetwork {
    /// Initialize CNI for a node
    pub async fn new(bridge_name: &str, node_cidr: &str) -> Result<Self, NetworkError> {
        let subnet: ipnetwork::Ipv4Network = node_cidr
            .parse()
            .map_err(|e| NetworkError::InvalidConfig(format!("Invalid CIDR: {}", e)))?;

        // Validate CIDR
        validate_cidr(&subnet)?;

        // Validate bridge name
        validate_bridge_name(bridge_name)?;

        // Ensure bridge exists
        let bridge = Bridge::create_or_get(bridge_name).await?;

        // Setup IPAM for this subnet
        let ipam = Ipam::new(subnet);

        // Configure bridge IP (first usable)
        let bridge_ip = subnet
            .nth(1)
            .ok_or_else(|| NetworkError::InvalidConfig("CIDR too small".to_string()))?;
        bridge.set_ip(bridge_ip, subnet).await?;
        bridge.set_up().await?;

        // Enable IP forwarding
        enable_ip_forwarding().await?;

        info!("CNI initialized: bridge {} on {}", bridge_name, node_cidr);

        Ok(Self {
            bridge,
            ipam,
            ebpf: EbpfManager::new()
                .await
                .map_err(|e| NetworkError::InvalidConfig(e.to_string()))?,
            mtu: 1500,
            attached_taps: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Setup network for a microVM
    pub async fn setup(&self, config: &NetworkConfig) -> Result<NetworkSetup, NetworkError> {
        debug!("Setting up network for VM {}", config.vm_id);

        // Validate configuration
        validate_network_config(config)?;

        // Allocate IP if not specified
        let guest_ip = if config.guest_ip == Ipv4Addr::UNSPECIFIED {
            self.ipam.allocate(config.app_id)?
        } else {
            self.ipam.allocate_specific(config.app_id, config.guest_ip)?
        };

        let host_ip = self.ipam.gateway();

        // Create TAP device
        let tap = TapDevice::create(&config.tap_name).await?;
        tap.set_owner(std::process::id()).await?;
        tap.set_mtu(self.mtu).await?;
        tap.attach_to_bridge(&self.bridge.name()).await?;
        tap.set_up().await?;

        // Track the TAP device for later cleanup
        {
            let mut taps = self.attached_taps.lock().unwrap();
            if !taps.contains(&config.tap_name) {
                taps.push(config.tap_name.clone());
            }
        }

        // Apply QoS via eBPF (no-op in fallback mode)
        if let Some(limit_mbps) = config.bandwidth_limit_mbps {
            self.ebpf
                .apply_qos(&config.tap_name, limit_mbps)
                .await
                .map_err(|e| NetworkError::InvalidConfig(e.to_string()))?;
        }

        // Attach firewall via eBPF (no-op in fallback mode)
        self.ebpf
            .attach_firewall(&config.tap_name)
            .await
            .map_err(|e| NetworkError::InvalidConfig(e.to_string()))?;

        info!(
            "Network ready for {}: TAP {} with IP {}/{}",
            config.app_id,
            config.tap_name,
            guest_ip,
            self.ipam.subnet().prefix()
        );

        Ok(NetworkSetup {
            tap_device: config.tap_name.clone(),
            guest_ip,
            host_ip,
            veth_pair: None,
        })
    }

    /// Teardown network for a microVM
    ///
    /// Cleans up:
    /// - eBPF firewall / QoS programs attached to the TAP device
    /// - tc qdiscs on the TAP device
    /// - iptables rules referencing the TAP device
    /// - The TAP device itself
    /// - IPAM allocation
    pub async fn teardown(&self, app_id: Uuid, tap_name: &str) -> Result<(), NetworkError> {
        debug!("Tearing down network for {} (tap: {})", app_id, tap_name);

        // 1. Remove tc qdiscs on the TAP device
        cleanup_tc_rules(tap_name).await;

        // 2. Remove iptables rules referencing this TAP device
        cleanup_iptables_for_tap(tap_name).await;

        // 3. Detach XDP program from the TAP device
        cleanup_xdp(tap_name).await;

        // 4. Release IP
        self.ipam.release(app_id);

        // 5. Delete TAP device
        TapDevice::delete(tap_name).await?;

        // 6. Remove from tracking
        {
            let mut taps = self.attached_taps.lock().unwrap();
            taps.retain(|t| t != tap_name);
        }

        info!("Network torn down for {} (tap: {})", app_id, tap_name);
        Ok(())
    }

    /// Teardown all attached networks and clean up global resources.
    pub async fn teardown_all(&self) -> Result<(), NetworkError> {
        info!("Tearing down all CNI networks");

        // Collect taps to tear down (drain the list)
        let taps: Vec<String> = {
            let mut guard = self.attached_taps.lock().unwrap();
            std::mem::take(&mut *guard)
        };

        for tap_name in taps {
            // We don't know the app_id for each tap, so use a nil UUID
            // The IPAM release with an unknown key is a no-op.
            cleanup_tc_rules(&tap_name).await;
            cleanup_iptables_for_tap(&tap_name).await;
            cleanup_xdp(&tap_name).await;
            let _ = TapDevice::delete(&tap_name).await;
        }

        // Detach all eBPF programs
        let _ = self.ebpf.detach_all().await;

        // Clean up global iptables chain
        cleanup_global_firewall_rules().await;

        info!("All CNI networks torn down");
        Ok(())
    }

    /// Get bridge interface name
    pub fn bridge_name(&self) -> &str {
        &self.bridge.name()
    }

    /// Get a list of currently attached TAP device names
    pub fn attached_taps(&self) -> Vec<String> {
        self.attached_taps.lock().unwrap().clone()
    }
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Validate that the subnet is usable for CNI.
fn validate_cidr(subnet: &ipnetwork::Ipv4Network) -> Result<(), NetworkError> {
    if subnet.prefix() < 24 {
        return Err(NetworkError::InvalidConfig(
            "CIDR must be at least /24 for CNI networking".to_string(),
        ));
    }
    if subnet.prefix() > 30 {
        return Err(NetworkError::InvalidConfig(
            "CIDR must be at most /30 for CNI networking".to_string(),
        ));
    }
    Ok(())
}

/// Validate the bridge name.
fn validate_bridge_name(name: &str) -> Result<(), NetworkError> {
    if name.is_empty() {
        return Err(NetworkError::InvalidConfig(
            "Bridge name cannot be empty".to_string(),
        ));
    }
    if name.len() > 15 {
        return Err(NetworkError::InvalidConfig(
            "Bridge name must be 15 characters or fewer".to_string(),
        ));
    }
    // Interface names must be alphanumeric plus hyphens/dots, not starting with a hyphen/dot
    let chars: Vec<char> = name.chars().collect();
    if chars[0] == '-' || chars[0] == '.' {
        return Err(NetworkError::InvalidConfig(
            "Bridge name must not start with '-' or '.'".to_string(),
        ));
    }
    for c in &chars {
        if !c.is_alphanumeric() && *c != '-' && *c != '.' {
            return Err(NetworkError::InvalidConfig(format!(
                "Bridge name contains invalid character: '{}'",
                c
            )));
        }
    }
    Ok(())
}

/// Validate the full NetworkConfig.
fn validate_network_config(config: &NetworkConfig) -> Result<(), NetworkError> {
    if config.tap_name.is_empty() {
        return Err(NetworkError::InvalidConfig(
            "TAP device name cannot be empty".to_string(),
        ));
    }
    if config.bridge_name.is_empty() {
        return Err(NetworkError::InvalidConfig(
            "Bridge name cannot be empty".to_string(),
        ));
    }
    if config.mtu == 0 {
        return Err(NetworkError::InvalidConfig(
            "MTU must be greater than 0".to_string(),
        ));
    }
    if let Some(limit) = config.bandwidth_limit_mbps {
        if limit == 0 {
            return Err(NetworkError::InvalidConfig(
                "Bandwidth limit must be greater than 0".to_string(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Cleanup helpers
// ---------------------------------------------------------------------------

/// Remove tc qdiscs on a TAP device.
async fn cleanup_tc_rules(tap_name: &str) {
    use tokio::process::Command;

    // Remove root qdisc
    let _ = Command::new("tc")
        .args(["qdisc", "del", "dev", tap_name, "root"])
        .output()
        .await;

    // Remove ingress qdisc
    let _ = Command::new("tc")
        .args(["qdisc", "del", "dev", tap_name, "ingress"])
        .output()
        .await;

    debug!("Cleaned up tc rules on {}", tap_name);
}

/// Remove iptables rules that reference a specific TAP device.
async fn cleanup_iptables_for_tap(tap_name: &str) {
    use tokio::process::Command;

    // Remove rules matching the TAP device as input/output interface
    let chains = ["INPUT", "FORWARD", "OUTPUT"];

    for chain in &chains {
        // Try multiple times to remove all matching rules (there could be multiple)
        for _ in 0..10 {
            let output = Command::new("iptables")
                .args([
                    "-D",
                    chain,
                    "-i",
                    tap_name,
                    "-j",
                    "SHELLWEGO-FW",
                ])
                .output()
                .await;

            if let Ok(out) = output {
                if !out.status.success() {
                    break; // No more matching rules
                }
            } else {
                break;
            }
        }
    }

    // Remove any direct DROP/ACCEPT rules for this TAP
    for chain in &chains {
        for _ in 0..10 {
            let output = Command::new("iptables")
                .args(["-D", chain, "-i", tap_name, "-j", "DROP"])
                .output()
                .await;

            if let Ok(out) = output {
                if !out.status.success() {
                    break;
                }
            } else {
                break;
            }
        }
    }

    debug!("Cleaned up iptables rules for {}", tap_name);
}

/// Detach XDP program from a TAP device.
async fn cleanup_xdp(tap_name: &str) {
    use tokio::process::Command;

    let _ = Command::new("ip")
        .args(["link", "set", "dev", tap_name, "xdp", "off"])
        .output()
        .await;

    debug!("Cleaned up XDP on {}", tap_name);
}

/// Remove the global SHELLWEGO-FW iptables chain.
async fn cleanup_global_firewall_rules() {
    use tokio::process::Command;

    // Remove jumps
    for chain in ["INPUT", "FORWARD"] {
        let _ = Command::new("iptables")
            .args(["-D", chain, "-j", "SHELLWEGO-FW"])
            .output()
            .await;
    }

    // Flush and delete chain
    let _ = Command::new("iptables")
        .args(["-F", "SHELLWEGO-FW"])
        .output()
        .await;

    let _ = Command::new("iptables")
        .args(["-X", "SHELLWEGO-FW"])
        .output()
        .await;

    debug!("Cleaned up global firewall rules");
}

/// Enable IP forwarding.
async fn enable_ip_forwarding() -> Result<(), NetworkError> {
    tokio::fs::write("/proc/sys/net/ipv4/ip_forward", "1")
        .await
        .map_err(|e| NetworkError::Io(e.to_string()))?;

    tokio::fs::write("/proc/sys/net/ipv6/conf/all/forwarding", "1")
        .await
        .map_err(|e| NetworkError::Io(e.to_string()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ipnetwork::Ipv4Network;

    fn make_config(app_id: Uuid) -> NetworkConfig {
        NetworkConfig {
            app_id,
            vm_id: Uuid::new_v4(),
            bridge_name: "br0".to_string(),
            tap_name: "tap0".to_string(),
            guest_mac: "02:00:00:00:00:00".to_string(),
            guest_ip: Ipv4Addr::UNSPECIFIED,
            host_ip: Ipv4Addr::UNSPECIFIED,
            subnet: Ipv4Network::new(Ipv4Addr::new(10, 0, 0, 0), 24).unwrap(),
            gateway: Ipv4Addr::new(10, 0, 0, 1),
            mtu: 1500,
            bandwidth_limit_mbps: None,
        }
    }

    // --- Validation tests ---

    #[test]
    fn test_validate_cidr_ok() {
        let subnet: Ipv4Network = "10.0.0.0/24".parse().unwrap();
        assert!(validate_cidr(&subnet).is_ok());

        let subnet: Ipv4Network = "10.0.0.0/28".parse().unwrap();
        assert!(validate_cidr(&subnet).is_ok());

        let subnet: Ipv4Network = "10.0.0.0/30".parse().unwrap();
        assert!(validate_cidr(&subnet).is_ok());
    }

    #[test]
    fn test_validate_cidr_too_small() {
        let subnet: Ipv4Network = "10.0.0.0/16".parse().unwrap();
        assert!(validate_cidr(&subnet).is_err());
    }

    #[test]
    fn test_validate_cidr_too_large() {
        let subnet: Ipv4Network = "10.0.0.0/31".parse().unwrap();
        assert!(validate_cidr(&subnet).is_err());
    }

    #[test]
    fn test_validate_bridge_name_ok() {
        assert!(validate_bridge_name("br0").is_ok());
        assert!(validate_bridge_name("my-bridge").is_ok());
        assert!(validate_bridge_name("bridge.1").is_ok());
        assert!(validate_bridge_name("a").is_ok());
    }

    #[test]
    fn test_validate_bridge_name_empty() {
        assert!(validate_bridge_name("").is_err());
    }

    #[test]
    fn test_validate_bridge_name_too_long() {
        assert!(validate_bridge_name("abcdefghijklmnop").is_err());
    }

    #[test]
    fn test_validate_bridge_name_bad_start() {
        assert!(validate_bridge_name("-bad").is_err());
        assert!(validate_bridge_name(".bad").is_err());
    }

    #[test]
    fn test_validate_bridge_name_invalid_chars() {
        assert!(validate_bridge_name("br 0").is_err());
        assert!(validate_bridge_name("br!0").is_err());
    }

    #[test]
    fn test_validate_network_config_ok() {
        let config = make_config(Uuid::new_v4());
        assert!(validate_network_config(&config).is_ok());
    }

    #[test]
    fn test_validate_network_config_empty_tap() {
        let mut config = make_config(Uuid::new_v4());
        config.tap_name = String::new();
        assert!(validate_network_config(&config).is_err());
    }

    #[test]
    fn test_validate_network_config_empty_bridge() {
        let mut config = make_config(Uuid::new_v4());
        config.bridge_name = String::new();
        assert!(validate_network_config(&config).is_err());
    }

    #[test]
    fn test_validate_network_config_zero_mtu() {
        let mut config = make_config(Uuid::new_v4());
        config.mtu = 0;
        assert!(validate_network_config(&config).is_err());
    }

    #[test]
    fn test_validate_network_config_zero_bandwidth() {
        let mut config = make_config(Uuid::new_v4());
        config.bandwidth_limit_mbps = Some(0);
        assert!(validate_network_config(&config).is_err());
    }

    // --- IPAM integration tests ---

    #[test]
    fn test_ipam_allocate_release() {
        let subnet: Ipv4Network = "10.0.0.0/24".parse().unwrap();
        let ipam = Ipam::new(subnet);

        let app1 = Uuid::new_v4();
        let ip = ipam.allocate(app1).unwrap();
        assert_eq!(ip, Ipv4Addr::new(10, 0, 0, 2));

        ipam.release(app1);

        let app2 = Uuid::new_v4();
        let ip2 = ipam.allocate(app2).unwrap();
        assert_eq!(ip2, Ipv4Addr::new(10, 0, 0, 2)); // Reused
    }
}
