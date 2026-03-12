# ShellWeGo REST API Reference

**Base URL**: `https://api.shellwego.com/v1 `  
**Protocol**: HTTPS only (TLS 1.3)  
**Content-Type**: `application/json`  
**Character Encoding**: UTF-8

---

## Authentication

All API requests require authentication via **Bearer Token** in the Authorization header.

### Authentication Header
```http
Authorization: Bearer <token>
```

### Token Types

| Token Type | Prefix | Use Case | Lifespan |
|------------|--------|----------|----------|
| **Personal Access Token** | `pat_` | User automation, CLI access | 1 year (rotatable) |
| **Service Account Token** | `sat_` | CI/CD, machine-to-machine | 90 days |
| **Session Token** | `sess_` | Web dashboard sessions | 24 hours |
| **Temporary Token** | `temp_` | Single operations (file uploads) | 15 minutes |

### Obtain Token
```bash
# Via CLI
shellwego auth login
# Returns: pat_shellwego_xxxxxxxxxxxx

# Via API (exchange credentials)
POST /v1/auth/token
{
  "email": "admin@company.com",
  "password": "your-password",
  "mfa_code": "123456",  # If MFA enabled
  "scope": ["apps:write", "nodes:read"]
}
```

### Token Response
```json
{
  "token": "pat_shellwego_xxxxxxxxxxxx",
  "type": "personal_access_token",
  "expires_at": "2025-12-31T23:59:59Z",
  "scope": ["apps:read", "apps:write", "nodes:read"],
  "permissions": {
    "resources": ["app:uuid-123", "node:*"],
    "actions": ["create", "read", "update", "delete"]
  }
}
```

### Revoke Token
```bash
DELETE /v1/auth/token/{token_id}
Authorization: Bearer pat_shellwego_xxxxxxxxxxxx
```

---

## Rate Limiting

Rate limits are enforced per token with the following headers:

| Header | Description |
|--------|-------------|
| `X-RateLimit-Limit` | Request quota per window |
| `X-RateLimit-Remaining` | Remaining requests |
| `X-RateLimit-Reset` | Unix timestamp when limit resets |
| `X-RateLimit-Retry-After` | Seconds until retry (on 429) |

### Limits by Tier

| Tier | Burst | Sustained | Window |
|------|-------|-----------|--------|
| **Free/Development** | 20 | 100 | 1 minute |
| **Starter** | 50 | 1,000 | 1 minute |
| **Growth** | 100 | 5,000 | 1 minute |
| **Enterprise** | 500 | 50,000 | 1 minute |
| **Webhooks (incoming)** | 10 | 100 | 1 second |

### Rate Limit Exceeded (429)
```json
{
  "error": "rate_limit_exceeded",
  "message": "API rate limit exceeded. Retry after 45 seconds.",
  "retry_after": 45,
  "limit": 1000,
  "window": "60s"
}
```

---

## Error Handling

All errors follow RFC 7807 (Problem Details). HTTP status codes indicate error categories.

### Error Response Format
```json
{
  "type": "https://api.shellwego.com/errors/invalid-request ",
  "title": "Invalid Request",
  "status": 400,
  "detail": "The 'memory' field must be between 64MB and 32GB.",
  "instance": "/v1/apps/app-123",
  "trace_id": "req_2v8s9d2k1m",
  "errors": [
    {
      "field": "resources.memory",
      "code": "out_of_range",
      "message": "Must be between 64MB and 32GB",
      "value": "64GB"
    }
  ]
}
```

### HTTP Status Codes

| Code | Meaning | Common Causes |
|------|---------|---------------|
| 200 | OK | Success |
| 201 | Created | Resource created successfully |
| 204 | No Content | Successful deletion |
| 400 | Bad Request | Invalid JSON, validation errors |
| 401 | Unauthorized | Missing/invalid token |
| 403 | Forbidden | Insufficient permissions |
| 404 | Not Found | Resource doesn't exist |
| 409 | Conflict | Resource already exists, state conflict |
| 422 | Unprocessable Entity | Business logic violation |
| 429 | Too Many Requests | Rate limit exceeded |
| 500 | Internal Server Error | Server-side failure |
| 503 | Service Unavailable | Maintenance or overload |

### Error Types Reference

