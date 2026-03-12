//! Network configuration types
//!
//! Defines network configuration for microVMs and host networking.

use ipnetwork::Ipv4Network;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use uuid::Uuid;

/// Network configuration for a microVM
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct NetworkConfig {
    /// Application ID
    pub app_id: Uuid,
    /// VM instance ID
    pub vm_id: Uuid,
    /// Bridge name for this network
    pub bridge_name: String,
    /// TAP device name
    pub tap_name: String,
    /// Guest MAC address
    pub guest_mac: String,
    /// Guest IP address
    pub guest_ip: Ipv4Addr,
    /// Host IP address
    pub host_ip: Ipv4Addr,
    /// Subnet configuration (CIDR notation, e.g., "10.0.0.0/24")
    #[cfg_attr(feature = "openapi", schemars(with = "String"))]
    pub subnet: Ipv4Network,
    /// Gateway IP address
    pub gateway: Ipv4Addr,
    /// MTU for the interface
    pub mtu: u16,
    /// Optional bandwidth limit in Mbps
    pub bandwidth_limit_mbps: Option<u32>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            app_id: Uuid::nil(),
            vm_id: Uuid::nil(),
            bridge_name: "br0".to_string(),
            tap_name: "tap0".to_string(),
            guest_mac: "02:00:00:00:00:00".to_string(),
            guest_ip: Ipv4Addr::new(10, 0, 0, 2),
            host_ip: Ipv4Addr::new(10, 0, 0, 1),
            subnet: Ipv4Network::new(Ipv4Addr::new(10, 0, 0, 0), 24).unwrap(),
            gateway: Ipv4Addr::new(10, 0, 0, 1),
            mtu: 1500,
            bandwidth_limit_mbps: None,
        }
    }
}

impl NetworkConfig {
    /// Create a new network configuration with the given app and VM IDs
    pub fn new(app_id: Uuid, vm_id: Uuid) -> Self {
        Self {
            app_id,
            vm_id,
            ..Default::default()
        }
    }

    /// Set the bridge name
    pub fn with_bridge(mut self, name: &str) -> Self {
        self.bridge_name = name.to_string();
        self
    }

    /// Set the TAP device name
    pub fn with_tap(mut self, name: &str) -> Self {
        self.tap_name = name.to_string();
        self
    }

    /// Set the guest MAC address
    pub fn with_guest_mac(mut self, mac: &str) -> Self {
        self.guest_mac = mac.to_string();
        self
    }

    /// Set the bandwidth limit
    pub fn with_bandwidth_limit(mut self, mbps: u32) -> Self {
        self.bandwidth_limit_mbps = Some(mbps);
        self
    }
}

/// Network setup result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct NetworkSetup {
    /// TAP device name
    pub tap_device: String,
    /// Guest IP address
    pub guest_ip: Ipv4Addr,
    /// Host IP address
    pub host_ip: Ipv4Addr,
    /// Optional veth pair (if using veth instead of tap)
    pub veth_pair: Option<(String, String)>,
}

impl NetworkSetup {
    /// Create a new network setup result
    pub fn new(tap_device: String, guest_ip: Ipv4Addr, host_ip: Ipv4Addr) -> Self {
        Self {
            tap_device,
            guest_ip,
            host_ip,
            veth_pair: None,
        }
    }

    /// Add a veth pair
    pub fn with_veth(mut self, host_veth: String, guest_veth: String) -> Self {
        self.veth_pair = Some((host_veth, guest_veth));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_config_default() {
        let config = NetworkConfig::default();
        assert_eq!(config.bridge_name, "br0");
        assert_eq!(config.mtu, 1500);
        assert!(config.bandwidth_limit_mbps.is_none());
    }

    #[test]
    fn test_network_config_builder() {
        let app_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let config = NetworkConfig::new(app_id, vm_id)
            .with_bridge("br1")
            .with_tap("tap1")
            .with_guest_mac("02:00:00:00:00:01")
            .with_bandwidth_limit(1000);

        assert_eq!(config.app_id, app_id);
        assert_eq!(config.bridge_name, "br1");
        assert_eq!(config.tap_name, "tap1");
        assert_eq!(config.guest_mac, "02:00:00:00:00:01");
        assert_eq!(config.bandwidth_limit_mbps, Some(1000));
    }

    #[test]
    fn test_network_setup_new() {
        let setup = NetworkSetup::new(
            "tap0".to_string(),
            Ipv4Addr::new(10, 0, 0, 2),
            Ipv4Addr::new(10, 0, 0, 1),
        );

        assert_eq!(setup.tap_device, "tap0");
        assert!(setup.veth_pair.is_none());
    }

    #[test]
    fn test_network_setup_with_veth() {
        let setup = NetworkSetup::new(
            "tap0".to_string(),
            Ipv4Addr::new(10, 0, 0, 2),
            Ipv4Addr::new(10, 0, 0, 1),
        )
        .with_veth("veth_host".to_string(), "veth_guest".to_string());

        assert!(setup.veth_pair.is_some());
        let (host, guest) = setup.veth_pair.unwrap();
        assert_eq!(host, "veth_host");
        assert_eq!(guest, "veth_guest");
    }
}
