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
            return Err(NetworkError::Io(std::io::Error::last_os_error()));
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
            return Err(NetworkError::Io(std::io::Error::last_os_error()));
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
            Err(NetworkError::Io(std::io::Error::last_os_error()))
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
