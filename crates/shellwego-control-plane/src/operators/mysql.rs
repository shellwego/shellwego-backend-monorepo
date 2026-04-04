//! MySQL operator for managed MySQL instances
//!
//! Supports MySQL and MariaDB with HA, backups, and scaling.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use rand::Rng;
use tokio::sync::RwLock;
use tracing::{debug, info};

use super::{
    DatabaseOperator, DatabaseSpec, ConnectionInfo, InstanceStatus, BackupInfo, 
    OperatorError, ResourceSpec, InstancePhase, BackupType, BackupStatus,
    OperatorConfig, SslMode,
};

/// MySQL operator
pub struct MySqlOperator {
    /// Configuration
    config: OperatorConfig,
    /// Running instances
    instances: Arc<RwLock<HashMap<String, MySqlInstance>>>,
    /// Backups
    backups: Arc<RwLock<HashMap<String, Vec<BackupInfo>>>>,
}

/// MySQL instance state
#[derive(Debug, Clone)]
pub struct MySqlInstance {
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
    /// Primary host
    pub primary_host: String,
    /// Replica hosts
    pub replica_hosts: Vec<String>,
}

impl MySqlOperator {
    /// Create new MySQL operator
    pub async fn new(config: &OperatorConfig) -> Result<Self, OperatorError> {
        info!("Initializing MySQL operator");
        
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
    
    /// Get default MySQL port
    fn default_port(&self) -> u16 {
        3306
    }
    
    /// Get MySQL image for version
    fn get_image(&self, version: &str) -> String {
        // Support both MySQL and MariaDB
        if version.starts_with("maria") || version.starts_with("10.") {
            format!("{}/mariadb:{}", self.config.image_registry, version.trim_start_matches("maria"))
        } else {
            format!("{}/mysql:{}", self.config.image_registry, version)
        }
    }
    
    /// Provision instance
    async fn provision_instance(&self, spec: &DatabaseSpec) -> Result<MySqlInstance, OperatorError> {
        let instance_id = spec.instance_id();
        let password = self.generate_password();
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let replica_count = spec.high_availability.as_ref()
            .map(|ha| ha.replica_count)
            .unwrap_or(0);
        
        let primary_host = format!("{}-mysql.{}.svc.cluster.local", instance_id, self.config.default_namespace);
        let replica_hosts: Vec<String> = (0..replica_count)
            .map(|i| format!("{}-mysql-{}.{}.svc.cluster.local", instance_id, i, self.config.default_namespace))
            .collect();
        
        let connection_info = ConnectionInfo {
            host: primary_host.clone(),
            port: self.default_port(),
            username: "root".to_string(),
            password: password.clone(),
            database: "app".to_string(),
            ssl_mode: SslMode::Prefer,
            instance_id: instance_id.clone(),
        };
        
        let instance = MySqlInstance {
            instance_id: instance_id.clone(),
            spec: spec.clone(),
            connection_info,
            phase: InstancePhase::Running,
            ready_replicas: replica_count + 1,
            storage_used: 0,
            created_at: Utc::now(),
            primary_host,
            replica_hosts,
        };
        
        Ok(instance)
    }
}

#[async_trait::async_trait]
impl DatabaseOperator for MySqlOperator {
    async fn provision(&self, spec: &DatabaseSpec) -> Result<ConnectionInfo, OperatorError> {
        let instance_id = spec.instance_id();
        
        info!("Provisioning MySQL instance: {}", instance_id);
        
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
        if spec.resources.cpu_cores > self.config.resource_quotas.max_cpu_cores {
            return Err(OperatorError::InvalidConfig(
                format!("CPU cores {} exceeds maximum", spec.resources.cpu_cores)
            ));
        }
        
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
        
        info!("MySQL instance {} provisioned successfully", instance_id);
        
        Ok(conn_info)
    }
    
    async fn deprovision(&self, instance_id: &str) -> Result<(), OperatorError> {
        info!("Deprovisioning MySQL instance: {}", instance_id);
        
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
        
        info!("MySQL instance {} deprovisioned", instance_id);
        
        Ok(())
    }
    
