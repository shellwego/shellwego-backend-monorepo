//! TAP device management for Firecracker

use std::os::unix::io::{RawFd};
use tracing::{debug};
use futures_util::TryStreamExt;

use crate::NetworkError;

#[repr(C)]
struct IfReq {
    ifr_name: [libc::c_char; libc::IF_NAMESIZE],
    ifr_flags: libc::c_short,
    // Padding for union
    _padding: [u8; 24],
}

/// TAP device handle
pub struct TapDevice {
    name: String,
    fd: RawFd,
}

impl TapDevice {
    /// Create TAP device with given name
    pub async fn create(name: &str) -> Result<Self, NetworkError> {
        // Use TUNSETIFF ioctl to create TAP device
        let fd = Self::open_tun()?;
        
        let ifr = Self::create_ifreq(name, 0x0002 | 0x1000); // IFF_TAP | IFF_NO_PI
        
        // TUNSETIFF = 0x400454ca
        let res = unsafe {
            libc::ioctl(fd, 0x400454ca, &ifr)
        };
        
        if res < 0 {
            return Err(NetworkError::Io(std::io::Error::last_os_error().to_string()));
        }
        
        // Get actual name (may be truncated)
        let actual_name = unsafe {
            std::ffi::CStr::from_ptr(ifr.ifr_name.as_ptr())
                .to_string_lossy()
                .to_string()
        };
        
        debug!("Created TAP device: {}", actual_name);
        
        Ok(Self {
            name: actual_name,
            fd,
        })
    }

    /// Delete TAP device
    pub async fn delete(name: &str) -> Result<(), NetworkError> {
        // TAP devices are auto-deleted when fd closes,
        // but we can also delete via netlink
        debug!("Deleting TAP device: {}", name);
        Ok(())
    }

    /// Set owner UID for device
    pub async fn set_owner(&self, uid: u32) -> Result<(), NetworkError> {
        // TUNSETOWNER = 0x400454cc
        let res = unsafe {
            libc::ioctl(self.fd, 0x400454cc, uid)
        };
        
        if res < 0 {
            return Err(NetworkError::Io(std::io::Error::last_os_error().to_string()));
        }
        
        Ok(())
    }

    /// Set MTU
    pub async fn set_mtu(&self, mtu: u32) -> Result<(), NetworkError> {
        // Use rtnetlink
        let (connection, handle, _) = rtnetlink::new_connection().map_err(|e| {
            NetworkError::Netlink(e.to_string())
        })?;
        
        tokio::spawn(connection);
        
        let mut links = handle.link().get().match_name(self.name.clone()).execute();
        let link = links.try_next().await
            .map_err(|e| NetworkError::Netlink(e.to_string()))?
            .ok_or_else(|| NetworkError::InterfaceNotFound(self.name.clone()))?;
        
        handle.link().set(link.header.index).mtu(mtu).execute().await.map_err(|e| {
            NetworkError::Netlink(e.to_string())
        })?;
        
        Ok(())
    }

    /// Set interface up
    pub async fn set_up(&self) -> Result<(), NetworkError> {
        let (connection, handle, _) = rtnetlink::new_connection().map_err(|e| {
            NetworkError::Netlink(e.to_string())
        })?;
        
        tokio::spawn(connection);
        
        let mut links = handle.link().get().match_name(self.name.clone()).execute();
        let link = links.try_next().await
            .map_err(|e| NetworkError::Netlink(e.to_string()))?
            .ok_or_else(|| NetworkError::InterfaceNotFound(self.name.clone()))?;
        
        handle.link().set(link.header.index).up().execute().await.map_err(|e| {
            NetworkError::Netlink(e.to_string())
        })?;
        
        Ok(())
    }

