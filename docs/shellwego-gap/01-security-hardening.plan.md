# Plan 01: Security Hardening

## 1. Title & Overview

**Security Hardening** — Fix five critical security gaps between what the README claims and what the code actually implements: (A) migrate JWT from HS256 to RS256, (B) wire RBAC permission checks into every protected handler, (C) replace simulated KMS encryption with real AES-256-GCM, (D) add audit logging on secret access, and (E) bootstrap supply-chain security (SBOM, image signing, vulnerability scanning). This is a prerequisite for any production deployment.

## 2. Gap Summary

| # | Readme Claim | Actual Implementation | File(s) | Severity |
|---|---|---|---|---|
| A | "JWT with RS256 (asymmetric)" | `Header::default()` = HS256, `EncodingKey::from_secret()` = shared HMAC secret | `crates/shellwego-control-plane/src/auth/jwt.rs` lines 88-89, 115-116 | **CRITICAL** |
| B | "RBAC with resource-level permissions" | `check_permission()` exists in `rbac.rs` and `middleware.rs` but is **never called** from any handler. Middleware only validates token identity, not permissions. | `crates/shellwego-control-plane/src/api/handlers.rs` (all protected handlers), `crates/shellwego-control-plane/src/api/middleware.rs` line 149 | **CRITICAL** |
| C | "AES-256-GCM" secrets encryption | All backends do `BASE64.encode(format!("prefix:{}", plaintext))` — reversible base64, not encryption | `crates/shellwego-control-plane/src/kms/mod.rs` lines 178-213 | **CRITICAL** |
| D | "Audit logging of all secret access" | `AuditLogEntry` struct exists in schema; `list_audit_logs` handler exists but returns empty. No writes to audit log anywhere. | `crates/shellwego-schema/src/entities/audit.rs`, `crates/shellwego-control-plane/src/api/handlers.rs` line 1671 | **HIGH** |
| E | "Cosign, Syft SBOM, Trivy, Nix builds" | No `.cosign/`, no `syft` config, no `.trivy.yml`, no `flake.nix` in repo | Root of repo | **MEDIUM** |

## 3. Scope

### Files to Modify

| File | Change |
|---|---|
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/auth/jwt.rs` | Replace HS256 with RS256; add key-loading helpers; update `create_access_token`, `create_refresh_token`, `validate_token` |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/config.rs` | Add `JwtConfig` fields: `private_key_pem: Option<String>`, `public_key_pem: Option<String>`, `private_key_path: Option<String>`, `public_key_path: Option<String>` |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/auth/mod.rs` | Export new key-loading helpers; keep `AuthService::new` backward-compatible |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/api/handlers.rs` | Add `Extension(current_user): Extension<CurrentUser>` parameter + `check_permission()` call to every protected handler |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/api/middleware.rs` | Keep existing `check_permission` helper; no structural changes needed (already returns FORBIDDEN) |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/api/mod.rs` | No structural changes (routes stay the same) |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/kms/mod.rs` | Replace base64 stubs with real AES-256-GCM using `aes-gcm` crate; add key-derivation from master key |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/Cargo.toml` | Add deps: `rsa`, `rand_core`, `aes-gcm`, `sha2`; add `jsonwebtoken` feature for `use_pem` |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/state.rs` | Add `audit_log: Arc<AuditService>` to `AppState` |
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/lib.rs` | Register `audit` module |

### New Files to Create

| File | Purpose |
|---|---|
| `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/audit.rs` | `AuditService` struct with `log_event()` method; writes `AuditLogEntry` to `audit_logs` DB table |
| `/home/z/my-project/shellwego-backend-monorepo/scripts/generate-jwt-keys.sh` | One-shot script to generate RSA-2048 PEM key pair for development |
| `/home/z/my-project/shellwego-backend-monorepo/.trivy.yml` | Trivy config: severity levels, ignore file, output format |
| `/home/z/my-project/shellwego-backend-monorepo/.syft.yaml` | Syft SBOM generation config |
| `/home/z/my-project/shellwego-backend-monorepo/scripts/sbom.sh` | Wrapper to generate SBOM via Syft after build |
| `/home/z/my-project/shellwego-backend-monorepo/scripts/scan.sh` | Wrapper to run Trivy filesystem + image scan |

