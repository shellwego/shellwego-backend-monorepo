-- 002_add_indexes.sql
-- Performance indexes for common query patterns
-- Compatible with both SQLite and PostgreSQL

CREATE INDEX IF NOT EXISTS idx_apps_organization_id ON apps(organization_id);
CREATE INDEX IF NOT EXISTS idx_apps_status ON apps(status);
CREATE INDEX IF NOT EXISTS idx_apps_slug ON apps(slug, organization_id);
CREATE INDEX IF NOT EXISTS idx_nodes_organization_id ON nodes(organization_id);
CREATE INDEX IF NOT EXISTS idx_nodes_status ON nodes(status);
CREATE INDEX IF NOT EXISTS idx_nodes_region ON nodes(region);
CREATE INDEX IF NOT EXISTS idx_volumes_organization_id ON volumes(organization_id);
CREATE INDEX IF NOT EXISTS idx_volumes_attached_to ON volumes(attached_to);
CREATE INDEX IF NOT EXISTS idx_domains_hostname ON domains(hostname);
CREATE INDEX IF NOT EXISTS idx_domains_organization_id ON domains(organization_id);
CREATE INDEX IF NOT EXISTS idx_secrets_organization_id ON secrets(organization_id);
CREATE INDEX IF NOT EXISTS idx_secrets_app_id ON secrets(app_id);
CREATE INDEX IF NOT EXISTS idx_secrets_name ON secrets(name, organization_id);
CREATE INDEX IF NOT EXISTS idx_builds_app_id ON builds(app_id);
CREATE INDEX IF NOT EXISTS idx_builds_status ON builds(status);
CREATE INDEX IF NOT EXISTS idx_deployments_app_id ON deployments(app_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_actor_id ON audit_logs(actor_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_resource ON audit_logs(resource_type, resource_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_timestamp ON audit_logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_organization_id ON users(organization_id);
CREATE INDEX IF NOT EXISTS idx_managed_databases_organization_id ON managed_databases(organization_id);
