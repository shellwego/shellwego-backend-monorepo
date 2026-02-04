//! Domain entities for the ShellWeGo platform.
//!
//! These structs define the wire format for API requests/responses
//! and the internal state machine representations.

#[cfg(feature = "orm")]
pub use app::{Entity as AppEntity, Model as AppModel, AppStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec, Relation as AppRelation, ActiveModel as AppActiveModel, InstanceStatus, Relation as AppInstanceRelation, ActiveModel as AppInstanceActiveModel};

#[cfg(not(feature = "orm"))]
pub use app::{App, AppStatus, AppInstance, InstanceStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec};

#[cfg(feature = "orm")]
pub use app::{Entity as AppInstanceEntity, Model as AppInstanceModel};

pub use app::{ResourceRequest, parse_memory, parse_cpu};

#[cfg(feature = "orm")]
pub use node::{Entity as NodeEntity, Model as NodeModel, NodeStatus, NodeCapacity, NodeCapabilities, NodeNetwork, Relation as NodeRelation, ActiveModel as NodeActiveModel};

#[cfg(not(feature = "orm"))]
pub use node::{Node, NodeStatus, NodeCapacity, NodeCapabilities, NodeNetwork};

pub use node::{RegisterNodeRequest, NodeJoinResponse};

#[cfg(feature = "orm")]
pub use database::{Entity as DatabaseEntity, Model as DatabaseModel, DatabaseStatus, DatabaseEngine, DatabaseEndpoint, DatabaseResources, DatabaseUsage, HighAvailability, DatabaseBackupConfig, Relation as DatabaseRelation, ActiveModel as DatabaseActiveModel};

#[cfg(not(feature = "orm"))]
pub use database::{Database, DatabaseStatus, DatabaseEngine, DatabaseEndpoint, DatabaseResources, DatabaseUsage, HighAvailability, DatabaseBackupConfig};

pub use database::{CreateDatabaseRequest, DatabaseBackup};

pub mod app;
pub mod database;
pub mod domain;
pub mod node;
pub mod secret;
pub mod volume;