| Type URI | HTTP Status | Description |
|----------|-------------|-------------|
| `authentication-required` | 401 | Token missing or expired |
| `insufficient-permissions` | 403 | RBAC denial |
| `resource-not-found` | 404 | UUID not found |
| `validation-failed` | 400 | Schema validation error |
| `quota-exceeded` | 429 | Resource limits reached |
| `payment-required` | 402 | Billing issue |
| `deployment-failed` | 422 | Runtime deployment error |
| `conflict` | 409 | Concurrent modification |

---

## Pagination

List endpoints support cursor-based pagination.

### Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `limit` | integer | 20 | Items per page (max 100) |
| `cursor` | string | - | Opaque cursor from previous response |
| `sort` | string | `created_at:desc` | Sort field and direction |
| `filter` | object | - | Field-specific filters |

### Pagination Response
```json
{
  "data": [...],
  "pagination": {
    "total": 150,
    "limit": 20,
    "has_more": true,
    "next_cursor": "eyJpZCI6ImFwcC0xMjMifQ==",
    "prev_cursor": null,
    "first_cursor": "eyJpZCI6ImFwcC0xIn0=",
    "last_cursor": "eyJpZCI6ImFwcC0xNTAifQ=="
  }
}
```

---

## Filtering & Sorting

### Filter Syntax
```bash
# Single filter
GET /v1/apps?filter[status]=running

# Multiple values (OR)
GET /v1/apps?filter[status]=running,paused

# Range filters
GET /v1/apps?filter[created_at][gte]=2024-01-01&filter[created_at][lte]=2024-12-31

# Nested fields
GET /v1/apps?filter[resources.memory]=512m

# Full-text search
GET /v1/apps?filter[q]=my-blog-app
```

### Sort Syntax
```bash
# Single field
GET /v1/apps?sort=created_at:desc

# Multiple fields
GET /v1/apps?sort=status:asc,updated_at:desc
```

---

## Core Resources

### 1. Apps (Applications)

Primary resource representing deployed applications.

#### Schema
```json
{
  "id": "app_uuid",
  "name": "my-blog",
  "slug": "my-blog",
  "status": "running|stopped|error|deploying|paused",
  "image": "ghost:latest",
  "command": ["node", "index.js"],
  "resources": {
    "memory": "512m",
    "cpu": "1.0",
    "disk": "10gb"
  },
  "env": {
    "DATABASE_URL": {
      "value": "postgres://...",
      "encrypted": true
    },
    "DEBUG": {
      "value": "true",
      "encrypted": false
    }
  },
  "domains": [
    {
      "hostname": "blog.example.com",
      "tls_status": "active",
      "certificate_expires": "2025-06-01"
    }
  ],
  "volumes": [
    {
      "mount_path": "/data",
      "size": "5gb",
      "type": "persistent"
    }
  ],
  "replicas": {
    "current": 2,
    "desired": 2,
    "healthy": 2
  },
  "networking": {
    "internal_dns": "my-blog.internal",
    "public": true,
    "allowed_egress": ["0.0.0.0/0"]
  },
  "health_check": {
    "path": "/health",
    "port": 8080,
    "interval": "10s",
    "timeout": "5s",
    "retries": 3
  },
  "source": {
    "type": "git|docker|tarball",
    "git": {
      "repository": "https://github.com/user/repo ",
      "branch": "main",
      "commit": "abc123"
    }
  },
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-20T14:22:00Z",
  "created_by": "user_uuid",
  "organization_id": "org_uuid",
  "tags": ["production", "blog"]
}
```

#### Endpoints

##### List Apps
```http
GET /v1/apps
```

**Query Parameters:**
- `filter[organization_id]`: Filter by org
- `filter[status]`: Filter by status
- `filter[node_id]`: Apps on specific node
- `include`: `volumes,domains,events` (eager load)

**Response:**
```json
{
  "data": [
    {
      "id": "app_2v8s9d2k1m",
      "name": "api-gateway",
      "status": "running",
      "resources": {"memory": "1g", "cpu": "2.0"},
      "replicas": {"current": 3, "desired": 3, "healthy": 3}
    }
  ],
  "pagination": {...}
}
```

---

##### Create App
```http
POST /v1/apps
```