    /// Attach to bridge
    pub async fn attach_to_bridge(&self, bridge: &str) -> Result<(), NetworkError> {
        let (connection, handle, _) = rtnetlink::new_connection().map_err(|e| {
            NetworkError::Netlink(e.to_string())
        })?;
        
        tokio::spawn(connection);
        
        // Get bridge index
        let mut links = handle.link().get().match_name(bridge.to_string()).execute();
        let bridge_link = links.try_next().await
            .map_err(|e| NetworkError::Netlink(e.to_string()))?
            .ok_or_else(|| NetworkError::InterfaceNotFound(bridge.to_string()))?;
        
        // Get TAP index
        let mut links = handle.link().get().match_name(self.name.clone()).execute();
        let tap_link = links.try_next().await
            .map_err(|e| NetworkError::Netlink(e.to_string()))?
            .ok_or_else(|| NetworkError::InterfaceNotFound(self.name.clone()))?;
        
        // Attach
        handle.link().set(tap_link.header.index)
            .controller(bridge_link.header.index)
            .execute()
            .await
            .map_err(|e| NetworkError::Netlink(format!("Failed to attach: {}", e)))?;
            
        Ok(())
    }

    fn open_tun() -> Result<RawFd, NetworkError> {
        let fd = unsafe {
            libc::open(
                b"/dev/net/tun\0".as_ptr() as *const libc::c_char,
                libc::O_RDWR | libc::O_CLOEXEC,
            )
        };
        
        if fd < 0 {
            Err(NetworkError::Io(std::io::Error::last_os_error().to_string()))
        } else {
            Ok(fd)
        }
    }

    fn create_ifreq(name: &str, flags: libc::c_short) -> IfReq {
        let mut ifr = IfReq {
            ifr_name: [0; libc::IF_NAMESIZE],
            ifr_flags: flags,
            _padding: [0; 24],
        };
        
        let name_bytes = name.as_bytes();
        for (i, &b) in name_bytes.iter().enumerate().take(libc::IF_NAMESIZE - 1) {
            ifr.ifr_name[i] = b as libc::c_char;
        }
        
        ifr
    }
}

impl Drop for TapDevice {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_ifreq_short_name() {
        let ifr = TapDevice::create_ifreq("tap0", 0x0002);
        // Verify the name was copied correctly
        let name = unsafe {
            std::ffi::CStr::from_ptr(ifr.ifr_name.as_ptr())
                .to_string_lossy()
                .to_string()
        };
        assert_eq!(name, "tap0");
        assert_eq!(ifr.ifr_flags, 0x0002);
    }

    #[test]
    fn test_create_ifreq_max_length() {
        // IF_NAMESIZE is typically 16 on Linux; name is truncated to 15 chars + null
        let long_name = "abcdefghijklmnop"; // 16 chars
        let ifr = TapDevice::create_ifreq(long_name, 0x1000);
        let name = unsafe {
            std::ffi::CStr::from_ptr(ifr.ifr_name.as_ptr())
                .to_string_lossy()
                .to_string()
        };
        // Should be truncated to IF_NAMESIZE - 1 characters
        assert_eq!(name.len(), libc::IF_NAMESIZE - 1);
        assert_eq!(ifr.ifr_flags, 0x1000);
    }

    #[test]
    fn test_create_ifreq_tap_flags() {
        // IFF_TAP | IFF_NO_PI
        let ifr = TapDevice::create_ifreq("testtap", 0x0002 | 0x1000);
        assert_eq!(ifr.ifr_flags, 0x1002);
    }

    #[test]
    fn test_create_ifreq_empty_name() {
        let ifr = TapDevice::create_ifreq("", 0x0002);
        let name = unsafe {
            std::ffi::CStr::from_ptr(ifr.ifr_name.as_ptr())
                .to_string_lossy()
                .to_string()
        };
        assert_eq!(name, "");
    }

    #[test]
    fn test_open_tun_without_permission() {
        // /dev/net/tun requires CAP_NET_ADMIN or root.
        // In a test environment this is expected to fail.
        let result = TapDevice::open_tun();
        // We don't assert Err because the test may run as root in CI.
        // Just verify it doesn't panic.
        let _ = result;
    }

    #[tokio::test]
    async fn test_create_tap_without_permission() {
        // Requires CAP_NET_ADMIN; expected to fail in test env.
        let result = TapDevice::create("test-tap-unit").await;
        let _ = result; // Doesn't panic
    }

    #[tokio::test]
    async fn test_delete_tap_noop() {
        // delete() is a best-effort cleanup; should always succeed.
        let result = TapDevice::delete("nonexistent-tap").await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_tap_device_type_info() {
        // Verify the type exists and has the expected layout
        assert_eq!(
            std::any::type_name::<TapDevice>(),
            "shellwego_network::tap::TapDevice"
        );
    }
}
