# ShellWeGo Backend Monorepo — Production Remediation Plan

> **Purpose**: Fix every misalignment, fake implementation, stub, and placeholder identified in the
> audit so the codebase matches what README.md claims: a real, deployable sovereign cloud platform.
>
> **Format**: Each phase is a self-contained work unit delegatable to **one AI agent**.
> Phases are ordered by dependency. An agent working on Phase N may assume Phase N-1 is complete.
>
> **Audit baseline** (as of 2026-04-04):
> - 11 crates total
> - 5 real: `shellwego-schema`, `shellwego-observability`, `shellwego-storage`, `shellwego-registry`, `shellwego-network`
> - 2 mostly real: `shellwego-edge` (ACME/WS stubbed), `shellwego-firecracker` (thin re-export layer)
> - 2 mixed: `shellwego-billing` (real metering, fake payments), `shellwego-agent` (real structure, scaffolded VMM/WASM)
> - 1 heavily faked: `shellwego-control-plane` (fake ORM, fake auth, fake KMS, all handlers return 404)
> - 1 real but no-op: `shellwego-cli` (real code, but talks to fake control-plane)
> - Missing: `install.sh`, `LICENSE`

---

## Phase 1 — Database & Persistence Layer

**Agent scope**: Replace the entirely fake ORM with a real database layer.

### What README claims
- "SQLite (single node) / Postgres (HA)" as state store
- "ACID compliance for scheduler state"
- `shellwego init --database=postgres://user:pass@localhost/shellwego`

### What actually exists
- `crates/shellwego-control-plane/src/orm/mod.rs` — a `Database` struct backed by `RwLock<bool>`
- Every method (`insert`, `update`, `delete`, `find_by_id`, `find_all`, `query`, `count`) is a no-op that returns `Ok(())`, `Ok(None)`, or `Ok(vec![])`
- `migrate()` logs migration names but runs zero SQL
- Comment in source: `"Simulated connection"` and `"In production: sqlx::migrate!().run(&self.pool).await"`
- Entity types (`App`, `Node`, `Volume`, `Secret`, `Organization`) defined inline but never stored
- No migration files exist anywhere in the repo (`migrations/` directory is empty/absent)
- `sqlx` is already in workspace dependencies with `postgres` + `sqlite` features

### Specific issues to fix
1. **CRITICAL**: `Database` struct is `RwLock<bool>` — no connection pool, no queries, no data
2. **CRITICAL**: Zero SQL migration files exist
3. **CRITICAL**: All CRUD methods are no-ops
4. **HIGH**: Entity types duplicated between `orm/mod.rs` and `shellwego-schema` — unify on schema crate
5. **HIGH**: `transaction()` wrapper is fake (just calls the closure, no rollback support)
6. **MEDIUM**: No connection pooling configuration is functional
7. **MEDIUM**: No database URL parsing/validation (SQLite vs Postgres detection)

### What the agent must do
1. Create `migrations/` directory at repo root with numbered SQL files:
   - `001_initial_schema.sql` — organizations, users, apps, nodes, deployments, volumes, domains, databases, secrets, audit_logs
   - `002_add_indexes.sql` — performance indexes for common query patterns
   - `003_add_billing_tables.sql` — customers, invoices, payments, usage_meters (for Phase 5)
   - `004_add_agent_state.sql` — agent_heartbeats, task_assignments
   - All migrations must work for **both** SQLite and Postgres (use `sqlx` conditional syntax or provide two variants)
2. Rewrite `crates/shellwego-control-plane/src/orm/mod.rs`:
   - Replace `Database` with a struct wrapping `sqlx::AnyPool` (or `SqlitePool`/`PgPool` behind an enum)
   - Implement real `insert()`, `update()`, `delete()`, `find_by_id()`, `find_all()`, `query()`, `count()`
   - Implement real `transaction()` with actual rollback on error
   - Use `sqlx::query!()` or `sqlx::query_as!()` with compile-time checking
   - Use entity types from `shellwego-schema` instead of local duplicates
3. Update `DatabaseConfig` to properly configure pool sizes, timeouts, and auto-migrate flag
4. Wire the real pool into `AppState` in `state.rs`
5. Ensure `main.rs` creates the pool on startup and runs migrations if `auto_migrate` is true

### Files to modify
- `crates/shellwego-control-plane/src/orm/mod.rs` — full rewrite
- `crates/shellwego-control-plane/src/state.rs` — use real pool
- `crates/shellwego-control-plane/src/main.rs` — pool initialization
- `crates/shellwego-control-plane/src/config.rs` — DB config wiring
- `crates/shellwego-control-plane/Cargo.toml` — ensure `sqlx` features correct
- `migrations/` — create all migration SQL files

### Files to reference (read-only)
- `crates/shellwego-schema/src/lib.rs` — canonical entity type definitions
- `Cargo.toml` — workspace sqlx dependency config
- `readme.md` — database requirements section

### Acceptance criteria
- [ ] `cargo check -p shellwego-control-plane` passes
- [ ] `Database::new(config)` actually connects to a SQLite file
- [ ] `db.insert("apps", &app)` writes a row that `db.find_by_id("apps", &id)` retrieves
- [ ] `db.migrate()` runs all SQL migration files
- [ ] `db.transaction()` rolls back on error
- [ ] All entity types come from `shellwego-schema`, not local duplicates
- [ ] At least 10 unit tests pass with an in-memory SQLite database

### Dependencies
- None (first phase, foundation for everything else)

---

## Phase 2 — Authentication & Authorization

**Agent scope**: Replace the fake UUID-as-JWT auth with real cryptographic authentication.

