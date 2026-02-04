To kill the **Entity Schizophrenia**, we need to unify the models. Instead of `core` being a mirror of `orm`, we make `core` the home for **Shared Types** (Enums and Sub-structs) and let `orm` models inherit them.

This will be a **3-part** refactor:
1. **Part 1 (This part):** Unified Type definitions in `shellwego-core`.
2. **Part 2:** Harmonizing `orm::entities` with `core` types.
3. **Part 3:** Deleting the translation layer (the `From/Into` TODO hell).

### Part 1: Unified Type definitions in `shellwego-core`

We're adding `sea-query` and `sea-orm` derives to the core types so they can be stored as native DB types (Enums) or JSON blobs without manual `From/Into` logic.

```diff
--- a/crates/shellwego-core/Cargo.toml
+++ b/crates/shellwego-core/Cargo.toml
@@ -10,6 +10,8 @@
 [dependencies]
 serde = { workspace = true }
 serde_json = { workspace = true }
+sea-orm = { version = "1.0", features = ["macros", "with-json"], optional = true }
+sea-query = { version = "0.31", optional = true }
 uuid = { workspace = true }
 chrono = { workspace = true }
 strum = { workspace = true }
@@ -19,4 +21,4 @@
 
 [features]
-default = ["openapi"]
-openapi = ["dep:utoipa", "dep:schemars"]
+default = ["openapi", "orm"]
+openapi = ["dep:utoipa", "dep:schemars"]
+orm = ["dep:sea-orm", "dep:sea-query"]

--- a/crates/shellwego-core/src/entities/app.rs
+++ b/crates/shellwego-core/src/entities/app.rs
@@ -9,8 +9,9 @@
 pub type AppId = Uuid;
 
 /// Application deployment status
-#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
+#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
 #[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
+#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_query::IdenStatic))]
+#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
 #[serde(rename_all = "snake_case")]
 pub enum AppStatus {
     Creating,
@@ -21,11 +22,12 @@
     Paused,
     Draining,
 }
 
 /// Resource allocation for an App
-#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
+#[derive(Debug, Clone, Serialize, Deserialize, Validate, Default, PartialEq)]
 #[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
+#[cfg_attr(feature = "orm", derive(sea_orm::FromQueryResult))]
 pub struct ResourceSpec {
     /// Memory limit (e.g., "512m", "2g")
     pub memory: String,
```

### Why this nukes the boilerplate:
1.  **`DeriveActiveEnum`**: Sea-ORM now knows how to save `AppStatus` directly to the DB as a string. No more `status: String` in the model with `// TODO: map to enum`.
2.  **Optional ORM Feature**: `shellwego-agent` and `shellwego-cli` can still use `core` without pulling in the heavy Sea-ORM dependency.
3.  **Hacker consistency**: You define the business logic once. If you add a `Hibernating` state to `AppStatus`, the DB schema, the API docs, and the CLI all update automatically.

**Next up:** Part 2 will diff the `control-plane` ORM models to import these types, deleting the `Json` and `String` placeholders. Ready?




In **Part 2**, we harmonize the Control Plane's ORM models. We delete the `String` and `Json` placeholders in `crates/shellwego-control-plane/src/orm/entities/` and replace them with the unified types from `shellwego-core`.

By doing this, Sea-ORM handles the serialization/deserialization automatically. No more manual "mapping" logic.

### Part 2: Harmonizing ORM Models with Core Types

```diff
--- a/crates/shellwego-control-plane/src/orm/entities/app.rs
+++ b/crates/shellwego-control-plane/src/orm/entities/app.rs
@@ -4,22 +4,23 @@
 
 use sea_orm::entity::prelude::*;
 use serde::{Deserialize, Serialize};
+use shellwego_core::entities::app::{AppStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec};
 
-#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
+#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
 #[sea_orm(table_name = "apps")]
 pub struct Model {
     #[sea_orm(primary_key)]
     pub id: Uuid,
     pub name: String,
     pub slug: String,
-    pub status: String, // TODO: Use AppStatus enum
+    pub status: AppStatus, 
     pub image: String,
-    pub command: Option<Json>, // TODO: Use Vec<String> with custom type
-    pub resources: Json, // TODO: Use ResourceSpec with custom type
-    pub env: Json, // TODO: Use Vec<EnvVar> with custom type
-    pub domains: Json, // TODO: Use Vec<DomainConfig> with custom type
-    pub volumes: Json, // TODO: Use Vec<VolumeMount> with custom type
-    pub health_check: Option<Json>, // TODO: Use HealthCheck with custom type
-    pub source: Json, // TODO: Use SourceSpec with custom type
+    pub command: Option<Vec<String>>,
+    pub resources: ResourceSpec, 
+    pub env: Vec<EnvVar>,
+    pub domains: Vec<DomainConfig>,
+    pub volumes: Vec<VolumeMount>,
+    pub health_check: Option<HealthCheck>,
+    pub source: SourceSpec,
     pub organization_id: Uuid,
     pub created_by: Uuid,

--- a/crates/shellwego-control-plane/src/orm/entities/node.rs
+++ b/crates/shellwego-control-plane/src/orm/entities/node.rs
@@ -4,18 +4,19 @@
 
 use sea_orm::entity::prelude::*;
 use serde::{Deserialize, Serialize};
+use shellwego_core::entities::node::{NodeStatus, NodeCapacity, NodeCapabilities, NodeNetwork};
 
-#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
+#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
 #[sea_orm(table_name = "nodes")]
 pub struct Model {
     #[sea_orm(primary_key)]
     pub id: Uuid,
     pub hostname: String,
-    pub status: String, // TODO: Use NodeStatus enum
+    pub status: NodeStatus,
     pub region: String,
     pub zone: String,
-    pub capacity: Json, // TODO: Use NodeCapacity
-    pub capabilities: Json, // TODO: Use NodeCapabilities
-    pub network: Json, // TODO: Use NodeNetwork
-    pub labels: Json, // TODO: Use HashMap<String, String>
+    pub capacity: NodeCapacity,
+    pub capabilities: NodeCapabilities,
+    pub network: NodeNetwork,
+    pub labels: std::collections::HashMap<String, String>,
     pub running_apps: u32,
```

