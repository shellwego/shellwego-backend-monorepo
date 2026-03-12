//! Network error types
//!
//! Defines errors for network operations.

use serde::{Deserialize, Serialize};
use std::io;
use thiserror::Error;

/// Network operation errors
#[derive(Debug, Clone, Error, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum NetworkError {
    /// I/O error
    #[error("I/O error: {0}")]
    Io(String),

    /// Interface not found
    #[error("Interface not found: {0}")]
    InterfaceNotFound(String),

    /// Interface already exists
    #[error("Interface already exists: {0}")]
    InterfaceExists(String),

    /// IP allocation failed
    #[error("IP allocation failed: {0}")]
    IpAllocationFailed(String),

    /// Subnet exhausted
    #[error("Subnet exhausted: {0}")]
    SubnetExhausted(String),

    /// Bridge error
    #[error("Bridge error: {0}")]
    BridgeError(String),

    /// Netlink error
    #[error("Netlink error: {0}")]
    Netlink(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Timeout error
    #[error("Operation timeout: {0}")]
    Timeout(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Generic error
    #[error("Network error: {0}")]
    Other(String),
}

impl From<io::Error> for NetworkError {
    fn from(err: io::Error) -> Self {
        NetworkError::Io(err.to_string())
    }
}

impl From<nix::Error> for NetworkError {
    fn from(err: nix::Error) -> Self {
        NetworkError::Netlink(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_error_display() {
        let err = NetworkError::InterfaceNotFound("eth0".to_string());
        assert_eq!(format!("{}", err), "Interface not found: eth0");

        let err = NetworkError::InvalidConfig("bad IP".to_string());
        assert_eq!(format!("{}", err), "Invalid configuration: bad IP");
    }

    #[test]
    fn test_network_error_from_io() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let net_err: NetworkError = io_err.into();
        assert!(matches!(net_err, NetworkError::Io(_)));
    }
}