### What README claims
- "JWT with RS256 (asymmetric), 15min expiry"
- "RBAC with resource-level permissions: `apps:read:uuid`, `nodes:write:*`"
- "Token bucket per API key (configurable per tenant)"
- "Audit Logs: Every mutation stored immutably (append-only log)"

### What actually exists
- `crates/shellwego-control-plane/src/api/handlers.rs` (auth section):
  - `create_token()` generates `Uuid::new_v4().to_string()` — a random UUID, not a JWT
  - No signing, no expiry, no audience/issuer claims
  - Password is not verified — any username/password pair creates a valid token
  - Token validation just checks `!token.is_empty()` — any non-empty string is accepted
- No RBAC implementation exists beyond commented-out permission strings
- No audit log table or service
- No refresh token mechanism
- No rate limiting middleware

### Specific issues to fix
1. **CRITICAL**: Auth tokens are raw UUIDs with zero cryptographic verification
2. **CRITICAL**: No password hashing — passwords are accepted but never stored or checked
3. **CRITICAL**: No token expiry — tokens last forever
4. **HIGH**: No RBAC — all authenticated users have full access to everything
5. **HIGH**: No audit logging
6. **MEDIUM**: No rate limiting
7. **LOW**: No refresh token rotation

### What the agent must do
1. Add dependencies to `shellwego-control-plane/Cargo.toml`:
   - `jsonwebtoken` (JWT encoding/decoding with RS256)
   - `argon2` (password hashing)
   - `tower-governor` or custom rate limiter
2. Create `crates/shellwego-control-plane/src/auth/` module:
   - `mod.rs` — public interface
   - `jwt.rs` — JWT generation with RS256, claims struct (sub, exp, iat, iss, aud, permissions), validation, refresh token support (15min access, 7d refresh)
   - `password.rs` — argon2id hash and verify functions
   - `rbac.rs` — permission enum/mask system: `apps:read`, `apps:write`, `nodes:read`, `nodes:write`, `admin:*`, etc. Middleware that extracts permissions from JWT and checks against required scope
   - `rate_limit.rs` — token-bucket rate limiter per API key/user
3. Rewrite auth handlers:
   - `POST /v1/auth/register` — hash password, store user in DB (Phase 1), return JWT
   - `POST /v1/auth/login` — verify password against hash, issue JWT pair (access + refresh)
   - `POST /v1/auth/refresh` — exchange refresh token for new access token
   - `DELETE /v1/auth/token` — revoke token (add to blocklist)
4. Add `auth_middleware` to Axum router that:
   - Extracts Bearer token from Authorization header
   - Validates JWT signature and expiry
   - Extracts user_id and permissions
   - Injects `CurrentUser` into request extensions
5. Create audit log service:
   - `POST /v1/audit-logs` endpoint (internal, called by other handlers)
   - Append-only table: `audit_logs (id, user_id, action, resource_type, resource_id, ip_address, timestamp, details_json)`
   - Every mutation handler (create, update, delete) must write an audit log entry

### Files to modify
- `crates/shellwego-control-plane/Cargo.toml` — add deps
- `crates/shellwego-control-plane/src/api/handlers.rs` — rewrite auth section
- `crates/shellwego-control-plane/src/api/middleware.rs` — add auth + RBAC + rate limit
- `crates/shellwego-control-plane/src/api/mod.rs` — wire middleware
- `crates/shellwego-control-plane/src/state.rs` — add JWT signing key to AppState
- `crates/shellwego-control-plane/src/config.rs` — JWT config (secret key path, expiry, issuer)
- `migrations/` — add users table, audit_logs table, token_blocklist table

### Files to reference (read-only)
- `crates/shellwego-schema/src/api.rs` — existing request/response types
- `readme.md` — security model section

### Acceptance criteria
- [ ] `create_token()` returns a real RS256-signed JWT, not a UUID
- [ ] JWT contains `exp` claim and is rejected after expiry
- [ ] Password is hashed with argon2id before storage
- [ ] Login with wrong password returns 401
- [ ] RBAC middleware blocks `apps:read` request from user with only `nodes:read`
- [ ] Every create/update/delete API call writes an audit log entry
- [ ] Rate limiter returns 429 after configured threshold
- [ ] All existing tests still pass + at least 5 new auth tests

### Dependencies
- Phase 1 (needs real DB for user/password storage)

---

## Phase 3 — KMS & Secrets Encryption

**Agent scope**: Replace the fake base64 "encryption" with real AES-256-GCM cryptography.

### What README claims
- "Encryption at rest" with AES-256-GCM
- Nonce per encryption, key_id reference to KMS/master key
- Master key options: HashiCorp Vault, AWS KMS, GCP KMS, Azure Key Vault, File-based (dev)
- "Secrets injected via tmpfs (RAM-only, never touch disk)"
- "Rotated automatically"

### What actually exists
- `crates/shellwego-control-plane/src/kms/mod.rs`:
  - All 5 backends (Vault, AWS, GCP, Azure, File) use `BASE64.encode(format!("prefix:{plaintext}"))`
  - "Decryption" does `trim_start_matches("prefix:")` — this is **zero actual cryptography**
  - Plaintext is stored as-is inside the base64 output, trivially reversible by anyone
  - Secrets stored in `RwLock<HashMap>` in memory, lost on restart
  - Key rotation increments a version counter but `reencrypt_all_secrets()` is an empty loop
  - Comment: `"Simplified encryption - in production use proper AES-GCM"`
- `crates/shellwego-storage/src/encryption.rs` — **already has a correct AES-256-GCM implementation** using `aes-gcm` crate with proper DEK/KEK envelope encryption. This is the reference implementation to copy from.