**Request Body:**
```json
{
  "name": "my-new-app",
  "image": "nginx:alpine",
  "command": ["nginx", "-g", "daemon off;"],
  "resources": {
    "memory": "256m",
    "cpu": "0.5",
    "disk": "5gb"
  },
  "env": {
    "NGINX_HOST": "example.com",
    "NGINX_PORT": "80"
  },
  "domains": ["api.example.com"],
  "volumes": [
    {
      "mount_path": "/var/log/nginx",
      "size": "1gb"
    }
  ],
  "health_check": {
    "path": "/health",
    "port": 80
  },
  "replicas": 2,
  "tags": ["production", "api"]
}
```

**Response (201):**
```json
{
  "id": "app_2v8s9d2k1m",
  "name": "my-new-app",
  "status": "deploying",
  "deployment_id": "dep_9x2k3m8s1d"
}
```

---

##### Get App
```http
GET /v1/apps/{app_id}
```

**Response:**
```json
{
  "id": "app_2v8s9d2k1m",
  "name": "my-new-app",
  "status": "running",
  "image": "nginx:alpine@sha256:abc123...",
  "runtime": {
    "microvm_version": "v1.5.0",
    "kernel_version": "5.15.0",
    "firecracker_version": "v1.5.0"
  },
  "instances": [
    {
      "id": "inst_9x2k3m8s1d",
      "node_id": "node_8s2d9k1m3x",
      "status": "healthy",
      "ip": "10.0.4.15",
      "started_at": "2024-01-20T10:00:00Z",
      "health_checks_passed": 150
    }
  ]
}
```

---

##### Update App
```http
PATCH /v1/apps/{app_id}
```

**Request Body (partial updates supported):**
```json
{
  "resources": {
    "memory": "512m",
    "cpu": "1.0"
  },
  "replicas": 4,
  "env": {
    "NEW_VAR": "value"
  }
}
```

**Behavior:**
- Zero-downtime rolling update for resource changes
- Blue-green deployment for image updates
- Environment variables hot-reloaded (no restart)

---

##### Delete App
```http
DELETE /v1/apps/{app_id}
```

**Query Parameters:**
- `force=true`: Force delete even if running (risks data loss)
- `preserve_volumes=true`: Keep persistent volumes for reuse

**Response (204):** No content on success

---

##### Start/Stop/Restart App
```http
POST /v1/apps/{app_id}/actions/start
POST /v1/apps/{app_id}/actions/stop
POST /v1/apps/{app_id}/actions/restart
```

**Request Body (restart only):**
```json
{
  "strategy": "rolling|immediate",  // Default: rolling
  "batch_size": 1                   // Number of instances at a time
}
```

---

##### Scale App
```http
POST /v1/apps/{app_id}/scale
```

**Request Body:**
```json
{
  "replicas": 5,
  "strategy": "auto",  // or "manual"
  "constraints": {
    "min": 1,
    "max": 10,
    "cpu_threshold": 70
  }
}
```

---

##### Deploy App
```http
POST /v1/apps/{app_id}/deploy
```

**Request Body:**
```json
{
  "image": "myapp:v2.0",
  "source": {
    "type": "git",
    "repository": "https://github.com/org/repo ",
    "commit": "sha256:abc123"
  },
  "strategy": "blue-green|rolling|canary",
  "health_check_timeout": "5m",
  "rollback_on_failure": true
}
```

**Response:**
```json
{
  "deployment_id": "dep_9x2k3m8s1d",
  "status": "in_progress",
  "progress": {
    "total_steps": 5,
    "completed_steps": 2,
    "current_step": "starting_health_checks"
  }
}
```

---

##### Get App Logs
```http
GET /v1/apps/{app_id}/logs
```

**Query Parameters:**
- `follow=true`: Stream logs (SSE/WebSocket upgrade)
- `since`: ISO 8601 timestamp or relative (e.g., `1h`, `24h`)
- `until`: ISO 8601 timestamp
- `tail`: Number of lines (default 100, max 10000)
- `instance_id`: Filter by specific instance
- `level`: `debug|info|warn|error`
- `search`: Full-text search query

**Response (streaming):**
```text
[2024-01-20T10:00:00Z] [INFO] [inst_9x2k3m8s1d] Server started on port 8080
[2024-01-20T10:00:01Z] [DEBUG] [inst_9x2k3m8s1d] Connected to database
```

**WebSocket Alternative:**
```http
GET /v1/apps/{app_id}/logs/stream
Upgrade: websocket
```

---

##### Get App Metrics
```http
GET /v1/apps/{app_id}/metrics
```

