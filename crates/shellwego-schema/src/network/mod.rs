//! Network types for ShellWeGo
//!
//! This module contains all types related to network configuration,
//! setup, and QUIC-based communication between control plane and agents.

pub mod config;
pub mod error;
pub mod quinn;

// Re-export commonly used types at module level
pub use config::{NetworkConfig, NetworkSetup};
pub use error::NetworkError;
pub use quinn::{AgentConnection, ChannelPriority, Message, QuicConfig, ResourceLimits};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_mac() {
        let uuid = uuid::Uuid::nil();
        let mac = generate_mac(&uuid);
        assert_eq!(mac, "02:00:00:00:00:00");

        let uuid = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let mac = generate_mac(&uuid);
        assert_eq!(mac, "02:00:00:55:0e:84");
    }

    #[test]
    fn test_parse_mac() {
        let mac_str = "02:00:00:55:0e:84";
        let bytes = parse_mac(mac_str).unwrap();
        assert_eq!(bytes, [0x02, 0x00, 0x00, 0x55, 0x0e, 0x84]);

        assert!(parse_mac("invalid").is_err());
        assert!(parse_mac("02:00:00:55:0e:8G").is_err());
    }
}