### Specific issues to fix
1. **CRITICAL**: KMS "encryption" is base64 encoding — no actual cryptography whatsoever
2. **CRITICAL**: Anyone who reads stored ciphertext can decode it trivially
3. **HIGH**: Secrets stored in memory only (RwLock<HashMap>) — lost on restart
4. **HIGH**: Key rotation is a no-op — version counter increments but nothing is re-encrypted
5. **HIGH**: No backend actually calls external services (Vault API, AWS KMS API, etc.)
6. **MEDIUM**: The correct implementation already exists in `shellwego-storage/src/encryption.rs` but is not used

### What the agent must do
1. Delete the entire fake KMS implementation in `crates/shellwego-control-plane/src/kms/mod.rs`
2. Create a new KMS module that delegates to `shellwego-storage::encryption` for the File backend:
   - Reuse `Aes256GcmEncryptor` from `shellwego-storage/src/encryption.rs` as the local encryption engine
   - Implement envelope encryption: generate DEK per secret, encrypt DEK with KEK (master key)
   - Store encrypted DEK alongside ciphertext and nonce in the database (Phase 1)
3. Implement real Vault backend:
   - Use `vaultrs` crate (or raw HTTP client) to call `POST /v1/transit/encrypt/{key_name}` and `POST /v1/transit/decrypt/{key_name}`
   - Read Vault address and token from `KmsConfig::Vault`
4. Implement real AWS KMS backend:
   - Use `aws-sdk-kms` to call `Encrypt` and `Decrypt` API
   - Read region and credentials from `KmsConfig::AwsKms`
5. For GCP and Azure: implement real API calls using their respective SDKs (or mark as "future" with a clear compile-time error if SDK not configured)
6. Rewrite key rotation:
   - Generate new KEK
   - Re-encrypt all stored DEKs with new KEK
   - Update version counter
   - Keep old KEK version for decryption of not-yet-rotated secrets
7. Move secret storage from `RwLock<HashMap>` to database (Phase 1 `secrets` table)
8. Ensure all API handlers that create/read secrets use the real KMS encrypt/decrypt

### Files to modify
- `crates/shellwego-control-plane/src/kms/mod.rs` — full rewrite
- `crates/shellwego-control-plane/Cargo.toml` — add `vaultrs`, `aws-sdk-kms` (optional)
- `crates/shellwego-control-plane/src/api/handlers.rs` — wire real KMS into secret CRUD
- `crates/shellwego-control-plane/src/state.rs` — KmsClient initialization

### Files to reference (read-only)
- `crates/shellwego-storage/src/encryption.rs` — **reference AES-256-GCM implementation**
- `readme.md` — security model / secrets management section

### Acceptance criteria
- [ ] `kms.encrypt("key", "secret")` produces ciphertext that is NOT reversible by base64 decode alone
- [ ] `kms.decrypt("key")` returns the original plaintext
- [ ] Encrypting "hello" and "world" produces different ciphertexts (random nonce)
- [ ] File backend uses `aes-gcm` crate (verify in Cargo.lock)
- [ ] Secrets are persisted to database, not lost on restart
- [ ] Key rotation actually re-encrypts all stored secrets
- [ ] Vault backend calls real Vault transit API (mockable for tests)
- [ ] All existing tests pass + at least 5 new KMS tests including roundtrip, rotation, wrong-key failure

### Dependencies
- Phase 1 (needs real DB for secret persistence)

---

## Phase 4 — API Handlers: Make Everything Work

**Agent scope**: Replace all 404-returning stub handlers with real database-backed implementations.

### What README claims
- Full REST API: apps, nodes, volumes, domains, databases, secrets, organizations, builds, deployments
- "Git-based deployment (push to deploy)"
- "Web-based log streaming (WebSocket)"
- "REST API + WebSocket real-time events"

### What actually exists
- `crates/shellwego-control-plane/src/api/handlers.rs`:
  - `list_apps()` returns `PaginatedResponse::empty()`
  - `get_app()`, `get_node()`, `get_volume()`, `get_domain()`, `get_database()`, `get_secret()`, `get_organization()`, `get_build()` — **all return 404 unconditionally**
  - `create_app/volume/domain/database/organization` — create objects in local variables that are **immediately discarded** (never stored)
  - `delete_app/volume/domain/database/secret` — all return 404
  - No WebSocket endpoint for log streaming
  - No Git webhook handler that triggers builds
- The Axum router structure exists and is well-organized — the routing is correct, just the handlers are empty

### Specific issues to fix
1. **CRITICAL**: Every GET handler returns 404 — no data ever returned
2. **CRITICAL**: Every POST handler discards the created entity — nothing is persisted
3. **CRITICAL**: Every DELETE handler returns 404 — nothing can be deleted
4. **HIGH**: No list/pagination works — all lists return empty
5. **HIGH**: No WebSocket endpoint for log streaming
6. **HIGH**: No build pipeline triggered by Git webhooks
7. **MEDIUM**: No input validation (request bodies accepted but ignored)

### What the agent must do
1. Rewrite all CRUD handlers to use the real Database from Phase 1:
   - `list_apps` → `SELECT * FROM apps WHERE organization_id = ? LIMIT ? OFFSET ?`
   - `get_app` → `SELECT * FROM apps WHERE id = ?` (return 404 only if actually missing)
   - `create_app` → `INSERT INTO apps (...) VALUES (...)` (return 201 with created entity)
   - `update_app` → `UPDATE apps SET ... WHERE id = ?` (return updated entity)
   - `delete_app` → `DELETE FROM apps WHERE id = ?` (return 204)
   - Repeat for: nodes, volumes, domains, databases, secrets, organizations, builds