**Query Parameters:**
- `metric`: `cpu|memory|disk|network|requests|latency`
- `resolution`: `1m|5m|1h|1d`
- `from`/`to`: Time range

**Response:**
```json
{
  "cpu": {
    "unit": "percentage",
    "data_points": [
      {"timestamp": "2024-01-20T10:00:00Z", "value": 45.2},
      {"timestamp": "2024-01-20T10:01:00Z", "value": 47.8}
    ],
    "statistics": {
      "min": 12.1,
      "max": 89.4,
      "avg": 52.3,
      "p99": 78.9
    }
  }
}
```

---

##### Exec Command in App
```http
POST /v1/apps/{app_id}/exec
```

**Request Body:**
```json
{
  "command": ["/bin/sh", "-c", "ps aux"],
  "instance_id": "inst_9x2k3m8s1d",  // Optional, picks random if omitted
  "stdin": "",
  "tty": false,
  "timeout": "30s"
}
```

**Response:**
```json
{
  "exit_code": 0,
  "stdout": "PID USER...",
  "stderr": "",
  "execution_time_ms": 145
}
```

**Interactive Shell (WebSocket):**
```http
GET /v1/apps/{app_id}/exec/ws?command=/bin/bash
Upgrade: websocket
```

---

### 2. Nodes (Infrastructure)

Worker nodes that run applications.

#### Schema
```json
{
  "id": "node_8s2d9k1m3x",
  "hostname": "worker-01.fra1.shellwego.com",
  "status": "ready|draining|maintenance|offline",
  "region": "fra1",
  "zone": "fra1-a",
  "capacity": {
    "cpu_cores": 16,
    "memory_total": "64gb",
    "disk_total": "1tb",
    "memory_available": "32gb",
    "cpu_available": "8.0"
  },
  "usage": {
    "cpu_percent": 45.2,
    "memory_percent": 62.1,
    "disk_percent": 34.5
  },
  "running_apps": 45,
  "microvm_capacity": {
    "total": 500,
    "used": 234
  },
  "networking": {
    "internal_ip": "10.0.1.5",
    "public_ip": "49.13.123.45",
    "wireguard_pubkey": "abc123..."
  },
  "labels": {
    "gpu": "false",
    "storage": "nvme",
    "environment": "production"
  },
  "kernel_version": "5.15.0-91-generic",
  "firecracker_version": "v1.5.0",
  "agent_version": "v1.0.0",
  "last_seen": "2024-01-20T14:30:00Z",
  "created_at": "2024-01-01T00:00:00Z"
}
```

#### Endpoints

##### List Nodes
```http
GET /v1/nodes
```

**Query Parameters:**
- `filter[region]`: Filter by region
- `filter[status]`: Filter by status
- `filter[label][key]=value`: Filter by labels

---

##### Get Node
```http
GET /v1/nodes/{node_id}
```

---

##### Register Node (On-premise)
```http
POST /v1/nodes
```

**Request Body:**
```json
{
  "hostname": "my-server-01",
  "region": "custom-datacenter",
  "labels": {
    "gpu": "true",
    "ssd": "true"
  },
  "capabilities": {
    "kvm": true,
    "nested_virtualization": false
  }
}
```

**Response:**
```json
{
  "id": "node_8s2d9k1m3x",
  "join_token": "sw_join_xxxxxxxxxxxx",
  "install_script": "curl -fsSL https://shellwego.com/install.sh  | bash -s -- --token=sw_join_xxxxxxxxxxxx"
}
```

---

##### Update Node
```http
PATCH /v1/nodes/{node_id}
```

**Request Body:**
```json
{
  "labels": {
    "maintenance": "true"
  },
  "status": "draining"  // Prevent new deployments
}
```

---

##### Delete Node
```http
DELETE /v1/nodes/{node_id}
```

**Query Parameters:**
- `migrate_apps=true`: Move apps to other nodes before removal

---

##### Drain Node
```http
POST /v1/nodes/{node_id}/actions/drain
```

**Request Body:**
```json
{
  "destination_nodes": ["node_xxx", "node_yyy"],  // Optional
  "batch_size": 5,
  "timeout": "10m"
}
```

---

### 3. Volumes (Persistent Storage)