## 4. Prerequisites

1. **Build must pass** — The `docs/build-report.md` (dated 2026-04-05) shows 0 errors, 145 warnings across all crates. The control-plane compiles. If build errors exist at execution time, fix them first (see Plan 00 if it exists, or resolve locally).

2. **No dependency on live infrastructure** — All changes are code-only. The RSA key pair can be generated locally. AES-256-GCM is pure Rust (`aes-gcm` crate, no system dependencies). Trivy/Syft are external tools invoked via shell scripts, not integrated into the build.

3. **Test infrastructure** — Existing unit tests in `jwt.rs`, `rbac.rs`, and `auth/mod.rs` must continue to pass after modifications. New tests must be added for RS256, real AES-GCM, and audit logging.

## 5. Detailed Implementation Steps

### Phase A: JWT Migration HS256 → RS256

**A1. Add crate dependencies**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/Cargo.toml`

Add under `[dependencies]`:
```toml
jsonwebtoken = { version = "9", features = ["use_pem"] }
rsa = "0.9"
rand_core = { version = "0.6", features = ["std"] }
sha2 = "0.10"
```

> The `jsonwebtoken` crate is already at version 9 but without the `use_pem` feature. Adding `use_pem` enables `EncodingKey::from_rsa_pem()` and `DecodingKey::from_rsa_pem()`.

**A2. Extend `JwtConfig`**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/config.rs`

Add fields to `JwtConfig`:
```rust
/// RSA private key PEM content (for token signing)
pub private_key_pem: Option<String>,
/// RSA public key PEM content (for token verification)
pub public_key_pem: Option<String>,
/// Path to RSA private key PEM file (alternative to inline)
pub private_key_path: Option<String>,
/// Path to RSA public key PEM file (alternative to inline)
pub public_key_path: Option<String>,
```

In `Config::load()`:
- Read `JWT_PRIVATE_KEY_PATH` env var → load file → set `private_key_pem`
- Read `JWT_PUBLIC_KEY_PATH` env var → load file → set `public_key_pem`
- Fallback: if neither set, log warning and **auto-generate** an RSA-2048 keypair in memory (development mode only)

In `Config::default()`: leave new fields as `None`.

**A3. Rewrite token creation/validation functions**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/auth/jwt.rs`

Changes:
- Add `use jsonwebtoken::{EncodingKey, DecodingKey, Header};` (already imported)
- `create_access_token()`: Replace `Header::default()` with `Header::new(jsonwebtoken::Algorithm::RS256)` and `EncodingKey::from_rsa_pem(config.private_key_pem.unwrap().as_bytes()).unwrap()`
- `create_refresh_token()`: Same RS256 change
- `validate_token()`: Replace `DecodingKey::from_secret()` with `DecodingKey::from_rsa_pem(config.public_key_pem.unwrap().as_bytes()).unwrap()`
- Add validation for algorithm: `validation.required_spec_claims = ...; validation.algorithms = vec![jsonwebtoken::Algorithm::RS256];`
- Keep the `secret` field in `JwtConfig` for backward compatibility but log a deprecation warning if `private_key_pem` is `None` and `secret` is non-empty (i.e., running in legacy HS256 mode for migration)

**A4. Update `AuthService`**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/auth/mod.rs`

- `AuthService::new()`: No changes needed — it passes `JwtConfig` through to `jwt::*` functions.
- Add a helper method:
```rust
pub fn validate_access_token_raw(&self, token_str: &str) -> Result<AccessClaims, AuthError> {
    jwt::validate_token(&self.jwt_config, token_str, false)
}
```
  This is useful for tests that don't need the blocklist check.

**A5. Generate dev keys script**

File: `/home/z/my-project/shellwego-backend-monorepo/scripts/generate-jwt-keys.sh`

```bash
#!/usr/bin/env bash
# Generate RSA-2048 PEM key pair for ShellWeGo development
set -euo pipefail
OUT_DIR="${1:-.}"
openssl genrsa -out "$OUT_DIR/jwt-private.pem" 2048
openssl rsa -in "$OUT_DIR/jwt-private.pem" -pubout -out "$OUT_DIR/jwt-public.pem"
echo "Generated: $OUT_DIR/jwt-private.pem, $OUT_DIR/jwt-public.pem"
```

