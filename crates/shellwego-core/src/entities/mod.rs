//! Domain entities for the ShellWeGo platform.
//!
//! These structs define the wire format for API requests/responses
//! and the internal state machine representations.

#[cfg(feature = "orm")]
pub use app::{Entity as AppEntity, Model as AppModel, AppStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec, Relation as AppRelation, ActiveModel as AppActiveModel};

#[cfg(feature = "orm")]
pub use app::{Entity as AppInstanceEntity, Model as AppInstanceModel, InstanceStatus, Relation as AppInstanceRelation, ActiveModel as AppInstanceActiveModel};

#[cfg(not(feature = "orm"))]
pub use app::{App, AppStatus, AppInstance, InstanceStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec};

pub use app::{ResourceRequest, parse_memory, parse_cpu};

pub mod app;
pub mod database;
pub mod domain;
pub mod node;
pub mod secret;
pub mod volume;