    async fn backup(&self, instance_id: &str) -> Result<BackupInfo, OperatorError> {
        info!("Creating backup for MySQL instance: {}", instance_id);
        
        {
            let instances = self.instances.read().await;
            if !instances.contains_key(instance_id) {
                return Err(OperatorError::NotFound(instance_id.to_string()));
            }
        }
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Simulate mysqldump
        let backup = BackupInfo {
            id: uuid::Uuid::new_v4().to_string(),
            instance_id: instance_id.to_string(),
            created_at: Utc::now(),
            size_bytes: 1024 * 1024 * 80, // 80MB
            wal_segment_range: None, // MySQL doesn't use WAL like Postgres
            backup_type: BackupType::Full,
            status: BackupStatus::Completed,
        };
        
        {
            let mut backups = self.backups.write().await;
            backups.entry(instance_id.to_string())
                .or_default()
                .push(backup.clone());
        }
        
        info!("Backup {} created for MySQL instance {}", backup.id, instance_id);
        
        Ok(backup)
    }
    
    async fn restore(&self, instance_id: &str, backup_id: &str) -> Result<(), OperatorError> {
        info!("Restoring MySQL instance {} from backup {}", instance_id, backup_id);
        
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
        
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        info!("MySQL restore completed for instance {}", instance_id);
        
        Ok(())
    }
    
    async fn scale(&self, instance_id: &str, resources: &ResourceSpec) -> Result<(), OperatorError> {
        info!("Scaling MySQL instance {}", instance_id);
        
        {
            let instances = self.instances.read().await;
            if !instances.contains_key(instance_id) {
                return Err(OperatorError::NotFound(instance_id.to_string()));
            }
        }
        
        if resources.cpu_cores > self.config.resource_quotas.max_cpu_cores {
            return Err(OperatorError::ScaleFailed("CPU exceeds maximum".to_string()));
        }
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        {
            let mut instances = self.instances.write().await;
            if let Some(instance) = instances.get_mut(instance_id) {
                instance.spec.resources = resources.clone();
            }
        }
        
        info!("MySQL scaling completed for instance {}", instance_id);
        
        Ok(())
    }
    
    async fn get_status(&self, instance_id: &str) -> Result<InstanceStatus, OperatorError> {
        debug!("Getting MySQL instance status: {}", instance_id);
        
        let instances = self.instances.read().await;
        let instance = instances.get(instance_id)
            .ok_or_else(|| OperatorError::NotFound(instance_id.to_string()))?;
        
        let is_healthy = instance.phase == InstancePhase::Running;
        
        Ok(InstanceStatus {
            phase: instance.phase,
            message: if is_healthy { "Healthy".to_string() } else { "Unhealthy".to_string() },
            ready_replicas: instance.ready_replicas,
            total_replicas: instance.spec.high_availability.as_ref()
                .map(|ha| ha.replica_count + 1)
                .unwrap_or(1),
            storage_used: instance.storage_used,
            storage_capacity: (instance.spec.storage_gb as u64) * 1024 * 1024 * 1024,
            instance_id: instance_id.to_string(),
            is_healthy,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mysql_operator_creation() {
        let config = OperatorConfig::default();
        let operator = MySqlOperator::new(&config).await;
        assert!(operator.is_ok());
    }

    #[tokio::test]
    async fn test_mysql_provision() {
        let config = OperatorConfig::default();
        let operator = MySqlOperator::new(&config).await.unwrap();
        
        let spec = DatabaseSpec {
            name: "test-mysql".to_string(),
            engine: super::super::DatabaseEngine::Mysql,
            version: "8.0".to_string(),
            resources: ResourceSpec::default(),
            high_availability: None,
            backup_config: None,
            storage_gb: 10,
            namespace: "default".to_string(),
        };
        
        let result = operator.provision(&spec).await;
        assert!(result.is_ok());
        
        let conn = result.unwrap();
        assert_eq!(conn.port, 3306);
    }
}