#### Schema
```json
{
  "id": "vol_3k8s2d9m1x",
  "name": "postgres-data",
  "status": "attached|detached|creating|error",
  "size": "100gb",
  "used": "45gb",
  "type": "persistent|ephemeral|shared",
  "filesystem": "ext4|zfs|xfs",
  "encrypted": true,
  "encryption_key_id": "key_xxx",
  "attached_to": "app_2v8s9d2k1m",
  "mount_path": "/var/lib/postgresql/data",
  "snapshots": [
    {
      "id": "snap_9x2k3m8s1d",
      "created_at": "2024-01-19T00:00:00Z",
      "size": "42gb"
    }
  ],
  "backup_policy": {
    "enabled": true,
    "frequency": "daily",
    "retention_days": 30,
    "destination": "s3://backups/volumes/"
  },
  "created_at": "2024-01-01T00:00:00Z"
}
```

#### Endpoints

##### List Volumes
```http
GET /v1/volumes
```

---

##### Create Volume
```http
POST /v1/volumes
```

**Request Body:**
```json
{
  "name": "app-data",
  "size": "50gb",
  "type": "persistent",
  "encrypted": true,
  "filesystem": "ext4",
  "snapshot_id": "snap_xxx"  // Optional: clone from snapshot
}
```

---

##### Attach Volume
```http
POST /v1/volumes/{volume_id}/attach
```

**Request Body:**
```json
{
  "app_id": "app_2v8s9d2k1m",
  "mount_path": "/data",
  "read_only": false
}
```

---

##### Detach Volume
```http
POST /v1/volumes/{volume_id}/detach
```

**Query Parameters:**
- `force=true`: Force detach (risks data corruption)

---

##### Create Snapshot
```http
POST /v1/volumes/{volume_id}/snapshots
```

**Request Body:**
```json
{
  "name": "pre-migration-backup",
  "description": "Before v2.0 upgrade"
}
```

---

##### Restore Snapshot
```http
POST /v1/volumes/{volume_id}/restore
```

**Request Body:**
```json
{
  "snapshot_id": "snap_9x2k3m8s1d",
  "create_new_volume": true  // If false, overwrites current
}
```

---

### 4. Domains & TLS

#### Schema
```json
{
  "id": "dom_2v8s9d2k1m",
  "hostname": "api.example.com",
  "status": "active|pending|error|expired",
  "tls_status": "active|provisioning|failed",
  "tls_certificate": {
    "issuer": "Let's Encrypt",
    "expires_at": "2025-06-01T00:00:00Z",
    "auto_renew": true
  },
  "dns_validation": {
    "type": "CNAME",
    "name": "_acme-challenge.api.example.com",
    "value": "validation.shellwego.com"
  },
  "routing": {
    "app_id": "app_2v8s9d2k1m",
    "port": 8080,
    "path": "/",  // Path-based routing
    "strip_prefix": false
  },
  "cdn_enabled": true,
  "waf_enabled": false,
  "created_at": "2024-01-15T10:30:00Z"
}
```

#### Endpoints

##### List Domains
```http
GET /v1/domains
```

---

##### Create Domain
```http
POST /v1/domains
```

**Request Body:**
```json
{
  "hostname": "api.example.com",
  "app_id": "app_2v8s9d2k1m",
  "port": 8080,
  "tls": {
    "provider": "letsencrypt",  // or "custom"
    "auto_renew": true
  },
  "cdn": {
    "enabled": true,
    "cache_ttl": "1h"
  }
}
```

---

##### Upload Custom Certificate
```http
POST /v1/domains/{domain_id}/certificate
```

**Request Body:**
```json
{
  "certificate": "-----BEGIN CERTIFICATE-----\n...",
  "private_key": "-----BEGIN PRIVATE KEY-----\n...",
  "chain": "-----BEGIN CERTIFICATE-----\n..."
}
```

---

##### Validate DNS
```http
POST /v1/domains/{domain_id}/actions/validate
```

**Response:**
```json
{
  "status": "valid",
  "records": [
    {
      "type": "A",
      "name": "api.example.com",
      "expected": "49.13.123.45",
      "actual": "49.13.123.45",
      "valid": true
    }
  ]
}
```

---

### 5. Databases (Managed)

