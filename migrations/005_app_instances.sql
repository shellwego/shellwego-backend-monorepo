-- 005_app_instances.sql
-- Tracks which app replicas are running on which nodes.
-- The scheduler writes a row per replica assignment;
-- the guardian updates status on health transitions.

CREATE TABLE IF NOT EXISTS app_instances (
    id TEXT PRIMARY KEY,
    app_id TEXT NOT NULL REFERENCES apps(id),
    node_id TEXT NOT NULL REFERENCES nodes(id),
    deployment_id TEXT REFERENCES deployments(id),
    status TEXT NOT NULL DEFAULT 'starting',
    internal_ip TEXT NOT NULL DEFAULT '',
    started_at TEXT NOT NULL,
    health_checks_passed INTEGER NOT NULL DEFAULT 0,
    health_checks_failed INTEGER NOT NULL DEFAULT 0,
    last_health_check TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_app_instances_app ON app_instances(app_id);
CREATE INDEX IF NOT EXISTS idx_app_instances_node ON app_instances(node_id);
CREATE INDEX IF NOT EXISTS idx_app_instances_status ON app_instances(status);
CREATE INDEX IF NOT EXISTS idx_app_instances_deployment ON app_instances(deployment_id);