**A6. Update existing tests**

In `jwt.rs` tests: Generate an RSA keypair at test setup time:
```rust
fn test_rsa_keys() -> (String, String) {
    let private_key = rsa::RsaPrivateKey::new(&mut rand_core::OsRng, 2048).unwrap();
    let public_key = private_key.to_public_key();
    let private_pem = private_key.to_pkcs1_pem(rsa::pkcs8::LineEnding::LF).unwrap().to_string();
    let public_pem = public_key.to_public_key_pem(rsa::pkcs8::LineEnding::LF).unwrap().to_string();
    (private_pem, public_pem)
}
```

Update `test_config()` to set `private_key_pem` and `public_key_pem`, and set `secret` to empty. Update all 7 existing tests to use RSA keys. Add a new test `test_hs256_rejected` that verifies RS256-only tokens cannot be validated with HS256 keys.

### Phase B: Wire RBAC into Handlers

**B1. Add `CurrentUser` extraction to every protected handler**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/api/handlers.rs`

Add import at top:
```rust
use axum::Extension;
```

For **each** protected handler, add `Extension(current_user): Extension<CurrentUser>` as a parameter and a `check_permission()` call. The `check_permission` function from `super::middleware` returns `Result<(), (StatusCode, ErrorResponse)>`.

Permission mapping (resource:action):

| Handler | Permission |
|---|---|
| `list_apps` | `"apps:read"` |
| `create_app` | `"apps:write"` |
| `get_app` | `"apps:read"` |
| `delete_app` | `"apps:delete"` |
| `deploy_app` | `"apps:write"` |
| `scale_app` | `"apps:write"` |
| `restart_app` | `"apps:write"` |
| `stop_app` | `"apps:write"` |
| `start_app` | `"apps:write"` |
| `get_logs` | `"apps:read"` |
| `ws_logs` | `"apps:read"` |
| `list_nodes` | `"nodes:read"` |
| `register_node` | `"nodes:write"` |
| `get_node` | `"nodes:read"` |
| `deregister_node` | `"nodes:delete"` |
| `drain_node` | `"nodes:write"` |
| `list_volumes` | `"volumes:read"` |
| `create_volume` | `"volumes:write"` |
| `get_volume` | `"volumes:read"` |
| `delete_volume` | `"volumes:delete"` |
| `attach_volume` | `"volumes:write"` |
| `detach_volume` | `"volumes:write"` |
| `snapshot_volume` | `"volumes:write"` |
| `list_domains` | `"domains:read"` |
| `create_domain` | `"domains:write"` |
| `get_domain` | `"domains:read"` |
| `delete_domain` | `"domains:delete"` |
| `verify_domain` | `"domains:write"` |
| `list_databases` | `"databases:read"` |
| `create_database` | `"databases:write"` |
| `get_database` | `"databases:read"` |
| `delete_database` | `"databases:delete"` |
| `backup_database` | `"databases:write"` |
| `restore_database` | `"databases:write"` |
| `list_secrets` | `"secrets:read"` |
| `create_secret` | `"secrets:write"` |
| `get_secret` | `"secrets:read"` |
| `delete_secret` | `"secrets:delete"` |
| `rotate_secret` | `"secrets:write"` |
| `list_builds` | `"builds:read"` |
| `get_build` | `"builds:read"` |
| `get_build_logs` | `"builds:read"` |
| `cancel_build` | `"builds:write"` |
| `list_organizations` | `"organizations:read"` |
| `create_organization` | `"organizations:write"` |
| `get_organization` | `"organizations:read"` |
| `get_metrics` | `"apps:read"` |
| `list_audit_logs` | `"audit:read"` |

Example transformation for `list_apps`:
```rust
pub async fn list_apps(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Query(params): Query<ListAppsQuery>,
) -> Result<Json<PaginatedResponse<App>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "apps:read")?;
    // ... existing logic unchanged ...
}
```

**B2. Add RBAC enforcement tests**

Add test module in `handlers.rs` or as integration tests:
- Test that a user with `"apps:read"` permission can call `list_apps` but not `create_app`
- Test that `admin:*` grants all permissions
- Test that `ReadOnly` role cannot call any write endpoint

### Phase C: Real AES-256-GCM Encryption in KMS

**C1. Add crate dependency**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/Cargo.toml`

