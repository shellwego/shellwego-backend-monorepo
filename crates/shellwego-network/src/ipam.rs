//! IP Address Management
//! 
//! Tracks allocated IPs within a subnet to prevent collisions.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Mutex;
use uuid::Uuid;

use crate::NetworkError;

/// Simple in-memory IPAM
pub struct Ipam {
    subnet: ipnetwork::Ipv4Network,
    gateway: Ipv4Addr,
    allocated: Mutex<HashMap<Uuid, Ipv4Addr>>,
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
    pub fn allocate(&self, app_id: Uuid) -> Result<Ipv4Addr, NetworkError> {
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
        app_id: Uuid,
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
    pub fn release(&self, app_id: Uuid) {
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
    pub fn list(&self) -> Vec<(Uuid, Ipv4Addr)> {
        let allocated = self.allocated.lock().unwrap();
        allocated.iter().map(|(&k, &v)| (k, v)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipnetwork::Ipv4Network;

    #[test]
    fn test_ipam_allocation() {
        let subnet: Ipv4Network = "10.0.0.0/24".parse().unwrap();
        let ipam = Ipam::new(subnet);

        let app1 = Uuid::new_v4();
        let ip1 = ipam.allocate(app1).unwrap();
        assert_eq!(ip1, Ipv4Addr::new(10, 0, 0, 2)); // .0 is net, .1 is gateway, .2 is first usable

        let app2 = Uuid::new_v4();
        let ip2 = ipam.allocate(app2).unwrap();
        assert_eq!(ip2, Ipv4Addr::new(10, 0, 0, 3));

        // Re-allocation returns same IP
        assert_eq!(ipam.allocate(app1).unwrap(), ip1);

        ipam.release(app1);
        let app3 = Uuid::new_v4();
        let ip3 = ipam.allocate(app3).unwrap();
        assert_eq!(ip3, ip1); // Should reuse released IP
    }

    #[test]
    fn test_ipam_specific_allocation() {
        let subnet: Ipv4Network = "10.0.0.0/24".parse().unwrap();
        let ipam = Ipam::new(subnet);

        let app1 = Uuid::new_v4();
        let requested = Ipv4Addr::new(10, 0, 0, 10);
        let ip1 = ipam.allocate_specific(app1, requested).unwrap();
        assert_eq!(ip1, requested);

        // Allocation from pool should skip 10
        let app2 = Uuid::new_v4();
        let ip2 = ipam.allocate(app2).unwrap();
        assert_eq!(ip2, Ipv4Addr::new(10, 0, 0, 2));
    }
}