2. Implement proper pagination:
   - `PaginatedResponse` with `total_count`, `page`, `per_page`, `data`
   - Default `per_page=20`, max `per_page=100`
   - Cursor-based or offset-based (offset is fine for now)
3. Add input validation using `validator` crate:
   - App names: 3-63 chars, alphanumeric + hyphens
   - Domain names: valid FQDN format
   - Volume sizes: 1GB-10TB range
   - All validated before hitting the database
4. Implement WebSocket log streaming:
   - `GET /v1/apps/{id}/logs` — upgrade to WebSocket
   - Subscribe to agent log stream via QUIC (Phase 6 dependency — stub for now, mark with `TODO`)
5. Implement Git webhook handler:
   - `POST /v1/webhooks/git` — parse GitHub/GitLab webhook payload
   - Create a Build record, trigger build pipeline
   - For now: store the webhook payload and create a `Build` record in `pending` status

### Files to modify
- `crates/shellwego-control-plane/src/api/handlers.rs` — full rewrite of all handlers
- `crates/shellwego-control-plane/src/api/response.rs` — proper error responses
- `crates/shellwego-control-plane/src/api/mod.rs` — add WebSocket route, webhook route
- `crates/shellwego-control-plane/src/git/` — webhook handler implementation
- `migrations/` — add builds table if not already present

### Files to reference (read-only)
- `crates/shellwego-schema/src/` — all entity types, request/response DTOs
- `crates/shellwego-control-plane/src/orm/mod.rs` — real Database (Phase 1 output)
- `readme.md` — API example section, REST endpoints

### Acceptance criteria
- [ ] `POST /v1/apps` creates an app that `GET /v1/apps/{id}` returns
- [ ] `GET /v1/apps` returns a paginated list of all created apps
- [ ] `DELETE /v1/apps/{id}` removes the app (subsequent GET returns 404)
- [ ] All entity types (nodes, volumes, domains, databases, secrets, orgs) have working CRUD
- [ ] Pagination works: `?page=2&per_page=5` returns correct slice
- [ ] Invalid input (empty name, negative size) returns 400 with error details
- [ ] `POST /v1/webhooks/git` stores webhook and creates Build record
- [ ] At least 15 integration tests covering CRUD for each entity type

### Dependencies
- Phase 1 (needs real DB)
- Phase 2 (auth middleware must protect these endpoints)
- Phase 3 (KMS needed for secret encryption/decryption in handlers)

---

## Phase 5 — Billing & Payments

**Agent scope**: Replace fake payment processing and fake storage with real Stripe integration and database persistence.

### What README claims
- "Pricing Strategy Playbook" with per-plan pricing
- Payment integration: Stripe, Paystack, Flutterwave, M-Pesa, GCash, UPI, MercadoPago, USDC, BTC Lightning
- "Commercial License" tiers: $99/mo, $299/mo, $999/mo
- Revenue share model (5%)

### What actually exists
- `crates/shellwego-billing/src/lib.rs` (~830 lines):
  - Customers stored in `RwLock<HashMap>` — lost on restart
  - `get_customer()` always returns `CustomerNotFound`
  - `get_invoice()` always returns `InvoiceNotFound`
  - `process_card_payment()`, `process_bank_transfer()`, `process_wallet_payment()`, `process_crypto_payment()` — all return `PaymentResult { success: true }` with random UUID. No actual API calls.
  - `verify_stripe_webhook()` checks `!signature.is_empty()` — string-non-empty check
  - `verify_paystack_webhook()` — identical fake check
  - `retry_failed_payments()` — body is `let _ = billing; Ok(())`
  - `store_invoice()` — just logs, `mark_invoice_paid()` — just logs
- `crates/shellwego-billing/src/metering.rs` (~663 lines):
  - **Real**: DashMap-based buffer, real PostgreSQL schema, batch flush, time-series aggregation
  - This is the only genuine part of billing
- `crates/shellwego-billing/src/invoices.rs` (~1052 lines):
  - **Real**: Tera template engine with embedded HTML invoice template, correct proration math, real `rust_decimal`
  - But invoice storage is a no-op

### Specific issues to fix
1. **CRITICAL**: All payment methods return `success: true` with fake transaction IDs
2. **CRITICAL**: Customer data stored in `RwLock<HashMap>` — not persisted
3. **CRITICAL**: Invoice storage is a no-op (just logs)
4. **HIGH**: Webhook signature verification is `!signature.is_empty()` — accepts any input
5. **HIGH**: No actual Stripe/Paystack API integration
6. **MEDIUM**: Pricing is hardcoded as literal floats in `get_pricing()`
7. **MEDIUM**: `retry_failed_payments()` is empty

### What the agent must do
1. Add real Stripe integration:
   - Add `stripe` crate to `Cargo.toml`
   - Implement `process_card_payment()` using `stripe::PaymentIntent` API
   - Implement `process_subscription()` using `stripe::Subscription` API
   - Implement `verify_stripe_webhook()` using HMAC-SHA256 signature verification with Stripe's signing secret
2. Replace customer storage with database:
   - Use Phase 1's `Database` or direct `sqlx` queries
   - Create `billing_customers`, `invoices`, `payments`, `subscriptions` tables (already partially defined in metering.rs PostgreSQL schema)
3. Implement real invoice persistence:
   - `store_invoice()` → INSERT into invoices table
   - `mark_invoice_paid()` → UPDATE invoice status + record payment
   - `get_invoice()` → SELECT from invoices table
