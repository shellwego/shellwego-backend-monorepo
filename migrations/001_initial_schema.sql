-- 001_initial_schema.sql
-- Core platform tables: organizations, users, apps, nodes, deployments, builds,
-- volumes, domains, managed_databases, secrets, audit_logs, team_members, api_keys
-- Compatible with BOTH SQLite and PostgreSQL (using TEXT/INTEGER types)

CREATE TABLE IF NOT EXISTS organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    plan TEXT NOT NULL DEFAULT 'free',
    settings TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    display_name TEXT NOT NULL DEFAULT '',
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    role TEXT NOT NULL DEFAULT 'developer',
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS apps (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'creating',
    image TEXT NOT NULL DEFAULT '',
    command TEXT,
    resources TEXT NOT NULL DEFAULT '{}',
    env TEXT NOT NULL DEFAULT '[]',
    domains TEXT NOT NULL DEFAULT '[]',
    volumes TEXT NOT NULL DEFAULT '[]',
    health_check TEXT,
    source TEXT NOT NULL DEFAULT '{}',
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    hostname TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'registering',
    region TEXT NOT NULL DEFAULT '',
    zone TEXT NOT NULL DEFAULT '',
    capacity TEXT NOT NULL DEFAULT '{}',
    capabilities TEXT NOT NULL DEFAULT '{}',
    network TEXT NOT NULL DEFAULT '{}',
    labels TEXT NOT NULL DEFAULT '{}',
    running_apps INTEGER NOT NULL DEFAULT 0,
    microvm_capacity INTEGER NOT NULL DEFAULT 0,
    microvm_used INTEGER NOT NULL DEFAULT 0,
    kernel_version TEXT NOT NULL DEFAULT '',
    firecracker_version TEXT NOT NULL DEFAULT '',
    agent_version TEXT NOT NULL DEFAULT '',
    last_seen TEXT NOT NULL,
    created_at TEXT NOT NULL,
    organization_id TEXT NOT NULL REFERENCES organizations(id)
);

CREATE TABLE IF NOT EXISTS deployments (
    id TEXT PRIMARY KEY,
    app_id TEXT NOT NULL REFERENCES apps(id),
    build_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    strategy TEXT NOT NULL DEFAULT 'rolling',
    started_at TEXT NOT NULL,
    finished_at TEXT,
    previous_deployment TEXT
);

CREATE TABLE IF NOT EXISTS builds (
    id TEXT PRIMARY KEY,
    app_id TEXT NOT NULL REFERENCES apps(id),
    status TEXT NOT NULL DEFAULT 'queued',
    source TEXT NOT NULL DEFAULT '{}',
    image_reference TEXT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    logs_url TEXT,
    triggered_by TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS volumes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'creating',
    size_gb INTEGER NOT NULL DEFAULT 0,
    used_gb INTEGER NOT NULL DEFAULT 0,
    volume_type TEXT NOT NULL DEFAULT 'persistent',
    filesystem TEXT NOT NULL DEFAULT 'ext4',
    encrypted INTEGER NOT NULL DEFAULT 0,
    encryption_key_id TEXT,
    attached_to TEXT,
    mount_path TEXT,
    snapshots TEXT NOT NULL DEFAULT '[]',
    backup_policy TEXT,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS domains (
    id TEXT PRIMARY KEY,
    hostname TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    tls_status TEXT NOT NULL DEFAULT 'none',
    certificate TEXT,
    validation TEXT,
    routing TEXT NOT NULL DEFAULT '{}',
    features TEXT NOT NULL DEFAULT '{}',
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS managed_databases (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    engine TEXT NOT NULL DEFAULT 'postgres',
    version TEXT NOT NULL DEFAULT '15',
    status TEXT NOT NULL DEFAULT 'provisioning',
    endpoint TEXT NOT NULL DEFAULT '{}',
    resources TEXT NOT NULL DEFAULT '{}',
    ha TEXT NOT NULL DEFAULT '{}',
    backup_config TEXT NOT NULL DEFAULT '{}',
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS secrets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT 'organization',
    app_id TEXT,
    current_version INTEGER NOT NULL DEFAULT 1,
    versions TEXT NOT NULL DEFAULT '[]',
    last_used_at TEXT,
    expires_at TEXT,
    encrypted_value TEXT NOT NULL DEFAULT '',
    key_id TEXT,
    nonce TEXT,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    org_id TEXT,
    actor_id TEXT NOT NULL,
    actor_type TEXT NOT NULL DEFAULT 'user',
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    changes TEXT,
    metadata TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS team_members (
    user_id TEXT NOT NULL REFERENCES users(id),
    org_id TEXT NOT NULL REFERENCES organizations(id),
    role TEXT NOT NULL DEFAULT 'developer',
    joined_at TEXT NOT NULL,
    PRIMARY KEY (user_id, org_id)
);

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    scopes TEXT NOT NULL DEFAULT '[]',
    last_used_at TEXT,
    expires_at TEXT,
    created_at TEXT NOT NULL
);