#### Schema
```json
{
  "id": "db_2v8s9d2k1m",
  "name": "production-postgres",
  "engine": "postgres|mysql|redis|mongodb",
  "version": "15.4",
  "status": "available|creating|backing_up|maintenance",
  "endpoint": {
    "host": "db-xxx.fra1.shellwego.com",
    "port": 5432,
    "username": "db_user",
    "password": "encrypted:xxx"
  },
  "resources": {
    "storage": "100gb",
    "memory": "4gb",
    "cpu": "2.0"
  },
  "usage": {
    "storage_used": "45gb",
    "connections_active": 12,
    "connections_max": 100
  },
  "backup_policy": {
    "enabled": true,
    "frequency": "daily",
    "retention": "30d",
    "time": "02:00"
  },
  "high_availability": {
    "enabled": true,
    "replica_regions": ["fra2", "ams1"]
  },
  "created_at": "2024-01-15T10:30:00Z"
}
```

#### Endpoints

##### List Databases
```http
GET /v1/databases
```

---

##### Create Database
```http
POST /v1/databases
```

**Request Body:**
```json
{
  "name": "user-service-db",
  "engine": "postgres",
  "version": "15",
  "resources": {
    "storage": "50gb",
    "memory": "2gb",
    "cpu": "1.0"
  },
  "high_availability": {
    "enabled": true,
    "replica_count": 2
  },
  "backup_policy": {
    "enabled": true,
    "frequency": "daily"
  }
}
```

---

##### Get Connection String
```http
GET /v1/databases/{db_id}/connection
```

**Response:**
```json
{
  "uri": "postgres://user:pass@host:5432/db?sslmode=require",
  "components": {
    "host": "db-xxx.fra1.shellwego.com",
    "port": 5432,
    "username": "db_user",
    "password": "auto_generated_password",
    "database": "db_name"
  }
}
```

---

##### Create Backup
```http
POST /v1/databases/{db_id}/backups
```

---

##### Restore Backup
```http
POST /v1/databases/{db_id}/restore
```

**Request Body:**
```json
{
  "backup_id": "bkup_9x2k3m8s1d",
  "point_in_time": "2024-01-20T10:00:00Z"  // Optional PITR
}
```

---

### 6. Organizations & Teams

#### Schema
```json
{
  "id": "org_2v8s9d2k1m",
  "name": "Acme Corp",
  "slug": "acme-corp",
  "plan": "growth",
  "quotas": {
    "apps": 100,
    "nodes": 10,
    "memory_total": "1tb",
    "volumes_total": "10tb"
  },
  "usage": {
    "apps": 45,
    "nodes": 5,
    "memory_used": "512gb"
  },
  "billing": {
    "email": "billing@acme.com",
    "address": {...},
    "payment_method": "card_xxx"
  },
  "settings": {
    "enforce_mfa": true,
    "allowed_regions": ["fra1", "ams1"],
    "audit_log_retention": "1y"
  },
  "created_at": "2024-01-01T00:00:00Z"
}
```

#### Endpoints

##### Get Organization
```http
GET /v1/organizations/{org_id}
```

---

##### List Members
```http
GET /v1/organizations/{org_id}/members
```

---

##### Invite Member
```http
POST /v1/organizations/{org_id}/invitations
```

**Request Body:**
```json
{
  "email": "newuser@example.com",
  "role": "admin|developer|viewer|billing",
  "resources": ["app:*", "node:read"],  // Optional RBAC restrictions
  "expires_in": "7d"
}
```

---

##### Update Member Role
```http
PATCH /v1/organizations/{org_id}/members/{user_id}
```

---

##### Remove Member
```http
DELETE /v1/organizations/{org_id}/members/{user_id}
```

---

### 7. Users

#### Endpoints

##### Get Current User
```http
GET /v1/user
```

**Response:**
```json
{
  "id": "usr_2v8s9d2k1m",
  "email": "admin@example.com",
  "name": "John Doe",
  "avatar_url": "https://...",
  "mfa_enabled": true,
  "organizations": [
    {
      "id": "org_xxx",
      "name": "Acme Corp",
      "role": "owner"
    }
  ],
  "api_quotas": {
    "requests_per_minute": 5000,
    "apps_limit": 100
  }
}
```

---

##### Update User
```http
PATCH /v1/user
```

---

##### Generate API Token
```http
POST /v1/user/tokens
```

**Request Body:**
```json
{
  "name": "CI/CD Pipeline",
  "scope": ["apps:read", "apps:write", "deployments:write"],
  "expires_in": "90d",
  "restrict_to_apps": ["app_xxx", "app_yyy"]  // Optional
}
```

---

##### List API Tokens
```http
GET /v1/user/tokens
```

---

