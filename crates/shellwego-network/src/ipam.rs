//! IP Address Management
//! 
//! Tracks allocated IPs within a subnet to prevent collisions.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Mutex;

use crate::NetworkError;

/// Simple in-memory IPAM
pub struct Ipam {
    subnet: ipnetwork::Ipv4Network,
    gateway: Ipv4Addr,
    allocated: Mutex<HashMap<uuid::Uuid, Ipv4Addr>>,
    reserved: Vec<Ipv4Addr>, // Gateway, broadcast, etc
}

impl Ipam {
    pub fn new(subnet: ipnetwork::Ipv4Network) -> Self {
        let gateway = subnet.nth(1).expect("Valid subnet");
        
        Self {
            subnet,
            gateway,
            allocated: Mutex::new(HashMap::new()),
            reserved: vec![
                subnet.network(),           // Network address
                gateway,                    // Gateway
                subnet.broadcast(),         // Broadcast
            ],
        }
    }

    /// Allocate IP for app
    pub fn allocate(&self, app_id: uuid::Uuid) -> Result<Ipv4Addr, NetworkError> {
        let mut allocated = self.allocated.lock().unwrap();
        
        // Check if already has IP
        if let Some(&ip) = allocated.get(&app_id) {
            return Ok(ip);
        }
        
        // Find free IP
        for ip in self.subnet.iter() {
            if self.reserved.contains(&ip) {
                continue;
            }
            if allocated.values().any(|&v| v == ip) {
                continue;
            }
            
            allocated.insert(app_id, ip);
            return Ok(ip);
        }
        
        Err(NetworkError::SubnetExhausted(self.subnet.to_string()))
    }

    /// Allocate specific IP
    pub fn allocate_specific(
        &self,
        app_id: uuid::Uuid,
        requested: Ipv4Addr,
    ) -> Result<Ipv4Addr, NetworkError> {
        if !self.subnet.contains(requested) {
            return Err(NetworkError::IpAllocationFailed(
                format!("{} not in {}", requested, self.subnet)
            ));
        }
        
        if self.reserved.contains(&requested) {
            return Err(NetworkError::IpAllocationFailed(
                format!("{} is reserved", requested)
            ));
        }
        
        let mut allocated = self.allocated.lock().unwrap();
        
        if allocated.values().any(|&v| v == requested) {
            return Err(NetworkError::IpAllocationFailed(
                format!("{} already in use", requested)
            ));
        }
        
        allocated.insert(app_id, requested);
        Ok(requested)
    }

    /// Release IP
    pub fn release(&self, app_id: uuid::Uuid) {
        let mut allocated = self.allocated.lock().unwrap();
        allocated.remove(&app_id);
    }

    /// Get gateway
    pub fn gateway(&self) -> Ipv4Addr {
        self.gateway
    }

    /// Get subnet
    pub fn subnet(&self) -> ipnetwork::Ipv4Network {
        self.subnet
    }

    /// List allocations
    pub fn list(&self) -> Vec<(uuid::Uuid, Ipv4Addr)> {
        let allocated = self.allocated.lock().unwrap();
        allocated.iter().map(|(&k, &v)| (k, v)).collect()
    }
}