4. Implement real retry logic:
   - `retry_failed_payments()` → query failed payments, retry with exponential backoff
   - Configurable max retries (3 default)
5. Implement Paystack webhook verification:
   - HMAC-SHA512 signature verification (Paystack's actual method)
6. Store pricing plans in database:
   - `pricing_plans` table: id, name, price_cents, currency, features_json, is_active
   - `get_pricing()` reads from DB, not hardcoded floats

### Files to modify
- `crates/shellwego-billing/Cargo.toml` — add `stripe` crate
- `crates/shellwego-billing/src/lib.rs` — rewrite customer/payment/invoice/webhook code
- `crates/shellwego-billing/src/invoices.rs` — wire real storage
- `migrations/` — add billing tables

### Files to reference (read-only)
- `crates/shellwego-billing/src/metering.rs` — real metering code (don't break this)
- `crates/shellwego-schema/src/billing.rs` — billing type definitions
- `readme.md` — pricing strategy section

### Acceptance criteria
- [ ] `process_card_payment()` calls real Stripe API (or mock in test mode)
- [ ] `verify_stripe_webhook()` rejects tampered signatures
- [ ] Customer created via `register_customer()` is persisted and retrievable after restart
- [ ] Invoice created via `store_invoice()` exists in database
- [ ] `retry_failed_payments()` actually retries failed payments with backoff
- [ ] Pricing plans read from database, not hardcoded
- [ ] All metering code (which is real) remains untouched and still works
- [ ] At least 8 new tests covering payment processing, webhook verification, invoice CRUD

### Dependencies
- Phase 1 (needs real DB for billing tables)

---

## Phase 6 — Agent Runtime Hardening

**Agent scope**: Complete the VMM driver and WASM runtime so the agent can actually start VMs and run WASM functions.

### What README claims
- "Firecracker microVMs (85ms cold start, 12MB overhead)"
- "Wasmtime WASM runtime (<10ms cold start)"
- "Automatic detection" of KVM/PVM/WASM backend
- Snapshot/restore, live migration
- Reconciliation loop (desired vs actual state)

### What actually exists
- `crates/shellwego-agent/` (~4,969 lines total):
  - `daemon.rs` — real QUIC-based heartbeat/command protocol
  - `reconciler.rs` — real K8s-style desired-vs-actual reconciliation loop
  - `detect_capabilities()` — reads system info via `sysinfo`, checks `/dev/kvm`
  - `wasm/runtime.rs` — creates Wasmtime engine but execution layer is thin
  - `vmm/driver.rs` — framework exists but VM lifecycle is mostly scaffolding
  - `snapshot.rs` — thin wrapper, no actual snapshot/restore
  - `migration.rs` — QUIC transport exists but live migration is incomplete
  - PVM detection checks for `/usr/local/bin/firecracker-pvm` and `/sys/module/pvm` (non-standard)

### Specific issues to fix
1. **HIGH**: VMM driver doesn't actually start/stop Firecracker VMs end-to-end
2. **HIGH**: WASM runtime creates engine but function loading/execution is incomplete
3. **HIGH**: Snapshot save/restore is a stub
4. **MEDIUM**: PVM detection uses non-standard paths
5. **MEDIUM**: Live migration over QUIC is skeleton-only
6. **LOW**: No integration between VMM driver and the Firecracker client crate

### What the agent must do
1. Complete the VMM driver (`vmm/driver.rs`):
   - Use `shellwego-firecracker` crate's HTTP client to:
     - `PUT /boot-source` — set kernel and initrd
     - `PUT /machine-config` — set vCPU, memory
     - `PUT /drives/{id}` — attach rootfs from ZFS snapshot
     - `PUT /network-interfaces/{id}` — configure tap device (from shellwego-network)
     - `PUT /actions` — InstanceStart, InstanceStop
   - Implement proper error handling for each API call
   - Add jailer wrapping: set up jailer chroot, uid/gid mapping
2. Complete WASM runtime (`wasm/runtime.rs`):
   - Implement `WasmFunction::from_file(path)` — load .wasm module
   - Implement `WasmFunction::call(input_bytes) -> output_bytes` — execute with Wasmtime
   - Implement resource limits (memory, CPU) via Wasmtime's `Config`
   - Implement stdin/stdout/stderr capture
   - Add module caching (pre-compiled modules for faster cold start)
3. Complete snapshot manager:
   - `save_snapshot(vm_id)` → call Firecracker's `PUT /snapshot/create`
   - `restore_snapshot(vm_id)` → call `PUT /snapshot/load`
   - Track snapshot files on disk (ZFS-backed)
4. Fix PVM detection to check for QEMU/KVM fallback instead of fictional binary:
   - Check for `qemu-system-x86_64` as fallback
   - Remove `/usr/local/bin/firecracker-pvm` check
5. Add proper integration between agent and the real control-plane API (Phase 4):
   - Agent reports actual node capacity (CPU, RAM, disk from `sysinfo`)
   - Agent receives deployment specs and reconciles

### Files to modify
- `crates/shellwego-agent/src/vmm/driver.rs` — complete VM lifecycle
- `crates/shellwego-agent/src/wasm/runtime.rs` — complete WASM execution
- `crates/shellwego-agent/src/snapshot.rs` — real snapshot save/restore
- `crates/shellwego-agent/src/migration.rs` — improve migration (or mark as future work)
- `crates/shellwego-agent/src/lib.rs` — capability detection fixes
- `crates/shellwego-agent/src/daemon.rs` — real capacity reporting

### Files to reference (read-only)
- `crates/shellwego-firecracker/src/` — Firecracker HTTP API client
- `crates/shellwego-storage/src/zfs/` — ZFS snapshot/clone for rootfs
- `crates/shellwego-network/src/` — tap device and bridge setup
- `crates/shellwego-schema/src/firecracker.rs` — VMM config types

### Acceptance criteria
- [ ] `VmmDriver.start_vm(config)` calls Firecracker API and boots a microVM (testable with mock socket)
- [ ] `VmmDriver.stop_vm(id)` sends InstanceStop action
- [ ] `WasmRuntime` loads and executes a .wasm file, returning correct output
- [ ] WASM resource limits are enforced (memory limit causes trap)
- [ ] Snapshot save/restore calls correct Firecracker API endpoints
- [ ] Node capacity reported to control-plane matches actual system resources
- [ ] At least 10 unit tests with mock Firecracker socket

### Dependencies
- Phase 1 (DB for agent state)
- Phase 4 (real API endpoints for agent to communicate with)

---

## Phase 7 — Edge Proxy: TLS & ACME

**Agent scope**: Replace the stubbed ACME and WebSocket proxy with real Let's Encrypt integration and proper proxying.

### What README claims
- "shellwego-edge (Rust proxy, Traefik replacement) with SSL auto-generation"
- "Automatic SSL (Let's Encrypt)"
- "Real-time log streaming via WebSocket"

### What actually exists
- `crates/shellwego-edge/` (~2,200 lines total):
  - **Real parts**: HTTP/1.1 reverse proxy with connection pooling, 5 load-balancing strategies, middleware pipeline, security headers, TLS cert parsing via `rustls-pemfile`, cert store CRUD
  - **STUBBED**: ACME integration — `request_certificate()` generates self-signed cert via `rcgen` instead of calling Let's Encrypt
  - **STUBBED**: `CertificateResolver.resolve()` returns `None` always — SNI-based cert selection doesn't work
  - **STUBBED**: WebSocket upgrade returns 101 but doesn't actually proxy the connection

### Specific issues to fix
1. **HIGH**: ACME/Let's Encrypt integration is fake (generates self-signed certs)
2. **HIGH**: SNI certificate resolution always returns None
3. **HIGH**: WebSocket proxy accepts upgrade but doesn't forward frames
4. **MEDIUM**: No certificate renewal automation
5. **LOW**: No OCSP stapling

### What the agent must do
1. Implement real ACME (Let's Encrypt) integration:
   - Use `acme2` crate or raw HTTP client for ACMEv2 protocol
   - Implement: account registration, order creation, DNS-01 or HTTP-01 challenge, certificate download
   - Auto-renewal: check expiry 30 days before, renew in background
   - Store issued certificates in the cert store (already exists)
2. Fix SNI-based certificate resolution:
   - `CertificateResolver.resolve(domain)` → look up cert in store by domain name
   - Serve correct cert for each incoming TLS connection based on SNI
   - Use `rustls` `ResolvesServerCert` trait properly
3. Implement real WebSocket proxy:
   - After 101 upgrade, spawn two tasks:
     - Client→backend: read frames from client, write to backend TCP
     - Backend→client: read from backend TCP, write frames to client
   - Handle ping/pong, close frames
   - Support proxying to the control-plane's WebSocket log streaming endpoint

### Files to modify
- `crates/shellwego-edge/src/tls.rs` — real ACME, SNI resolution
- `crates/shellwego-edge/src/proxy.rs` — WebSocket proxying
- `crates/shellwego-edge/Cargo.toml` — add `acme2` or equivalent

### Files to reference (read-only)
- `crates/shellwego-edge/src/lib.rs` — existing architecture
- `readme.md` — SSL/TLS section

### Acceptance criteria
- [ ] ACME client registers account with Let's Encrypt staging
- [ ] Certificate is issued and stored after completing HTTP-01 challenge
- [ ] SNI resolution returns correct cert for each domain
- [ ] WebSocket proxy forwards frames bidirectionally
- [ ] Certificate auto-renewal task runs and renews certs before expiry
- [ ] At least 5 tests covering cert resolution, WS frame forwarding

### Dependencies
- Phase 4 (needs real control-plane WebSocket endpoint to proxy to)

---

## Phase 8 — Networking & eBPF Verification

**Agent scope**: Verify and harden the networking stack, ensure eBPF programs are real and functional.

### What README claims
- "Custom eBPF (Aya)" for XDP/TC packet filtering
- "XDP (eXpress Data Path) for DDoS protection at NIC level"
- "3x faster than iptables"
- VXLAN overlay, WireGuard support
- IPAM for address management
- QUIC message bus (5M+ msgs/sec)

### What actually exists
- `crates/shellwego-network/` (~2,000+ lines):
   - Uses `rtnetlink` for real Linux network interface management
   - Uses `nix` for system-level operations
   - Quinn QUIC client/server with real crypto
   - eBPF module with precompiled binary (`shellwego.bin`)
   - VXLAN overlay and WireGuard support
   - IPAM for address management
- The network crate appears to be one of the **real** crates but the eBPF binary needs verification

### Specific issues to verify/fix
1. **HIGH**: eBPF binary — is `shellwego.bin` a real compiled eBPF program or a placeholder? Need to verify
2. **MEDIUM**: XDP program — does it actually attach to network interface and filter packets?
3. **MEDIUM**: TC egress rate limiter — functional or stubbed?
4. **LOW**: QUIC performance claims (5M+ msgs/sec) — benchmark needed

### What the agent must do
1. Verify the eBPF binary:
   - Run `llvm-objdump -d shellwego.bin` or `bpftool prog show` to verify it's a real eBPF program
   - If it's a 0-byte file or placeholder, create a real XDP packet filter using Aya:
     - Count packets per source IP
     - Drop packets from blocked IPs (simple blacklist)
     - Rate limit per-IP (token bucket in BPF map)
2. Verify/fix the CNI networking:
   - Test `create_tap()`, `setup_bridge()`, `assign_ip()` on a real Linux system
   - Ensure IPAM doesn't assign duplicate IPs across VMs
3. Verify QUIC message bus:
   - Test actual message throughput between control-plane and agent
   - Test reconnection after network interruption
   - Test message ordering guarantees
4. Document the networking setup clearly:
   - Required kernel modules (br_netfilter, vxlan, wireguard)
   - Required capabilities (CAP_NET_ADMIN, CAP_SYS_ADMIN)
   - Required sysctl settings

### Files to modify
- `crates/shellwego-network/src/ebpf/` — verify/fix eBPF programs
- `crates/shellwego-network/src/cni/` — verify CNI implementation
- `crates/shellwego-network/src/quinn/` — verify QUIC bus

### Files to reference (read-only)
- `crates/shellwego-network/src/lib.rs` — architecture overview
- `readme.md` — networking section, eBPF section

### Acceptance criteria
- [ ] eBPF binary is a real compiled BPF program (verified via bpftool or objdump)
- [ ] XDP program attaches to interface and filters packets
- [ ] IPAM assigns unique IPs, no duplicates
- [ ] QUIC message bus delivers messages with <10ms latency at 10k msgs/sec
- [ ] Documentation lists all prerequisites (kernel version, modules, capabilities)

### Dependencies
- Phase 6 (agent needs networking for VM tap devices)

---

## Phase 9 — Infrastructure & Compliance

**Agent scope**: Create all missing infrastructure files and bring the repo into legal/compliance alignment.

### What README claims
- "One-command deployment: `./install.sh` and you have a cloud"
- "AGPL-3.0 Licensed"
- "Docker Compose import"
- "Kubernetes Helm charts"
- "Contributor License Agreement (CLA)"
- GitHub Actions CI with KVM-enabled runners

### What actually exists
- `install.sh` — **MISSING** (README references `curl -fsSL https://shellwego.com/install.sh | bash`)
- `LICENSE` — **MISSING** (Cargo.toml and README claim AGPL-3.0-or-later)
- `docker-compose.yml` — exists but is a template, needs verification
- `Makefile` — exists
- `scripts/` — exists but may be incomplete
- No Helm charts
- No GitHub Actions workflows
- No CLA.md
- No CONTRIBUTING.md

### Specific issues to fix
1. **CRITICAL**: No `LICENSE` file — illegal to claim AGPL-3.0 without the actual license text
2. **CRITICAL**: No `install.sh` — the entire "5-minute deploy" claim is false
3. **HIGH**: No CI/CD pipeline
4. **MEDIUM**: No CONTRIBUTING.md or CLA.md
5. **MEDIUM**: No Helm charts
6. **LOW**: Docker images not buildable from Dockerfile

### What the agent must do
1. Create `LICENSE` — copy the official AGPL-3.0 full text from SPDX
2. Create `scripts/install.sh`:
   - Parse arguments: `--domain`, `--email`, `--mode` (kvm/pvm/wasm), `--license`
   - Detect OS and package manager (apt/yum/dnf/apk)
   - Install Rust toolchain if not present
   - Install system dependencies (zfs, firecracker, wasmtime)
   - Clone and build the monorepo
   - Initialize ZFS pool if mode != wasm
   - Generate self-signed TLS cert (or use ACME from Phase 7)
   - Create systemd service files for control-plane and agent
   - Run `shellwego init --role=control-plane ...`
   - Print success message with dashboard URL
   - Support `--uninstall` for cleanup
3. Create `.github/workflows/ci.yml`:
   - On push/PR: `cargo fmt --check`, `cargo clippy`, `cargo test`
   - On release: build release binaries for linux-amd64 and linux-arm64
   - Cache cargo registry and target
   - Use KVM-enabled runner for integration tests (or skip with feature flag)
4. Create `CONTRIBUTING.md`:
   - Development setup instructions
   - Code style (rustfmt, clippy)
   - PR process
   - Testing requirements
5. Create `CLA.md`:
   - Contributor License Agreement text matching README's description
6. Verify `docker-compose.yml`:
   - Ensure it references a real Docker image or builds from Dockerfile
   - Create `Dockerfile` if missing (multi-stage: build with Rust, run with minimal base)
7. Create basic Helm chart structure:
   - `charts/shellwego/Chart.yaml`
   - `charts/shellwego/values.yaml`
   - `charts/shellwego/templates/deployment.yaml`
   - `charts/shellwego/templates/service.yaml`
   - `charts/shellwego/templates/configmap.yaml`

### Files to create
- `LICENSE`
- `scripts/install.sh`
- `.github/workflows/ci.yml`
- `CONTRIBUTING.md`
- `CLA.md`
- `Dockerfile` (if missing)
- `charts/shellwego/` directory with Helm templates

### Files to reference (read-only)
- `readme.md` — installation section, license section, development section
- `docker-compose.yml` — existing compose config
- `Makefile` — existing build commands

### Acceptance criteria
- [ ] `LICENSE` contains full AGPL-3.0 text
- [ ] `scripts/install.sh` runs end-to-end on a fresh Ubuntu 22.04 and produces a working control-plane
- [ ] CI pipeline runs `cargo fmt`, `cargo clippy`, `cargo test` on every push
- [ ] `Dockerfile` produces a working container image
- [ ] Helm chart installs via `helm install`
- [ ] CONTRIBUTING.md and CLA.md exist with complete content

### Dependencies
- Phases 1-8 (install.sh needs a working binary to deploy)

---

## Phase 10 — Integration Testing & Validation

**Agent scope**: Write end-to-end tests that verify the entire system works as README claims.

### What README claims
- "Production Ready" — version 1.0.0, battle-tested on 500+ production apps
- "15-second cold starts"
- "97% margin" on $15/month pricing
- "Zero-copy proxy"
- All API endpoints, CLI commands, and platform features functional

### What actually exists
- Minimal unit tests in some crates (orm, kms)
- No integration tests
- No E2E test that exercises the full stack
- README's claims are entirely aspirational

### Specific issues to fix
1. **HIGH**: No integration tests exist
2. **HIGH**: No E2E test covers the full deploy flow
3. **MEDIUM**: No performance benchmarks
4. **MEDIUM**: README claims are unverified

### What the agent must do
1. Create integration test suite (`tests/` directory):
   - `test_auth_flow.rs` — register, login, refresh token, expired token rejection
   - `test_crud_apps.rs` — create app, list apps, get app, update app, delete app
   - `test_crud_nodes.rs` — register node, list nodes, node heartbeat
   - `test_secrets_flow.rs` — create secret (encrypted), retrieve secret (decrypted), verify KMS roundtrip
   - `test_billing_flow.rs` — create customer, create subscription, generate invoice, mock payment
   - `test_deploy_flow.rs` — create app via API, agent picks up, starts VM (mocked Firecracker)
2. Create test utilities:
   - `tests/common/mod.rs` — test harness that starts control-plane with test DB (SQLite in-memory)
   - Mock helpers for external services (Stripe, Vault, Firecracker)
   - Test data factories for creating entities
3. Create E2E test script (`tests/e2e/deploy_test.sh`):
   - Start control-plane
   - Register a user
   - Create an organization
   - Create an app
   - Verify app appears in list
   - Delete app
   - Verify app is gone
4. Add `Makefile` targets:
   - `make test` — unit tests only
   - `make test-integration` — integration tests
   - `make test-e2e` — full E2E flow
5. Update README's checked items to reflect actual status:
   - Remove `[x]` from features that don't work
   - Add `[ ]` to features that need Phase N to complete
   - Or: after this phase, verify and keep `[x]` for real features

### Files to create
- `tests/common/mod.rs`
- `tests/test_auth_flow.rs`
- `tests/test_crud_apps.rs`
- `tests/test_crud_nodes.rs`
- `tests/test_secrets_flow.rs`
- `tests/test_billing_flow.rs`
- `tests/test_deploy_flow.rs`
- `tests/e2e/deploy_test.sh`

### Files to modify
- `Makefile` — add test targets
- `readme.md` — update feature checklist to reflect reality

### Acceptance criteria
- [ ] `make test-integration` passes all tests
- [ ] Auth flow test: register → login → access protected endpoint → token expires → 401
- [ ] Secret roundtrip: encrypt "hello" → store → retrieve → decrypt → "hello"
- [ ] Full CRUD cycle works for all entity types
- [ ] E2E script completes without errors
- [ ] README feature checklist accurately reflects what works

### Dependencies
- Phases 1-9 (all features must be implemented before testing)

---

## Dependency Graph

```
Phase 1 (Database)
├── Phase 2 (Auth)
│   └── Phase 4 (API Handlers) ── depends on Phase 2 + Phase 3
│       ├── Phase 5 (Billing)
│       ├── Phase 6 (Agent)
│       │   └── Phase 8 (Networking)
│       └── Phase 7 (Edge Proxy)
├── Phase 3 (KMS)
│   └── Phase 4 (API Handlers)
└── Phase 9 (Infrastructure) ── depends on all above
    └── Phase 10 (Integration Testing) ── depends on all above
```

**Parallelizable** (can run simultaneously):
- Phase 2 and Phase 3 can run in parallel (both depend only on Phase 1)
- Phase 5 can start once Phase 1 is done (it doesn't need API handlers)
- Phase 7 and Phase 8 can run in parallel

---

## Effort Estimate

| Phase | Scope | Lines Changed (est.) | Complexity |
|-------|-------|---------------------|------------|
| 1 — Database | Full ORM rewrite + migrations | ~2,000 | High |
| 2 — Auth | JWT, RBAC, rate limiting, audit | ~1,500 | High |
| 3 — KMS | Real crypto, 5 backends | ~1,200 | High |
| 4 — API Handlers | All CRUD + validation + WS | ~3,000 | High |
| 5 — Billing | Stripe, persistence, webhooks | ~1,500 | Medium |
| 6 — Agent | VMM driver, WASM, snapshots | ~2,000 | Very High |
| 7 — Edge | ACME, SNI, WebSocket proxy | ~800 | Medium |
| 8 — Networking | eBPF verification, testing | ~500 | High |
| 9 — Infrastructure | install.sh, LICENSE, CI, Helm | ~1,500 | Medium |
| 10 — Testing | Integration + E2E suites | ~2,000 | Medium |
| **Total** | | **~16,000** | |

---

## How to Use This Plan

Each phase is designed to be handed to a single AI agent with this prompt structure:

```
You are working on Phase N: [Title] of the ShellWeGo production remediation.

Repository: /path/to/shellwego-backend-monorepo
Previous phases completed: [list completed phases]
Your phase: [paste the full phase section from this plan]

Read all "Files to reference" first to understand the codebase context.
Then implement all items in "What the agent must do".
Verify all "Acceptance criteria" before finishing.
Run `cargo check -p [relevant-crate]` and `cargo test -p [relevant-crate]` to validate.
```
