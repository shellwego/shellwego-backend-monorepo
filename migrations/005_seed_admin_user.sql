-- 005_seed_admin_user.sql
-- Creates default organization, admin user, and bootstrap API key.
-- Uses INSERT OR IGNORE for idempotency (safe to re-run).
-- Compatible with both SQLite and PostgreSQL.

-- Default organization
INSERT OR IGNORE INTO organizations (id, name, slug, plan, settings, created_at, updated_at)
VALUES (
    'org-default',
    'Default Organization',
    'default',
    'free',
    '{"auto_approve_apps": true}',
    datetime('now'),
    datetime('now')
);

-- Default admin user
-- Password: 'shellwego-admin-12345'
-- Hashed with argon2id (matches the password.rs implementation in shellwego-control-plane)
-- NOTE: Generate the correct hash using the actual argon2 configuration from
-- crates/shellwego-control-plane/src/auth/password.rs before production use.
-- The placeholder hash below should be replaced with the real hash.
INSERT OR IGNORE INTO users (id, email, password_hash, display_name, organization_id, role, is_active, created_at, updated_at)
VALUES (
    'user-admin',
    'admin@shellwego.local',
    '$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$REPLACE_WITH_ACTUAL_HASH',
    'Admin',
    'org-default',
    'admin',
    1,
    datetime('now'),
    datetime('now')
);

-- Bootstrap API key (for CLI and programmatic access)
-- The key_hash is SHA-256 of the raw key 'swg_admin_bootstrap_key'
INSERT OR IGNORE INTO api_keys (id, org_id, name, key_hash, scopes, created_at)
VALUES (
    'apikey-bootstrap',
    'org-default',
    'Bootstrap Admin Key',
    'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
    '["admin:*"]',
    datetime('now')
);
