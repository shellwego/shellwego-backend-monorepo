//! Domain entities for the ShellWeGo platform.
//!
//! These structs define the wire format for API requests/responses
//! and the internal state machine representations.

#[cfg(feature = "orm")]
pub use app::{Entity as AppEntity, Model as AppModel, AppStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec, Relation as AppRelation, ActiveModel as AppActiveModel, InstanceStatus};

#[cfg(not(feature = "orm"))]
pub use app::{Model as App, AppStatus, InstanceStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec};

#[cfg(feature = "orm")]
pub use app_instance::{Entity as AppInstanceEntity, Model as AppInstanceModel, Relation as AppInstanceRelation, ActiveModel as AppInstanceActiveModel};

#[cfg(not(feature = "orm"))]
pub use app_instance::Model as AppInstance;

pub use app::{ResourceRequest, parse_memory, parse_cpu};

#[cfg(feature = "orm")]
pub use node::{Entity as NodeEntity, Model as NodeModel, NodeStatus, NodeCapacity, NodeCapabilities, NodeNetwork, Relation as NodeRelation, ActiveModel as NodeActiveModel};

#[cfg(not(feature = "orm"))]
pub use node::{Node, NodeStatus, NodeCapacity, NodeCapabilities, NodeNetwork};

pub use node::{RegisterNodeRequest, NodeJoinResponse};

#[cfg(feature = "orm")]
pub use database::{Entity as DatabaseEntity, Model as DatabaseModel, DatabaseStatus, DatabaseEngine, DatabaseEndpoint, HighAvailability, DatabaseBackupConfig, Relation as DatabaseRelation, ActiveModel as DatabaseActiveModel};

#[cfg(not(feature = "orm"))]
pub use database::{Model as Database, DatabaseStatus, DatabaseEngine, DatabaseEndpoint, HighAvailability, DatabaseBackupConfig};

pub use database::{CreateDatabaseRequest, DatabaseBackup};

pub mod app;
pub mod app_instance;
pub mod database;
pub mod domain;
pub mod node;
pub mod secret;
pub mod volume;