```toml
aes-gcm = "0.10"
```

**C2. Rewrite KMS encryption for `File` backend**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/kms/mod.rs`

Add master key derivation:
- Add `master_key: [u8; 32]` field to `KmsClient`
- In `KmsClient::from_config()`, if `KmsBackend::File`, derive master key from `config.key_id` using Argon2 (already a dependency via `argon2` crate):
```rust
use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
use rand_core::OsRng;

let salt = SaltString::encode_b64(&OsRng);
let argon2 = Argon2::default();
let hash = argon2.hash_password(config.key_id.as_bytes(), &salt).unwrap();
let master_key: [u8; 32] = hash.hash.unwrap().as_bytes()[..32].try_into().unwrap();
```

Replace `encrypt_file()`:
```rust
async fn encrypt_file(&self, plaintext: &str) -> Result<(String, String), KmsError> {
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
    let cipher = Aes256Gcm::new_from_slice(&self.master_key)
        .map_err(|e| KmsError::EncryptionFailed(e.to_string()))?;
    let nonce_bytes = rand::random::<[u8; 12]>();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| KmsError::EncryptionFailed(e.to_string()))?;
    Ok((BASE64.encode(&ciphertext), BASE64.encode(&nonce_bytes)))
}
```

Replace `decrypt_file()`:
```rust
async fn decrypt_file(&self, ciphertext: &str) -> Result<String, KmsError> {
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
    let cipher = Aes256Gcm::new_from_slice(&self.master_key)
        .map_err(|e| KmsError::DecryptionFailed(e.to_string()))?;
    let ciphertext_bytes = BASE64.decode(ciphertext)?;
    let nonce_bytes = /* nonce stored alongside ciphertext or in EncryptedSecret.nonce */;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext_bytes.as_ref())
        .map_err(|e| KmsError::DecryptionFailed(e.to_string()))?;
    String::from_utf8(plaintext).map_err(Into::into)
}
```

**C3. Update external backend stubs**

For `Vault`, `AwsKms`, `GcpKms`, `AzureKeyVault` backends: Keep the stubs but add clear `todo!()` panics with messages indicating real client SDKs must be integrated. Currently they all do base64 encoding. The stubs should return `KmsError::BackendError("Vault backend not implemented. Integrate hvac/vault-rs crate.".to_string())` instead of fake encryption.

**C4. Update KMS tests**

- Update `test_encrypt_decrypt` to verify the ciphertext is NOT simply base64(plaintext)
- Add `test_different_plaintexts_produce_different_ciphertexts` (due to random nonce)
- Add `test_wrong_key_fails_decryption`
- Add `test_empty_plaintext_roundtrip`

**C5. Data migration note**

Existing secrets encrypted with the old fake base64 scheme will NOT decrypt with AES-256-GCM. Document this: all existing secrets must be re-encrypted. Add a migration helper method `KmsClient::migrate_legacy_secret()` that detects the `"file:"` prefix and re-encrypts.

### Phase D: Audit Logging on Secret Access

**D1. Create AuditService**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/audit.rs`

```rust
use crate::orm::Database;
use crate::auth::CurrentUser;
use chrono::Utc;
use serde_json;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

pub struct AuditService {
    db: Arc<Database>,
}

impl AuditService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Log an audit event to the database
    pub async fn log(
        &self,
        actor: &CurrentUser,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        changes: Option<serde_json::Value>,
    ) -> Result<(), anyhow::Error> {
        let entry = serde_json::json!({
            "id": Uuid::new_v4(),
            "timestamp": Utc::now().to_rfc3339(),
            "org_id": actor.organization_id,
            "actor_id": actor.user_id,
            "actor_type": "User",
            "action": action,
            "resource_type": resource_type,
            "resource_id": resource_id,
            "changes": changes,
            "metadata": {
                "ip_address": null,
                "user_agent": null,
                "request_id": null,
            },
        });

        self.db.insert("audit_logs", &entry).await?;
        info!(
            "AUDIT: user={} action={} resource={}:{}",
            actor.username, action, resource_type, resource_id
        );
        Ok(())
    }
}
```