##### Revoke API Token
```http
DELETE /v1/user/tokens/{token_id}
```

---

### 8. Deployments

#### Schema
```json
{
  "id": "dep_9x2k3m8s1d",
  "app_id": "app_2v8s9d2k1m",
  "status": "in_progress|successful|failed|rolled_back",
  "version": "git:abc123",
  "image": "myapp:v2.0",
  "strategy": "blue-green|rolling|canary",
  "initiated_by": "usr_2v8s9d2k1m",
  "stages": [
    {
      "name": "building",
      "status": "completed",
      "started_at": "2024-01-20T10:00:00Z",
      "completed_at": "2024-01-20T10:02:30Z",
      "logs_url": "https://..."
    },
    {
      "name": "health_checks",
      "status": "in_progress",
      "started_at": "2024-01-20T10:02:35Z"
    }
  ],
  "metrics": {
    "build_time_ms": 150000,
    "deploy_time_ms": 45000
  },
  "rollback_target": "dep_8s2d9k1m3x",  // Previous deployment
  "created_at": "2024-01-20T10:00:00Z"
}
```

#### Endpoints

##### List Deployments
```http
GET /v1/apps/{app_id}/deployments
```

---

##### Get Deployment
```http
GET /v1/deployments/{deployment_id}
```

---

##### Rollback Deployment
```http
POST /v1/deployments/{deployment_id}/actions/rollback
```

**Request Body:**
```json
{
  "strategy": "immediate",  // or "gradual"
  "reason": "Critical bug in v2.0"
}
```

---

##### Get Deployment Logs
```http
GET /v1/deployments/{deployment_id}/logs
```

---

### 9. Events & Audit Logs

#### Schema
```json
{
  "id": "evt_2v8s9d2k1m",
  "type": "app.deployed|node.offline|user.login|volume.attached",
  "severity": "info|warning|error|critical",
  "actor": {
    "type": "user|token|system",
    "id": "usr_2v8s9d2k1m",
    "ip": "203.0.113.45"
  },
  "resource": {
    "type": "app",
    "id": "app_2v8s9d2k1m"
  },
  "metadata": {
    "previous_status": "stopped",
    "new_status": "running",
    "deployment_id": "dep_9x2k3m8s1d"
  },
  "timestamp": "2024-01-20T14:30:00Z"
}
```

#### Endpoints

##### List Events
```http
GET /v1/events
```

**Query Parameters:**
- `filter[type]`: Event type filter
- `filter[severity]`: Severity filter
- `filter[resource_type]`: Resource type
- `filter[resource_id]`: Specific resource

---

##### Subscribe to Events (SSE)
```http
GET /v1/events/stream
Accept: text/event-stream
```

**Event Format:**
```text
event: app.deployed
id: evt_2v8s9d2k1m
data: {"app_id": "app_xxx", "status": "running"}

event: node.offline
id: evt_9x2k3m8s1d
data: {"node_id": "node_xxx", "last_seen": "..."}
```

---

### 10. Secrets

#### Schema
```json
{
  "id": "sec_2v8s9d2k1m",
  "name": "DATABASE_URL",
  "scope": "organization|app",
  "app_id": "app_xxx",  // If app-scoped
  "version": 3,
  "versions": [
    {
      "version": 3,
      "created_at": "2024-01-20T10:00:00Z",
      "created_by": "usr_xxx"
    }
  ],
  "last_used": "2024-01-20T14:30:00Z",
  "expires_at": null,
  "created_at": "2024-01-15T10:30:00Z"
}
```

#### Endpoints

##### List Secrets
```http
GET /v1/secrets
```

---

##### Create Secret
```http
POST /v1/secrets
```

**Request Body:**
```json
{
  "name": "STRIPE_API_KEY",
  "value": "sk_live_xxxxxxxx",
  "scope": "app",
  "app_id": "app_2v8s9d2k1m",
  "expires_at": "2025-01-01T00:00:00Z"  // Optional rotation
}
```

**Response (value never returned):**
```json
{
  "id": "sec_2v8s9d2k1m",
  "name": "STRIPE_API_KEY",
  "version": 1
}
```

---

##### Update Secret (Rotate)
```http
POST /v1/secrets/{secret_id}/versions
```

**Request Body:**
```json
{
  "value": "sk_live_newkey"
}
```

---

##### Delete Secret
```http
DELETE /v1/secrets/{secret_id}
```

