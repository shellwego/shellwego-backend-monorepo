//! Linux bridge management

use rtnetlink::{new_connection, Handle};
use std::net::Ipv4Addr;
use tracing::{info, debug};
use futures_util::TryStreamExt;

use crate::NetworkError;

/// Linux bridge interface
pub struct Bridge {
    name: String,
    handle: Handle,
}

impl Bridge {
    /// Create new bridge or get existing
    pub async fn create_or_get(name: &str) -> Result<Self, NetworkError> {
        let (connection, handle, _) = new_connection().map_err(|e| {
            NetworkError::Netlink(format!("Failed to create netlink connection: {}", e))
        })?;
        
        // Spawn connection handler
        tokio::spawn(connection);
        
        // Check if exists
        let mut links = handle.link().get().match_name(name.to_string()).execute();
        
        if let Some(link) = links.try_next().await.map_err(|e| NetworkError::Netlink(e.to_string()))? {
            let _link = link;
            debug!("Using existing bridge: {}", name);
            return Ok(Self {
                name: name.to_string(),
                handle,
            });
        }
        
        // Create bridge
        info!("Creating bridge: {}", name);
        
        handle
            .link()
            .add()
            .bridge(name.to_string())
            .execute()
            .await
            .map_err(|e| NetworkError::Netlink(format!("Failed to create bridge: {}", e)))?;
            
        // Wait for creation
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        Ok(Self {
            name: name.to_string(),
            handle,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set bridge IP address
    pub async fn set_ip(
        &self,
        addr: Ipv4Addr,
        subnet: ipnetwork::Ipv4Network,
    ) -> Result<(), NetworkError> {
        let index = self.get_index().await?;
        
        // Flush existing addresses
        let mut addrs = self.handle.address().get().set_link_index_filter(index).execute();
        while let Some(addr) = addrs.try_next().await.map_err(|e| NetworkError::Netlink(e.to_string()))? {
            self.handle.address().del(addr).execute().await.ok();
        }
        
        // Add new address
        self.handle
            .address()
            .add(index, std::net::IpAddr::V4(addr), subnet.prefix())
            .execute()
            .await
            .map_err(|e| NetworkError::Netlink(format!("Failed to set IP: {}", e)))?;
            
        Ok(())
    }

    /// Set interface up
    pub async fn set_up(&self) -> Result<(), NetworkError> {
        let index = self.get_index().await?;
        
        self.handle
            .link()
            .set(index)
            .up()
            .execute()
            .await
            .map_err(|e| NetworkError::Netlink(format!("Failed to set up: {}", e)))?;
            
        Ok(())
    }

    /// Attach interface to bridge
    pub async fn attach(&self, iface_index: u32) -> Result<(), NetworkError> {
        let bridge_index = self.get_index().await?;
        
        self.handle
            .link()
            .set(iface_index)
            .controller(bridge_index)
            .execute()
            .await
            .map_err(|e| NetworkError::Netlink(format!("Failed to attach: {}", e)))?;
            
        Ok(())
    }

    async fn get_index(&self) -> Result<u32, NetworkError> {
        let mut links = self.handle
            .link()
            .get()
            .match_name(self.name.clone())
            .execute();
            
        let link = links.try_next().await
            .map_err(|e| NetworkError::Netlink(e.to_string()))?
            .ok_or_else(|| NetworkError::InterfaceNotFound(self.name.clone()))?;
        
        Ok(link.header.index)
    }
}