**D2. Register in AppState**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/state.rs`

- Add `pub audit: Arc<AuditService>` to `AppState`
- In `AppState::new()`: `let audit = Arc::new(AuditService::new(db.clone()));`

**D3. Register module in lib.rs**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/lib.rs`

Add `pub mod audit;`

**D4. Wire audit logging into secret handlers**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/api/handlers.rs`

In each secret handler, after successful operation, call audit:
```rust
// In create_secret, after successful insert:
state.audit.log(&current_user, "secret.create", "secret", &id.to_string(), None).await
    .map_err(|e| tracing::warn!("Audit log failed: {}", e)); // non-fatal

// In get_secret, after successful find:
state.audit.log(&current_user, "secret.read", "secret", &secret_id.to_string(), None).await
    .map_err(|e| tracing::warn!("Audit log failed: {}", e));

// In delete_secret, after successful delete:
state.audit.log(&current_user, "secret.delete", "secret", &secret_id.to_string(), None).await
    .map_err(|e| tracing::warn!("Audit log failed: {}", e));

// In rotate_secret, after successful update:
state.audit.log(&current_user, "secret.rotate", "secret", &secret_id.to_string(),
    Some(serde_json::json!({"new_version": 2}))).await
    .map_err(|e| tracing::warn!("Audit log failed: {}", e));
```

**D5. Add audit logging for secret-related config operations (optional)**

Also audit `list_secrets` with action `"secret.list"` (read visibility).

**D6. Update `list_audit_logs` handler**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/api/handlers.rs` (line 1671)

Replace the empty-return stub with actual DB query:
```rust
pub async fn list_audit_logs(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<PaginatedResponse<serde_json::Value>>, (StatusCode, Json<ErrorResponse>)> {
    super::middleware::check_permission(&current_user, "audit:read")?;
    let items: Vec<serde_json::Value> = state.db.find_all("audit_logs").await
        .map_err(internal_db_err)?;
    let total = items.len() as u64;
    Ok(Json(PaginatedResponse::new(items, None, false).with_total_count(total)))
}
```

### Phase E: Supply Chain Security Bootstrapping

**E1. Create Trivy config**

File: `/home/z/my-project/shellwego-backend-monorepo/.trivy.yml`

```yaml
severity:
  - CRITICAL
  - HIGH
ignore:
  - CVE-XXXX-XXXXX  # Example: replace with real ignored CVEs if needed
output: table
format: table
```

**E2. Create Syft config**

File: `/home/z/my-project/shellwego-backend-monorepo/.syft.yaml`

```yaml
output:
  - "spdx-json=sbom.spdx.json"
  - "cyclonedx-json=sbom.cdx.json"
```

**E3. Create SBOM generation script**

File: `/home/z/my-project/shellwego-backend-monorepo/scripts/sbom.sh`

```bash
#!/usr/bin/env bash
# Generate SBOM for all ShellWeGo binaries
set -euo pipefail
TARGET_DIR="${1:-target/release}"
OUT_DIR="${2:-sbom}"
mkdir -p "$OUT_DIR"
for bin in shellwego-control-plane shellwego-agent shellwego-cli; do
    if [ -f "$TARGET_DIR/$bin" ]; then
        syft "$TARGET_DIR/$bin" -o spdx-json="$OUT_DIR/$bin.spdx.json" -o cyclonedx-json="$OUT_DIR/$bin.cdx.json"
        echo "SBOM generated: $OUT_DIR/$bin.*"
    fi
done
```

**E4. Create vulnerability scan script**

File: `/home/z/my-project/shellwego-backend-monorepo/scripts/scan.sh`

```bash
#!/usr/bin/env bash
# Run Trivy filesystem scan on the ShellWeGo binary
set -euo pipefail
BINARY="${1:-target/release/shellwego-control-plane}"
trivy fs --config .trivy.yml "$BINARY"
```

**E5. Add Makefile targets**

File: `/home/z/my-project/shellwego-backend-monorepo/Makefile`

Add:
```makefile
# Generate SBOM
sbom:
	bash scripts/sbom.sh

# Vulnerability scan
scan:
	bash scripts/scan.sh

# Generate JWT dev keys
jwt-keys:
	bash scripts/generate-jwt-keys.sh
```

**E6. Update Dockerfile with Cosign verify step**

File: `/home/z/my-project/shellwego-backend-monorepo/Dockerfile`

