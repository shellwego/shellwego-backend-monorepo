//! Redis operator for managed Redis instances
//!
//! Supports standalone Redis, Redis Sentinel, and Redis Cluster
//! with persistence and high availability.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use rand::Rng;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::{
    DatabaseOperator, DatabaseSpec, ConnectionInfo, InstanceStatus, BackupInfo, 
    OperatorError, ResourceSpec, InstancePhase, BackupType, BackupStatus,
    OperatorConfig, SslMode,
};

/// Redis operator
pub struct RedisOperator {
    /// Configuration
    config: OperatorConfig,
    /// Running instances
    instances: Arc<RwLock<HashMap<String, RedisInstance>>>,
    /// Backups
    backups: Arc<RwLock<HashMap<String, Vec<BackupInfo>>>>,
}

/// Redis instance state
#[derive(Debug, Clone)]
pub struct RedisInstance {
    /// Instance ID
    pub instance_id: String,
    /// Specification
    pub spec: DatabaseSpec,
    /// Connection info
    pub connection_info: ConnectionInfo,
    /// Current phase
    pub phase: InstancePhase,
    /// Ready replicas
    pub ready_replicas: u32,
    /// Storage used
    pub storage_used: u64,
    /// Created at
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Master host
    pub master_host: String,
    /// Replica hosts
    pub replica_hosts: Vec<String>,
    /// Sentinel hosts
    pub sentinel_hosts: Vec<String>,
    /// Redis mode
    pub mode: RedisMode,
}

/// Redis deployment mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedisMode {
    /// Single instance
    Standalone,
    /// Master-replica with Sentinel
    Sentinel,
    /// Redis Cluster
    Cluster,
}

