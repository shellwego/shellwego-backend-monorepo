//! MySQL/MariaDB operator

use crate::operators::{
    DatabaseOperator, DatabaseSpec, ConnectionInfo, InstanceStatus,
    BackupInfo, OperatorError, ResourceSpec,
};

/// MySQL operator
pub struct MysqlOperator {
    // TODO: Similar to PostgresOperator but for MySQL
}

#[async_trait::async_trait]
impl DatabaseOperator for MysqlOperator {
    async fn provision(&self, spec: &DatabaseSpec) -> Result<ConnectionInfo, OperatorError> {
        // TODO: Deploy MySQL with group replication if HA
        unimplemented!("MysqlOperator::provision")
    }

    async fn deprovision(&self, instance_id: &str) -> Result<(), OperatorError> {
        unimplemented!("MysqlOperator::deprovision")
    }

    async fn backup(&self, instance_id: &str) -> Result<BackupInfo, OperatorError> {
        // TODO: Use mysqldump or xtrabackup
        unimplemented!("MysqlOperator::backup")
    }

    async fn restore(&self, instance_id: &str, backup_id: &str) -> Result<(), OperatorError> {
        unimplemented!("MysqlOperator::restore")
    }

    async fn scale(&self, instance_id: &str, resources: &ResourceSpec) -> Result<(), OperatorError> {
        unimplemented!("MysqlOperator::scale")
    }

    async fn get_status(&self, instance_id: &str) -> Result<InstanceStatus, OperatorError> {
        unimplemented!("MysqlOperator::get_status")
    }
}