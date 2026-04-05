-- 004_add_agent_state.sql
-- Agent state tables: heartbeats, task assignments
-- Compatible with both SQLite and PostgreSQL

CREATE TABLE IF NOT EXISTS agent_heartbeats (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'ready',
    cpu_usage REAL NOT NULL DEFAULT 0,
    memory_usage REAL NOT NULL DEFAULT 0,
    disk_usage REAL NOT NULL DEFAULT 0,
    running_vms INTEGER NOT NULL DEFAULT 0,
    reported_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS task_assignments (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL,
    task_type TEXT NOT NULL,
    task_payload TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'pending',
    assigned_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    error_message TEXT,
    retries INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_agent_heartbeats_node ON agent_heartbeats(node_id);
CREATE INDEX IF NOT EXISTS idx_agent_heartbeats_reported ON agent_heartbeats(reported_at);
CREATE INDEX IF NOT EXISTS idx_task_assignments_node ON task_assignments(node_id);
CREATE INDEX IF NOT EXISTS idx_task_assignments_status ON task_assignments(status);