---

## Webhooks

ShellWeGo can send webhook notifications to your endpoints for real-time events.

### Webhook Configuration

```http
POST /v1/webhooks
```

**Request Body:**
```json
{
  "url": "https://yourapp.com/webhooks/shellwego ",
  "events": ["app.deployed", "app.crashed", "node.offline"],
  "secret": "whsec_your_signing_secret",
  "headers": {
    "X-Custom-Header": "value"
  },
  "retry_policy": {
    "max_retries": 5,
    "backoff": "exponential"
  }
}
```

### Webhook Payload

```json
{
  "id": "evt_2v8s9d2k1m",
  "type": "app.deployed",
  "timestamp": "2024-01-20T14:30:00Z",
  "data": {
    "app_id": "app_2v8s9d2k1m",
    "deployment_id": "dep_9x2k3m8s1d",
    "previous_status": "deploying",
    "current_status": "running",
    "version": "v2.0.1"
  }
}
```

### Signature Verification

```python
import hmac
import hashlib

def verify_signature(payload, signature, secret):
    expected = hmac.new(
        secret.encode(),
        payload.encode(),
        hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(f"sha256={expected}", signature)

# Header: X-ShellWeGo-Signature: sha256=abc123...
```

---

## SDKs & Tools

### Official SDKs

| Language | Package | Install |
|----------|---------|---------|
| **JavaScript/Node** | `@shellwego/sdk` | `npm install @shellwego/sdk` |
| **Python** | `shellwego-py` | `pip install shellwego` |
| **Go** | `github.com/shellwego/go-sdk` | `go get github.com/shellwego/go-sdk` |
| **Rust** | `shellwego-sdk` | `cargo add shellwego-sdk` |
| **Ruby** | `shellwego` | `gem install shellwego` |

### CLI Usage

```bash
# Install
curl -fsSL https://shellwego.com/install-cli.sh  | bash

# Authenticate
shellwego auth login

# Deploy
shellwego apps create --name myapp --image nginx:latest

# Stream logs
shellwego logs --app myapp --follow

# Scale
shellwego apps scale myapp --replicas 5
```

---

## Changelog

### v1.0.0 (2024-01-20)
- Initial stable release
- Core resources: Apps, Nodes, Volumes, Domains
- Webhook support
- SSE streaming for logs and events

### v1.1.0 (2024-02-15)
- Added Database managed service endpoints
- Secrets management API
- Enhanced audit logging

### v1.2.0 (2024-03-01)
- Canary deployment strategies
- Multi-region support
- GraphQL endpoint (beta)

---

## Support

- **API Status**: https://status.shellwego.com 
- **Documentation**: https://docs.shellwego.com/api 
- **Support**: api-support@shellwego.com
- **Bug Reports**: https://github.com/shellwego/shellwego/issues 

---

**Base URL**: `https://api.shellwego.com/v1 `  
**OpenAPI Spec**: `https://api.shellwego.com/v1/openapi.json `  
**Postman Collection**: `https://shellwego.com/docs/postman-collection.json `
```

This comprehensive REST API documentation for **ShellWeGo.com** includes:

1. **Authentication**: Multiple token types (PAT, Service Account, Session), JWT with RS256, scope-based permissions
2. **Rate Limiting**: Tier-based limits with standard headers, retry logic
3. **Error Handling**: RFC 7807 Problem Details, comprehensive error types, field-level validation errors
4. **Pagination**: Cursor-based with bidirectional navigation
5. **Filtering/Sorting**: Advanced query syntax for complex filters
6. **Core Resources**:
   - **Apps**: Full CRUD, deployment, scaling, logs (streaming), metrics, exec
   - **Nodes**: Infrastructure management, draining, registration
   - **Volumes**: Storage with snapshots, backups, attach/detach
   - **Domains**: TLS automation, custom certs, DNS validation
   - **Databases**: Managed DBaaS with HA, PITR
   - **Organizations**: Multi-tenancy, RBAC, invitations
   - **Users**: Profile management, token lifecycle
   - **Deployments**: Rollback strategies, stage tracking
   - **Events**: Audit logs, SSE streaming
   - **Secrets**: Versioned, encrypted, rotation support
7. **Webhooks**: Event subscriptions with signature verification (HMAC-SHA256)
8. **SDKs**: Official clients for 5+ languages
9. **Operational**: Changelog, status page, support contacts 