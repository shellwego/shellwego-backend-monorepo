//! Domain entities for the ShellWeGo platform.
//!
//! These structs define the wire format for API requests/responses
//! and the internal state machine representations.

// Always export Model as the main type alias for API use
// When orm feature is enabled, also export Entity/ActiveModel for database operations
#[cfg(feature = "orm")]
pub use app::{Entity as AppEntity, Model as AppModel, Model as App, AppStatus, InstanceStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec, Relation as AppRelation, ActiveModel as AppActiveModel, CreateAppRequest, UpdateAppRequest};

#[cfg(not(feature = "orm"))]
pub use app::{Model as App, AppStatus, InstanceStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec, CreateAppRequest, UpdateAppRequest};

#[cfg(feature = "orm")]
pub use app_instance::{Entity as AppInstanceEntity, Model as AppInstanceModel, Model as AppInstance, Relation as AppInstanceRelation, ActiveModel as AppInstanceActiveModel};

#[cfg(not(feature = "orm"))]
pub use app_instance::Model as AppInstance;

pub use app::{ResourceRequest, parse_memory, parse_cpu};

#[cfg(feature = "orm")]
pub use node::{Entity as NodeEntity, Model as NodeModel, Model as Node, NodeStatus, NodeCapacity, NodeCapabilities, NodeNetwork, Relation as NodeRelation, ActiveModel as NodeActiveModel};

#[cfg(not(feature = "orm"))]
pub use node::{Model as Node, NodeStatus, NodeCapacity, NodeCapabilities, NodeNetwork};

pub use node::{RegisterNodeRequest, NodeJoinResponse};

#[cfg(feature = "orm")]
pub use database::{Entity as DatabaseEntity, Model as DatabaseModel, Model as Database, DatabaseStatus, DatabaseEngine, DatabaseEndpoint, HighAvailability, DatabaseBackupConfig, Relation as DatabaseRelation, ActiveModel as DatabaseActiveModel};

#[cfg(not(feature = "orm"))]
pub use database::{Model as Database, DatabaseStatus, DatabaseEngine, DatabaseEndpoint, HighAvailability, DatabaseBackupConfig};

pub use database::{CreateDatabaseRequest, DatabaseBackup};

#[cfg(feature = "orm")]
pub use volume::{Entity as VolumeEntity, Model as VolumeModel, Model as Volume, VolumeStatus, VolumeType, FilesystemType, Relation as VolumeRelation, ActiveModel as VolumeActiveModel};

#[cfg(not(feature = "orm"))]
pub use volume::{Model as Volume, VolumeStatus, VolumeType, FilesystemType};

pub use volume::{CreateVolumeRequest, Snapshot, Snapshots, BackupPolicy};

#[cfg(feature = "orm")]
pub use domain::{Entity as DomainEntity, Model as DomainModel, Model as Domain, DomainStatus, TlsStatus, Relation as DomainRelation, ActiveModel as DomainActiveModel};

#[cfg(not(feature = "orm"))]
pub use domain::{Model as Domain, DomainStatus, TlsStatus};

pub use domain::{CreateDomainRequest, UploadCertificateRequest, TlsCertificate, DnsValidation, RoutingConfig, EdgeFeatures};

#[cfg(feature = "orm")]
pub use secret::{Entity as SecretEntity, Model as SecretModel, Model as Secret, SecretScope, Relation as SecretRelation, ActiveModel as SecretActiveModel};

#[cfg(not(feature = "orm"))]
pub use secret::{Model as Secret, SecretScope};

pub use secret::{CreateSecretRequest, SecretVersion, SecretVersions, SecretId};

pub mod app;
pub mod app_instance;
pub mod database;
pub mod domain;
pub mod node;
pub mod secret;
pub mod volume;