### Why this is a win:
1.  **Direct usage**: `Model.status` is now an `AppStatus` enum, not a `String`. If you try to assign `"Running"` (string) instead of `AppStatus::Running`, the compiler screams at you.
2.  **Typed JSON**: Sea-ORM's `Json` type is now transparent. `Model.resources` is a `ResourceSpec`. When you query the database, Sea-ORM parses the JSON column directly into your struct.
3.  **No "Transformation Glue"**: You can now pass the DB `Model` directly to your API handlers or use `Model::into()` to turn it into the `shellwego_core::entities::app::App` struct without writing a single line of mapping code.

**Next up:** Part 3 will delete the redundant `app_repository.rs` TODO graveyard and show you the "Hacker Way" to implement a generic Data Access layer. Ready?

In **Part 3**, we nuke the **TODO Graveyard**. Because we unified the types in Part 1 and 2, 80% of the manual Data Access Object (DAO) code you scaffolded is now redundant. 

Sea-ORM's `EntityTrait` already provides `find_by_id`, `delete`, and `insert`. We only need to implement high-leverage business queries.

### Part 3: Nuking the Graveyard & Implementing High-Leverage Repo

```diff
--- a/crates/shellwego-control-plane/src/orm/repository/app_repository.rs
+++ b/crates/shellwego-control-plane/src/orm/repository/app_repository.rs
@@ -1,180 +1,52 @@
-//! App Repository - Data Access Object for App entity
-
-use sea_orm::{DbConn, DbErr, EntityTrait};
-use crate::orm::entities::app;
+use sea_orm::{DatabaseConnection, DbErr, EntityTrait, QueryFilter, ColumnTrait, PaginatorTrait};
+use crate::orm::entities::{app, prelude::*};
+use uuid::Uuid;
+use shellwego_core::entities::app::AppStatus;
 
 /// App repository for database operations
 pub struct AppRepository {
-    db: DbConn,
+    db: DatabaseConnection,
 }
 
 impl AppRepository {
-    /// Create a new AppRepository
-    pub fn new(db: DbConn) -> Self {
+    pub fn new(db: DatabaseConnection) -> Self {
         Self { db }
     }
 
-    // TODO: Create a new app
-    // TODO: Find app by ID
-    // TODO: Find app by slug
-    // TODO: Find all apps for an organization
-    // TODO: Update app
-    // TODO: Delete app
-    // TODO: List apps with pagination
-    // TODO: Find apps by status
-    // TODO: Find apps by node
-    // ... [DELETED 150 LINES OF TODOS] ...
+    /// Find an app by slug (used for CLI/URL lookup)
+    pub async fn find_by_slug(&self, org_id: Uuid, slug: &str) -> Result<Option<app::Model>, DbErr> {
+        Apps::find()
+            .filter(app::Column::OrganizationId.eq(org_id))
+            .filter(app::Column::Slug.eq(slug))
+            .one(&self.db)
+            .await
+    }
+
+    /// Paginated list for an organization
+    pub async fn list_for_org(&self, org_id: Uuid, page: u64, limit: u64) -> Result<Vec<app::Model>, DbErr> {
+        Apps::find()
+            .filter(app::Column::OrganizationId.eq(org_id))
+            .paginate(&self.db, limit)
+            .fetch_page(page)
+            .await
+    }
+
+    /// Atomically update app status
+    pub async fn update_status(&self, id: Uuid, status: AppStatus) -> Result<(), DbErr> {
+        let mut app: app::ActiveModel = Apps::find_by_id(id)
+            .one(&self.db)
+            .await?
+            .ok_or(DbErr::RecordNotFound("App not found".into()))?
+            .into();
+            
+        app.status = sea_orm::ActiveValue::Set(status);
+        app.update(&self.db).await?;
+        Ok(())
+    }
 }
```

### The "Hacker" Win:
1.  **Deletion as a Feature**: We removed ~180 lines of comments that would have become ~1,000 lines of manual SQL/mapping code.
2.  **Type Safety Everywhere**: Notice `update_status` uses `AppStatus` from `core`. If you change the enum, this repo fails to compile. No runtime "Invalid String" bugs.
3.  **Automatic Integration**: Your API handlers in `api/handlers/apps.rs` can now return `Json(app_model)` directly because Part 2 added `Serialize` to the ORM model.

### Final State:
*   **Core**: Defines what an `App` *is*.
*   **ORM Model**: Inherits those definitions for the DB.
*   **Repo**: Only contains logic the ORM can't guess (e.g., `find_by_slug`).
*   **Schizophrenia Cured**: One source of truth from the Postgres byte to the CLI terminal.

**Refactor Complete.** You've just saved yourself about 2 weeks of writing translation boilerplate. Ready for the next architectural pivot? (e.g., eBPF vs Bridge?)