impl RedisOperator {
    /// Create new Redis operator
    pub async fn new(config: &OperatorConfig) -> Result<Self, OperatorError> {
        info!("Initializing Redis operator");
        
        Ok(Self {
            config: config.clone(),
            instances: Arc::new(RwLock::new(HashMap::new())),
            backups: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    /// Generate password
    fn generate_password(&self) -> String {
        const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut rng = rand::thread_rng();
        
        (0..32)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }
    
    /// Get default Redis port
    fn default_port(&self) -> u16 {
        6379
    }
    
    /// Get Sentinel port
    fn sentinel_port(&self) -> u16 {
        26379
    }
    
    /// Get Redis image for version
    fn get_image(&self, version: &str) -> String {
        format!("{}/redis:{}", self.config.image_registry, version)
    }
    
    /// Determine deployment mode from spec
    fn determine_mode(&self, spec: &DatabaseSpec) -> RedisMode {
        if spec.high_availability.is_some() {
            // For Redis, we use Sentinel for HA
            RedisMode::Sentinel
        } else {
            RedisMode::Standalone
        }
    }
    
    /// Provision instance
    async fn provision_instance(&self, spec: &DatabaseSpec) -> Result<RedisInstance, OperatorError> {
        let instance_id = spec.instance_id();
        let password = self.generate_password();
        let mode = self.determine_mode(spec);
        
        // Simulate provisioning delay
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let master_host = format!("{}-redis.{}.svc.cluster.local", instance_id, self.config.default_namespace);
        
        let (replica_hosts, sentinel_hosts, ready_replicas) = match mode {
            RedisMode::Standalone => (vec![], vec![], 1),
            RedisMode::Sentinel => {
                let replica_count = spec.high_availability.as_ref()
                    .map(|ha| ha.replica_count)
                    .unwrap_or(2);
                
                let replicas: Vec<String> = (0..replica_count)
                    .map(|i| format!("{}-redis-replica-{}.{}.svc.cluster.local", 
                        instance_id, i, self.config.default_namespace))
                    .collect();
                
                let sentinels: Vec<String> = (0..3)
                    .map(|i| format!("{}-sentinel-{}.{}.svc.cluster.local", 
                        instance_id, i, self.config.default_namespace))
                    .collect();
                
                (replicas, sentinels, replica_count + 1)
            }
            RedisMode::Cluster => {
                // Redis Cluster has 6 nodes minimum (3 masters, 3 replicas)
                let nodes: Vec<String> = (0..6)
                    .map(|i| format!("{}-redis-{}.{}.svc.cluster.local", 
                        instance_id, i, self.config.default_namespace))
                    .collect();
                
                (nodes[1..].to_vec(), vec![], 6)
            }
        };
        
        let connection_info = ConnectionInfo {
            host: master_host.clone(),
            port: self.default_port(),
            username: "default".to_string(), // Redis 6+ ACL
            password: password.clone(),
            database: "0".to_string(), // Redis database number as string
            ssl_mode: SslMode::Prefer,
            instance_id: instance_id.clone(),
        };
        
        Ok(RedisInstance {
            instance_id: instance_id.clone(),
            spec: spec.clone(),
            connection_info,
            phase: InstancePhase::Running,
            ready_replicas,
            storage_used: 0,
            created_at: Utc::now(),
            master_host,
            replica_hosts,
            sentinel_hosts,
            mode,
        })
    }
}

#[async_trait::async_trait]
impl DatabaseOperator for RedisOperator {
    async fn provision(&self, spec: &DatabaseSpec) -> Result<ConnectionInfo, OperatorError> {
        let instance_id = spec.instance_id();
        
        info!("Provisioning Redis instance: {} (mode: {:?})", instance_id, self.determine_mode(spec));
        
        // Check if instance already exists
        {
            let instances = self.instances.read().await;
            if instances.contains_key(&instance_id) {
                return Err(OperatorError::ProvisionFailed(
                    format!("Instance {} already exists", instance_id)
                ));
            }
        }
        
        // Validate resources
        if spec.resources.memory_gb > self.config.resource_quotas.max_memory_gb {
            return Err(OperatorError::InvalidConfig(
                format!("Memory {}GB exceeds maximum", spec.resources.memory_gb)
            ));
        }
        
        let instance = self.provision_instance(spec).await?;
        let conn_info = instance.connection_info.clone();
        
        {
            let mut instances = self.instances.write().await;
            instances.insert(instance_id.clone(), instance);
        }
        
        info!("Redis instance {} provisioned successfully", instance_id);
        
        Ok(conn_info)
    }
    
    async fn deprovision(&self, instance_id: &str) -> Result<(), OperatorError> {
        info!("Deprovisioning Redis instance: {}", instance_id);
        
        {
            let instances = self.instances.read().await;
            if !instances.contains_key(instance_id) {
                return Err(OperatorError::NotFound(instance_id.to_string()));
            }
        }
        
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        {
            let mut instances = self.instances.write().await;
            instances.remove(instance_id);
        }
        
        {
            let mut backups = self.backups.write().await;
            backups.remove(instance_id);
        }
        
        info!("Redis instance {} deprovisioned", instance_id);
        
        Ok(())
    }
    
    async fn backup(&self, instance_id: &str) -> Result<BackupInfo, OperatorError> {
        info!("Creating RDB backup for Redis instance: {}", instance_id);
        
        {
            let instances = self.instances.read().await;
            if !instances.contains_key(instance_id) {
                return Err(OperatorError::NotFound(instance_id.to_string()));
            }
        }
        
        // Simulate BGSAVE
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        let backup = BackupInfo {
            id: uuid::Uuid::new_v4().to_string(),
            instance_id: instance_id.to_string(),
            created_at: Utc::now(),
            size_bytes: 1024 * 1024 * 50, // 50MB simulated RDB
            wal_segment_range: None,
            backup_type: BackupType::Full,
            status: BackupStatus::Completed,
        };
        
        {
            let mut backups = self.backups.write().await;
            backups.entry(instance_id.to_string())
                .or_default()
                .push(backup.clone());
        }
        
        info!("Redis backup {} created", backup.id);
        
        Ok(backup)
    }
    
    async fn restore(&self, instance_id: &str, backup_id: &str) -> Result<(), OperatorError> {
        info!("Restoring Redis instance {} from backup {}", instance_id, backup_id);
        
        {
            let instances = self.instances.read().await;
            if !instances.contains_key(instance_id) {
                return Err(OperatorError::NotFound(instance_id.to_string()));
            }
        }
        
        {
            let backups = self.backups.read().await;
            let instance_backups = backups.get(instance_id)
                .ok_or_else(|| OperatorError::RestoreFailed("No backups found".to_string()))?;
            
            if !instance_backups.iter().any(|b| b.id == backup_id) {
                return Err(OperatorError::RestoreFailed(
                    format!("Backup {} not found", backup_id)
                ));
            }
        }
        
        // Simulate RDB restore
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        info!("Redis restore completed for instance {}", instance_id);
        
        Ok(())
    }
    
    async fn scale(&self, instance_id: &str, resources: &ResourceSpec) -> Result<(), OperatorError> {
        info!("Scaling Redis instance {}", instance_id);
        
        {
            let instances = self.instances.read().await;
            if !instances.contains_key(instance_id) {
                return Err(OperatorError::NotFound(instance_id.to_string()));
            }
        }
        
        if resources.memory_gb > self.config.resource_quotas.max_memory_gb {
            return Err(OperatorError::ScaleFailed("Memory exceeds maximum".to_string()));
        }
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        {
            let mut instances = self.instances.write().await;
            if let Some(instance) = instances.get_mut(instance_id) {
                instance.spec.resources = resources.clone();
                
                // Update maxmemory config (would be applied to actual Redis)
                let maxmemory = format!("{}gb", resources.memory_gb);
                debug!("Setting maxmemory to {}", maxmemory);
            }
        }
        
        info!("Redis scaling completed for instance {}", instance_id);
        
        Ok(())
    }
    
    async fn get_status(&self, instance_id: &str) -> Result<InstanceStatus, OperatorError> {
        debug!("Getting Redis instance status: {}", instance_id);
        
        let instances = self.instances.read().await;
        let instance = instances.get(instance_id)
            .ok_or_else(|| OperatorError::NotFound(instance_id.to_string()))?;
        
        let is_healthy = instance.phase == InstancePhase::Running;
        
        Ok(InstanceStatus {
            phase: instance.phase,
            message: if is_healthy { "Healthy".to_string() } else { "Unhealthy".to_string() },
            ready_replicas: instance.ready_replicas,
            total_replicas: instance.ready_replicas,
            storage_used: instance.storage_used,
            storage_capacity: (instance.spec.storage_gb as u64) * 1024 * 1024 * 1024,
            instance_id: instance_id.to_string(),
            is_healthy,
        })
    }
}

impl RedisOperator {
    /// List all Redis instances
    pub async fn list_instances(&self) -> Vec<String> {
        let instances = self.instances.read().await;
        instances.keys().cloned().collect()
    }
    
    /// Get instance details
    pub async fn get_instance(&self, instance_id: &str) -> Option<RedisInstance> {
        let instances = self.instances.read().await;
        instances.get(instance_id).cloned()
    }
    
    /// Execute Redis command (simulated)
    pub async fn execute_command(&self, _instance_id: &str, _command: &str) -> Result<String, OperatorError> {
        Ok("OK".to_string())
    }
    
    /// Get connection string for Sentinel
    pub async fn get_sentinel_connection(&self, instance_id: &str) -> Result<String, OperatorError> {
        let instances = self.instances.read().await;
        let instance = instances.get(instance_id)
            .ok_or_else(|| OperatorError::NotFound(instance_id.to_string()))?;
        
        if instance.mode != RedisMode::Sentinel {
            return Err(OperatorError::InvalidConfig(
                "Instance is not in Sentinel mode".to_string()
            ));
        }
        
        // Return sentinel connection string
        let sentinel_addr = instance.sentinel_hosts.join(",");
        Ok(format!("redis+sentinel://{}?master_name=mymaster", sentinel_addr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_redis_operator_creation() {
        let config = OperatorConfig::default();
        let operator = RedisOperator::new(&config).await;
        assert!(operator.is_ok());
    }

    #[tokio::test]
    async fn test_redis_provision() {
        let config = OperatorConfig::default();
        let operator = RedisOperator::new(&config).await.unwrap();
        
        let spec = DatabaseSpec {
            name: "test-redis".to_string(),
            engine: super::super::DatabaseEngine::Redis,
            version: "7.0".to_string(),
            resources: ResourceSpec::default(),
            high_availability: None,
            backup_config: None,
            storage_gb: 5,
            namespace: "default".to_string(),
        };
        
        let result = operator.provision(&spec).await;
        assert!(result.is_ok());
        
        let conn = result.unwrap();
        assert_eq!(conn.port, 6379);
    }

    #[tokio::test]
    async fn test_redis_sentinel_mode() {
        let config = OperatorConfig::default();
        let operator = RedisOperator::new(&config).await.unwrap();
        
        let spec = DatabaseSpec {
            name: "test-redis-ha".to_string(),
            engine: super::super::DatabaseEngine::Redis,
            version: "7.0".to_string(),
            resources: ResourceSpec::default(),
            high_availability: Some(super::super::HaConfig::default()),
            backup_config: None,
            storage_gb: 5,
            namespace: "default".to_string(),
        };
        
        let result = operator.provision(&spec).await.unwrap();
        
        let instance = operator.get_instance(&spec.instance_id()).await.unwrap();
        assert_eq!(instance.mode, RedisMode::Sentinel);
        assert!(!instance.sentinel_hosts.is_empty());
    }
}