Add comment block before the runtime stage (actual Cosign verification would be in CI):
```dockerfile
# NOTE: In production CI, verify image signatures before deployment:
#   cosign verify --key cosign.pub shellwego/control-plane:$TAG
```

> Full Cosign key generation and signing is a CI/CD concern, not something that can be implemented purely in code. This plan bootstraps the tooling; actual integration into CI pipelines is a separate effort.

## 6. Dependencies on Other Plans

| Plan ID | Dependency | Notes |
|---|---|---|
| **None** | This plan is self-contained | All changes are within the control-plane crate and root-level scripts |

This plan should be executed **first** (or in parallel with Plan 00 if it exists) because:
- RBAC changes touch every handler — other plans modifying handlers must merge on top
- JWT config changes affect the auth module shared by all authenticated flows
- KMS changes affect the secret handlers that billing/deployment plans may also touch

## 7. Acceptance Criteria

### Unit Tests
- [ ] `cargo test -p shellwego-control-plane` passes with 0 failures
- [ ] All 7 existing JWT tests updated and passing with RS256
- [ ] New test: RS256 tokens validated, HS256 tokens rejected
- [ ] All 5 existing RBAC tests pass unchanged
- [ ] New KMS tests: AES-GCM encrypt/decrypt roundtrip, different ciphertexts, wrong key fails
- [ ] AuditService test: `log()` writes to DB and `list_audit_logs` returns it

### Integration Verification
- [ ] Start server with `JWT_PRIVATE_KEY_PATH` and `JWT_PUBLIC_KEY_PATH` set → auth works
- [ ] Start server without RSA keys → auto-generates keys, auth works (dev mode)
- [ ] User with `"apps:read"` only can GET `/v1/apps` but gets 403 on POST `/v1/apps`
- [ ] User with `"admin:*"` can access all endpoints
- [ ] Secret created via API → stored with AES-256-GCM ciphertext (not base64 of plaintext)
- [ ] Secret rotation → audit log entry appears in `GET /v1/audit-logs`
- [ ] `cargo build --release` succeeds (no new compile errors)

### Supply Chain
- [ ] `bash scripts/generate-jwt-keys.sh` produces valid PEM files
- [ ] `bash scripts/sbom.sh` produces SPDX and CycloneDX JSON files
- [ ] `bash scripts/scan.sh` runs Trivy without errors

## 8. Estimated Complexity

**XL** (Extra Large)

Rationale:
- Phase A (JWT): ~150 lines changed across 3 files + test updates. Medium complexity but high security criticality.
- Phase B (RBAC): ~40 handlers need modification (add 2 lines each) — mechanical but large surface area. ~80 lines of changes. High risk of merge conflicts.
- Phase C (KMS): ~100 lines changed in 1 file + new tests. Moderate complexity (crypto code).
- Phase D (Audit): ~100 lines new code + ~20 lines changed across handlers. Low-medium complexity.
- Phase E (Supply chain): ~60 lines across 5 new files. Low complexity, mostly config/scripts.

Total: ~490 lines of production code + ~150 lines of test code.

## 9. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **Breaking JWT change** — existing tokens invalid after switch | High | High — all clients must re-authenticate | Auto-generate RSA keys in dev mode; document that existing sessions are invalidated; consider dual-algorithm support during migration |
| **Merge conflicts on handlers.rs** — Phase B touches every handler | Medium | Medium — rebase may be needed | Execute this plan early; keep RBAC additions minimal (2 lines per handler) |
| **AES-GCM key derivation weakness** — Argon2 on short `key_id` | Low | High | Enforce minimum key_id length of 16 chars; warn if using default key_id |
| **KMS migration breaks existing secrets** | Certain if secrets exist | High | Add migration detection (prefix check); provide `migrate_legacy_secret()` helper |
| **`aes-gcm` crate version conflict** | Low | Medium | `aes-gcm 0.10` uses modern `cipher` traits; verify compatibility with `rand_core` version |
| **Trivy/Syft not installed in CI** | High (tooling) | Low — scripts fail gracefully | Scripts should check for tool existence and print install instructions |
| **Performance regression from audit logging** — extra DB write per request | Low | Low — audit is async fire-and-forget | Use `tokio::spawn` for audit writes so they don't block the handler response |
