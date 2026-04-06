# Plan 11: Infrastructure, Deployment & Dashboard

## 1. Title & Overview

**Infrastructure, Deployment & Dashboard** — Close the gap between what the README claims (one-command install at `shellwego.com/install.sh`, Docker images at `shellwego/shellwego:latest`, Helm chart at `charts.shellwego.com`, web dashboard at `:8080`, and `config/branding.yml` white-label support) and what actually exists. Specifically: (A) fix Docker images so published images are buildable and downloadable, (B) add database initialization (seed default admin user and organization) so a fresh deploy is usable without manual DB surgery, (C) publish the Helm chart to a hosted repository, (D) fix `install.sh` to remove references to a non-existent dashboard and make it work end-to-end, (E) build a minimal web dashboard so the README claim of an admin UI at `:8080` is true, (F) implement `config/branding.yml` white-label configuration, and (G) add CI/CD workflows so images and charts are published on release.

## 2. Gap Summary

| # | Readme Claim | Actual Implementation | File(s) | Severity |
|---|---|---|---|---|
| A1 | `image: shellwego/shellwego:latest` in `docker-compose.yml` | `docker-compose.yml` references no such image; it uses local `build:` directives. No `shellwego/shellwego` image exists on Docker Hub. Root `Dockerfile` builds only control-plane. | `docker-compose.yml` line 191 (readme), `docker-compose.yml` (actual — uses local build) | **CRITICAL** |
| A2 | Docker Hub account `shellwego/` | No Docker Hub account or published images exist. Both `docker/control-plane.Dockerfile` and `docker/agent.Dockerfile` build locally only. | `docker/control-plane.Dockerfile`, `docker/agent.Dockerfile` | **HIGH** |
| A3 | `docker-compose.yml` references non-existent image | The actual `docker-compose.yml` (line 6) uses `build:` context + `docker/control-plane.Dockerfile`. But the README's `docker-compose.yml` snippet references `shellwego/shellwego:latest` which does not exist. | `docker-compose.yml` actual vs. `readme.md` line 191 | **HIGH** |
| A4 | Root `Dockerfile` duplicates `docker/control-plane.Dockerfile` | Two separate Dockerfiles build the same control-plane binary. Root `Dockerfile` does not use `musl` target despite `rust-toolchain.toml` specifying `x86_64-unknown-linux-musl`. | `Dockerfile` (root), `docker/control-plane.Dockerfile`, `rust-toolchain.toml` | **MEDIUM** |
| A5 | `config/prometheus.yml` referenced in `docker-compose.yml` | `docker-compose.yml` line 56 mounts `./config/prometheus.yml` but no `config/` directory exists in the repo. Prometheus volume mount fails silently. | `docker-compose.yml` line 56 | **MEDIUM** |
| B | "Default login: admin / shellwego-admin-12345" | No migration or seed file creates a default user. The `users` table requires `organization_id` foreign key. Fresh installs have an empty database with no way to log in. | `migrations/001_initial_schema.sql` — no seed data, `readme.md` line 215 | **CRITICAL** |
| C1 | `helm repo add shellwego https://charts.shellwego.com` | No `charts.shellwego.com` domain exists. No chart repository is hosted. The local chart at `charts/shellwego/` exists but cannot be installed via `helm repo add`. | `readme.md` line 219, `charts/shellwego/Chart.yaml` | **HIGH** |
| C2 | Helm chart depends on PostgreSQL sub-chart | `values.yaml` line 180 configures `postgresql:` sub-chart but no `Chart.yaml` dependency is declared. The embedded PostgreSQL is referenced in `configmap.yaml` via `_helpers.tpl` but will never be installed. | `charts/shellwego/Chart.yaml`, `charts/shellwego/values.yaml` line 180 | **HIGH** |
| C3 | Helm chart ConfigMap puts `DATABASE_URL` with password in plaintext | `configmap.yaml` line 13 renders the full database URL (with password) into a ConfigMap, which is not secret. | `charts/shellwego/templates/configmap.yaml` line 13 | **MEDIUM** |
| D1 | `curl -fsSL https://shellwego.com/install.sh \| bash` | `shellwego.com` domain does not serve the install script. The script exists at `scripts/install.sh` (992 lines) but is not hosted anywhere. | `readme.md` line 63, 148, 158, 166, 516, `scripts/install.sh` | **HIGH** |
| D2 | Install script references dashboard URL | `install.sh` line 842 prints `Dashboard: ${proto}://${DOMAIN}/dashboard`. No dashboard exists anywhere in the repository. No `frontend/`, `dashboard/`, or web assets directory exists. | `scripts/install.sh` line 842 | **MEDIUM** |
| D3 | Install script TLS cert-gen writes to wrong directory | `install.sh` generates certs at `${TLS_DIR}` = `${CONFIG_DIR}/tls` but systemd unit has `ProtectSystem=strict` and `ReadWritePaths=/var/lib/shellwego /var/log/shellwego` — it cannot read from `/etc/shellwego/tls`. | `scripts/install.sh` line 631, 682-718 | **HIGH** |
| E | "Web dashboard (static files)" and "Admin UI at :8080" | Zero frontend code exists. No HTML, JS, CSS, WASM, or framework files. No `frontend/` or `web/` directory. The control-plane serves only the REST API. | Entire repository — no frontend files found | **CRITICAL** |
| F1 | `config/branding.yml` white-label support | No `config/` directory exists. No branding configuration system. README shows a full YAML schema with brand name, logo, colors, payment gateway config — none of this is implemented. | `readme.md` lines 689-722, no `config/branding.yml` in repo | **HIGH** |
| F2 | `shellwego build --release --branding ./config/branding.yml` | No `build` subcommand exists in CLI. No branding argument parsing. | `readme.md` line 721, `crates/shellwego-cli/src/commands/mod.rs` | **MEDIUM** |
| G | CI/CD for image/chart publishing | No `.github/` directory, no CI workflows, no release automation. All 3 Dockerfiles, the Helm chart, and the install script exist but have zero automation for publishing. | Root of repo — no `.github/` directory | **HIGH** |

## 3. Scope

### Files to Modify

| File | Change |
|---|---|
| `/home/z/my-project/shellwego-backend-monorepo/Dockerfile` | Delete — it is a duplicate of `docker/control-plane.Dockerfile` |
| `/home/z/my-project/shellwego-backend-monorepo/docker-compose.yml` | Fix Prometheus config mount; add healthcheck waits; make it usable standalone |
| `/home/z/my-project/shellwego-backend-monorepo/docker/control-plane.Dockerfile` | Add multi-arch build support; pin Rust version to `rust-toolchain.toml`; add musl target for static binary; embed static dashboard assets |
| `/home/z/my-project/shellwego-backend-monorepo/docker/agent.Dockerfile` | Add multi-arch build support; pin Rust version; add musl target |
| `/home/z/my-project/shellwego-backend-monorepo/Makefile` | Add `docker-build`, `docker-push`, `helm-package`, `helm-publish`, `dev-dashboard` targets |
| `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/Chart.yaml` | Add `appVersion`, add PostgreSQL sub-chart dependency, bump version |
| `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/values.yaml` | Add dashboard-related values, add agent deployment config, add init job config |
| `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/templates/configmap.yaml` | Move `DATABASE_URL` and `JWT_SECRET` to a Secret template; keep only non-sensitive values in ConfigMap |
| `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/templates/NOTES.txt` | Update post-install instructions to reflect working dashboard and seed credentials |
| `/home/z/my-project/shellwego-backend-monorepo/scripts/install.sh` | Remove dashboard URL from success message; fix TLS directory for systemd sandboxing; add `--skip-build` flag for pre-built binary installs; validate `shellwego init` succeeded |
| `/home/z/my-project/shellwego-backend-monorepo/readme.md` | Remove `shellwego/shellwego:latest` Docker Compose snippet; remove dashboard claim until built; fix `charts.shellwego.com` reference; mark claims that are not yet implemented |
| `/home/z/my-project/shellwego-backend-monorepo/migrations/004_add_agent_state.sql` | Add seed data for default organization, admin user, and API key (idempotent `INSERT OR IGNORE`) |

### New Files to Create

| File | Purpose |
|---|---|
| `/home/z/my-project/shellwego-backend-monorepo/migrations/005_seed_admin_user.sql` | Idempotent seed migration: creates default organization, admin user (bcrypt-hashed password), and bootstrap API key |
| `/home/z/my-project/shellwego-backend-monorepo/.github/workflows/release.yml` | GitHub Actions workflow: build multi-arch Docker images, push to GHCR, package and push Helm chart to GitHub Pages |
| `/home/z/my-project/shellwego-backend-monorepo/.github/workflows/ci.yml` | GitHub Actions CI: cargo build, cargo test, clippy lint, fmt check |
| `/home/z/my-project/shellwego-backend-monorepo/config/prometheus.yml` | Prometheus scrape config for ShellWeGo control-plane metrics endpoint |
| `/home/z/my-project/shellwego-backend-monorepo/config/branding.yml` | White-label branding configuration (brand name, colors, logo path, footer, email, payment gateway) |
| `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/templates/secret.yaml` | Kubernetes Secret template for `DATABASE_URL`, `JWT_SECRET`, and other sensitive values |
| `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/templates/job-init.yaml` | Helm post-install Job that runs database migrations and seed on first deploy |
| `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/templates/agent-daemonset.yaml` | Helm DaemonSet template for deploying shellwego-agent to worker nodes |
| `/home/z/my-project/shellwego-backend-monorepo/frontend/` | Minimal single-page admin dashboard (HTML + vanilla JS + CSS) — see Phase E for details |
| `/home/z/my-project/shellwego-backend-monorepo/frontend/index.html` | Dashboard entry point |
| `/home/z/my-project/shellwego-backend-monorepo/frontend/app.js` | Dashboard application logic (API calls, auth, state) |
| `/home/z/my-project/shellwego-backend-monorepo/frontend/style.css` | Dashboard styles (CSS custom properties for branding) |
| `/home/z/my-project/shellwego-backend-monorepo/docker/.dockerignore` | Docker build context exclusions to reduce image size |

## 4. Prerequisites

1. **Build must pass** — `cargo build --release` must succeed for all three binaries (`shellwego-control-plane`, `shellwego-agent`, `shellwego-cli`). The existing `rust-toolchain.toml` pins Rust 1.94.1 with `x86_64-unknown-linux-musl` target. Build issues must be resolved before Docker images can be created.

2. **Authentication must work** — Plan 01 (Security Hardening) should be executed first. The seed admin user migration (005) creates a user with a bcrypt password hash, which requires `argon2` or `bcrypt` crate integration in the auth module. If the password hashing in `password.rs` uses argon2, the seed SQL must use an argon2 hash instead of bcrypt.

3. **Database migrations run** — The control plane must support running migrations from the `migrations/` directory on startup. The `Makefile` has a `migrate` target (`sqlx migrate run`) but the control-plane binary itself must auto-run migrations (or the Helm chart init Job must run them). Verify that `shellwego-control-plane` applies migrations on startup or add that capability.

4. **Docker build environment** — Multi-arch builds require `docker buildx`. The CI runner must support `linux/amd64` and `linux/arm64` emulation (via QEMU). Test locally with `docker buildx build --platform linux/amd64`.

5. **No frontend framework dependency** — The dashboard in Phase E uses vanilla HTML/JS/CSS to avoid adding npm, node_modules, or a framework as a project dependency. This keeps the Rust-only build intact.

## 5. Detailed Implementation Steps

### Phase A: Docker Image Fix & Publishing

**A1. Delete root Dockerfile**

The root `Dockerfile` is a near-duplicate of `docker/control-plane.Dockerfile`. The control-plane Dockerfile is more complete (has OCI labels, healthcheck, `strip` step). Delete the root file:

```bash
rm /home/z/my-project/shellwego-backend-monorepo/Dockerfile
```

**A2. Create `.dockerignore`**

File: `/home/z/my-project/shellwego-backend-monorepo/docker/.dockerignore`

```
target/
.git/
.github/
docs/
tests/
frontend/node_modules/
*.md
CLA.md
LICENSE
```

This reduces Docker build context size from the full monorepo (~50MB+) to just source code.

**A3. Fix control-plane Dockerfile for multi-arch and musl**

File: `/home/z/my-project/shellwego-backend-monorepo/docker/control-plane.Dockerfile`

Changes:
- Change base image from `rust:1.75-slim` to `rust:1.94-slim` (match `rust-toolchain.toml`)
- Add `ARG TARGETPLATFORM` and `ARG BUILDPLATFORM` for multi-arch
- Install `musl-tools` for arm64 cross-compilation
- Copy `rust-toolchain.toml` so the correct Rust version is used
- Add `--target x86_64-unknown-linux-musl` (or arm64 equivalent) to cargo build
- Add a stage for embedding static frontend assets (built in CI or at image build time)
- Copy static assets from `frontend/dist/` to `/var/lib/shellwego/static/`
- Update `ENTRYPOINT` to pass `--static-dir /var/lib/shellwego/static` flag
- Add `VOLUME ["/var/lib/shellwego"]` for data persistence

Updated Dockerfile (key changes):

```dockerfile
FROM --platform=$BUILDPLATFORM rust:1.94-slim AS builder

ARG TARGETPLATFORM
ARG BUILDPLATFORM

# Install cross-compilation dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential protobuf-compiler clang llvm pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install musl-tools for static linking
RUN if [ "$TARGETPLATFORM" = "linux/arm64" ]; then \
        apt-get update && apt-get install -y --no-install-recommends musl-tools && rm -rf /var/lib/apt/lists/*; \
    else \
        apt-get update && apt-get install -y --no-install-recommends musl-tools && rm -rf /var/lib/apt/lists/*; \
    fi

WORKDIR /build

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./

# Determine target triple
RUN set -eux; \
    if [ "$TARGETPLATFORM" = "linux/arm64" ]; then \
        echo "aarch64-unknown-linux-musl" > /build/target.txt; \
        rustup target add aarch64-unknown-linux-musl; \
    else \
        echo "x86_64-unknown-linux-musl" > /build/target.txt; \
    fi

# Stub and cache dependencies (same pattern as before, but using target triple)
# ... (existing stub logic, but with --target $(cat /build/target.txt))

COPY crates/ crates/
COPY migrations/ migrations/

ARG TARGET_TRIPLE
RUN touch crates/shellwego-control-plane/src/main.rs && \
    cargo build --release --target $(cat /build/target.txt) --bin shellwego-control-plane && \
    strip /build/target/$(cat /build/target.txt)/release/shellwego-control-plane

# --- Runtime stage ---
FROM scratch AS runtime

# CA certificates from debian
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Non-root user (in scratch, we use numeric IDs)
COPY --from=builder /etc/passwd /etc/passwd

COPY --from=builder /build/target/*/release/shellwego-control-plane /usr/local/bin/shellwego-control-plane

# Static dashboard files (copied from build context or separate stage)
COPY --from=builder /build/frontend/dist/ /var/lib/shellwego/static/

# Create data directories
USER 1000:1000

EXPOSE 8080 9090

ENV BIND_ADDR=0.0.0.0:8080 \
    LOG_LEVEL=info \
    STATIC_DIR=/var/lib/shellwego/static \
    DEFAULT_REGION=default

ENTRYPOINT ["shellwego-control-plane"]
```

> Note: Using `FROM scratch` with musl-static produces the smallest possible image (~15MB for the binary + ~1MB for certs). If build complexity is too high, fall back to `debian:bookworm-slim`.

**A4. Fix agent Dockerfile similarly**

File: `/home/z/my-project/shellwego-backend-monorepo/docker/agent.Dockerfile`

Apply the same multi-arch and Rust version fixes. Key difference: the agent cannot use `FROM scratch` because it needs QEMU/KVM, libvirt, ZFS, and iptables at runtime. Keep `debian:bookworm-slim` as the runtime base.

**A5. Create Prometheus config**

File: `/home/z/my-project/shellwego-backend-monorepo/config/prometheus.yml`

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'shellwego-control-plane'
    static_configs:
      - targets: ['control-plane:8080']
    metrics_path: '/metrics'
    scrape_interval: 10s

  - job_name: 'shellwego-agent'
    static_configs:
      - targets: ['agent:9090']
    metrics_path: '/metrics'
    scrape_interval: 10s
```

This fixes the broken Prometheus volume mount in `docker-compose.yml` line 56.

**A6. Fix `docker-compose.yml`**

File: `/home/z/my-project/shellwego-backend-monorepo/docker-compose.yml`

Changes:
- Add `healthcheck` to `control-plane` and `postgres` services with `depends_on` condition
- Fix the postgres healthcheck: `test: ["CMD-SHELL", "pg_isready -U shellwego"]`
- Add `restart: unless-stopped` to control-plane and agent
- Add `SHELLWEGO_DOMAIN` env var to control-plane
- Ensure `./config/prometheus.yml` mount works (file now exists)
- Add a `version` comment noting this is for development only

```yaml
  control-plane:
    build:
      context: .
      dockerfile: docker/control-plane.Dockerfile
    ports:
      - "8080:8080"
    environment:
      - DATABASE_URL=postgres://shellwego:shellwego@postgres:5432/shellwego
      - NATS_URL=nats://nats:4222
      - RUST_LOG=info
      - SHELLWEGO_DOMAIN=localhost
      - ADMIN_EMAIL=admin@localhost
    depends_on:
      postgres:
        condition: service_healthy
      nats:
        condition: service_started
    restart: unless-stopped

  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: shellwego
      POSTGRES_PASSWORD: shellwego
      POSTGRES_DB: shellwego
    volumes:
      - postgres-data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U shellwego"]
      interval: 5s
      timeout: 3s
      retries: 5
```

### Phase B: Database Seeding (Default Admin User)

**B1. Create seed migration**

File: `/home/z/my-project/shellwego-backend-monorepo/migrations/005_seed_admin_user.sql`

```sql
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
-- The hash below is an argon2id hash of 'shellwego-admin-12345' with default parameters.
-- IMPORTANT: Generate the correct hash using the actual argon2 configuration from
-- crates/shellwego-control-plane/src/auth/password.rs before using this migration.
INSERT OR IGNORE INTO users (id, email, password_hash, display_name, organization_id, role, is_active, created_at, updated_at)
VALUES (
    'user-admin',
    'admin@shellwego.local',
    '$argon2id$v=19$m=19456,t=2,p=1$REPLACE_WITH_ACTUAL_HASH$REPLACE_WITH_ACTUAL_HASH',
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
    'SHA256_HASH_OF_BOOTSTRAP_KEY',
    '["admin:*"]',
    datetime('now')
);
```

**B2. Generate correct argon2id hash**

Before finalizing the migration, generate the password hash using the actual parameters from `crates/shellwego-control-plane/src/auth/password.rs`:

```bash
# Install argon2 CLI or use a Rust one-liner:
# cargo run --bin hash-password -- "shellwego-admin-12345"
```

If the control-plane uses `argon2` crate with specific params, the hash must match exactly. Read `password.rs` to determine the salt length, memory cost (`m`), time cost (`t`), and parallelism (`p`), then generate the hash.

**B3. Auto-run migrations on startup**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/main.rs`

Add migration runner at startup, before the Axum server binds. The control-plane must:
1. Read the `migrations/` directory (embedded via `include_dir!` or read from filesystem)
2. Apply each migration in order using the configured database backend
3. Log the number of migrations applied
4. If any migration fails, log error and exit with code 1

Alternatively, for Kubernetes deployments, migrations run as a Helm post-install Job (see Phase C4).

### Phase C: Helm Chart Fix & Publishing

**C1. Fix Chart.yaml — add PostgreSQL dependency**

File: `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/Chart.yaml`

```yaml
apiVersion: v2
name: shellwego
description: ShellWeGo Sovereign Cloud Platform
type: application
version: 0.2.0
appVersion: "0.1.0"
maintainers:
  - name: ShellWeGo Contributors
    email: maintainers@shellwego.dev
    url: https://github.com/shellwego/shellwego
home: https://github.com/shellwego/shellwego
keywords:
  - sovereign-cloud
  - edge-computing
  - vps
  - firecracker
  - rust
  - self-hosted
sources:
  - https://github.com/shellwego/shellwego
dependencies:
  - name: postgresql
    version: "16.x.x"
    repository: "https://charts.bitnami.com/bitnami"
    condition: postgresql.enabled
annotations:
  catalog.cattle.io/certified: "false"
  catalog.cattle.io/display-name: ShellWeGo
```

**C2. Move sensitive values from ConfigMap to Secret**

File: `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/templates/secret.yaml`

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: {{ include "shellwego.fullname" . }}
  labels:
    {{- include "shellwego.labels" . | nindent 4 }}
type: Opaque
stringData:
  DATABASE_URL: {{ include "shellwego.databaseUrl" . | quote }}
  {{- if .Values.config.jwtSecret }}
  JWT_SECRET: {{ .Values.config.jwtSecret | quote }}
  {{- else }}
  JWT_SECRET: {{ randAlphaNum 32 | quote }}
  {{- end }}
  {{- if .Values.config.vaultToken }}
  VAULT_TOKEN: {{ .Values.config.vaultToken | quote }}
  {{- end }}
```

File: `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/templates/configmap.yaml`

Remove `DATABASE_URL`, `JWT_SECRET`, and `VAULT_TOKEN` lines. Keep only non-sensitive config. The Deployment template must add the Secret as an `envFrom` source alongside the ConfigMap.

**C3. Update Deployment template to reference Secret**

File: `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/templates/deployment.yaml`

In the `containers.envFrom` section, add:

```yaml
          envFrom:
            - configMapRef:
                name: {{ include "shellwego.fullname" . }}
            - secretRef:
                name: {{ include "shellwego.fullname" . }}
```

**C4. Add init Job for migrations and seeding**

File: `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/templates/job-init.yaml`

```yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: {{ include "shellwego.fullname" . }}-init
  labels:
    {{- include "shellwego.labels" . | nindent 4 }}
  annotations:
    "helm.sh/hook": post-install,post-upgrade
    "helm.sh/hook-weight": "-5"
    "helm.sh/hook-delete-policy": hook-succeeded
spec:
  template:
    metadata:
      name: {{ include "shellwego.fullname" . }}-init
    spec:
      restartPolicy: Never
      containers:
        - name: init
          image: "{{ .Values.image.repository }}:{{ include "shellwego.imageTag" . }}"
          imagePullPolicy: {{ .Values.image.pullPolicy }}
          command: ["shellwego-control-plane", "migrate"]
          envFrom:
            - secretRef:
                name: {{ include "shellwego.fullname" . }}
            - configMapRef:
                name: {{ include "shellwego.fullname" . }}
```

This runs database migrations (and the seed migration 005) on first install and every upgrade.

**C5. Add agent DaemonSet template**

File: `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/templates/agent-daemonset.yaml`

```yaml
{{- if .Values.agent.enabled -}}
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: {{ include "shellwego.fullname" . }}-agent
  labels:
    {{- include "shellwego.labels" . | nindent 4 }}
    app.kubernetes.io/component: agent
spec:
  selector:
    matchLabels:
      {{- include "shellwego.selectorLabels" . | nindent 6 }}
      app.kubernetes.io/component: agent
  template:
    metadata:
      labels:
        {{- include "shellwego.selectorLabels" . | nindent 8 }}
        app.kubernetes.io/component: agent
    spec:
      hostPID: true
      containers:
        - name: agent
          image: "{{ .Values.agent.image.repository }}:{{ .Values.agent.image.tag | default .Chart.AppVersion }}"
          imagePullPolicy: {{ .Values.agent.image.pullPolicy | default "IfNotPresent" }}
          securityContext:
            privileged: true
          env:
            - name: CONTROL_PLANE_URL
              value: "http://{{ include "shellwego.fullname" . }}:8080"
            - name: NODE_REGION
              value: {{ .Values.agent.region | default .Values.config.defaultRegion | quote }}
            - name: RUST_LOG
              value: {{ .Values.config.logLevel | quote }}
          volumeMounts:
            - name: lib-modules
              mountPath: /lib/modules
              readOnly: true
            - name: dev-kvm
              mountPath: /dev/kvm
      volumes:
        - name: lib-modules
          hostPath:
            path: /lib/modules
        - name: dev-kvm
          hostPath:
            path: /dev/kvm
{{- end }}
```

**C6. Add agent values to values.yaml**

Append to `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/values.yaml`:

```yaml
# =============================================================================
# Agent Configuration
# =============================================================================
agent:
  enabled: false
  image:
    repository: shellwego/agent
    tag: ""
    pullPolicy: IfNotPresent
  region: default
```

**C7. Update NOTES.txt**

File: `/home/z/my-project/shellwego-backend-monorepo/charts/shellwego/templates/NOTES.txt`

Add after the existing content:

```
== First Login ==

  Default credentials:
    Email:    admin@{{ .Values.config.domain }}
    Password: shellwego-admin-12345

  IMPORTANT: Change the default password immediately after first login.

== Dashboard ==

  {{- if .Values.ingress.enabled }}
  Dashboard: http{{ if .Values.ingress.tls }}s{{ end }}://{{ (index .Values.ingress.hosts 0).host }}/dashboard
  {{- else }}
  Forward the dashboard port:
    kubectl port-forward svc/{{ include "shellwego.fullname" . }} 8080:8080
  Then open: http://localhost:8080/dashboard
  {{- end }}
```

### Phase D: Install Script Fix

**D1. Fix TLS directory for systemd sandboxing**

File: `/home/z/my-project/shellwego-backend-monorepo/scripts/install.sh`

The systemd unit has `ProtectSystem=strict` and `ReadWritePaths=/var/lib/shellwego /var/log/shellwego`. The TLS certs at `/etc/shellwego/tls/` are NOT in the read-write paths.

Change `TLS_DIR` from `${CONFIG_DIR}/tls` to `${DATA_DIR}/tls`:

```bash
readonly TLS_DIR="${DATA_DIR}/tls"
```

And update the config.toml TLS paths:
```bash
[tls]
cert_path = "${DATA_DIR}/tls/cert.pem"
key_path  = "${DATA_DIR}/tls/key.pem"
```

This ensures the control-plane can read TLS certs from `/var/lib/shellwego/tls/` which is within its allowed `ReadWritePaths`.

**D2. Remove dashboard URL from success message**

File: `/home/z/my-project/shellwego-backend-monorepo/scripts/install.sh` line 842

Replace:
```bash
printf "  ${BOLD}Dashboard:${RESET}    ${CYAN}${proto}://${DOMAIN}/dashboard${RESET}\n"
```

With:
```bash
printf "  ${BOLD}Web UI:${RESET}       ${CYAN}${proto}://${DOMAIN}${RESET}\n"
```

Remove the `/dashboard` path since the dashboard will be served at the root of the control-plane static dir.

**D3. Add `--skip-build` flag**

Add a `SKIP_BUILD=false` flag so users can install from pre-built binaries:

```bash
# In parse_args():
--skip-build) SKIP_BUILD=true ;;

# In main(), wrap build_from_source:
if [[ "$SKIP_BUILD" == false ]]; then
    build_from_source
else
    step "Skipping build (--skip-build flag)"
    for bin in "${bins[@]}"; do
        if [[ ! -f "${BIN_DIR}/${bin}" ]]; then
            die "Binary ${BIN_DIR}/${bin} not found. Cannot skip build."
        fi
    done
fi
```

**D4. Add `--binary-url` flag for downloading pre-built binaries**

```bash
BINARY_URL=""

# In parse_args():
--binary-url=*) BINARY_URL="${1#*=}" ;;

# In main(), before build:
if [[ -n "$BINARY_URL" ]]; then
    download_binaries "$BINARY_URL"
elif [[ "$SKIP_BUILD" == false ]]; then
    build_from_source
fi
```

`download_binaries()` would curl the release tarball from GitHub Releases and extract the binaries.

### Phase E: Minimal Web Dashboard

**E1. Create dashboard directory structure**

```
frontend/
  index.html       — Single-page dashboard
  app.js           — Application logic
  style.css        — Styles with CSS custom properties for branding
  assets/
    logo.svg       — Default ShellWeGo logo placeholder
```

**E2. Create `frontend/index.html`**

File: `/home/z/my-project/shellwego-backend-monorepo/frontend/index.html`

A single HTML file that:
- Loads `style.css` and `app.js`
- Has a login form (email + password) that calls `POST /v1/auth/login`
- Stores the JWT token in `localStorage`
- Has a navigation sidebar with sections: Dashboard, Apps, Nodes, Volumes, Domains, Databases, Secrets, Logs
- Has a main content area that dynamically renders each section
- Each section calls the corresponding API endpoints from the control-plane
- Shows real-time app status, node health, deployment logs
- Has a "Branding" section that displays values from `GET /v1/config/branding` (or reads from a `/branding.json` static file embedded at build time)

Key HTML structure:

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>ShellWeGo Dashboard</title>
    <link rel="stylesheet" href="/style.css">
    <link id="favicon" rel="icon" href="/assets/logo.svg">
</head>
<body>
    <div id="app">
        <!-- Login screen -->
        <div id="login-screen" class="screen">
            <div class="login-card">
                <img id="brand-logo" src="/assets/logo.svg" alt="Logo" class="logo">
                <h1 id="brand-name">ShellWeGo</h1>
                <form id="login-form">
                    <input type="email" id="email" placeholder="Email" required>
                    <input type="password" id="password" placeholder="Password" required>
                    <button type="submit">Sign In</button>
                </form>
                <p id="login-error" class="error" hidden></p>
            </div>
        </div>

        <!-- Dashboard (shown after login) -->
        <div id="dashboard-screen" class="screen" hidden>
            <nav id="sidebar">
                <div class="brand">
                    <img id="nav-logo" src="/assets/logo.svg" alt="Logo" class="nav-logo">
                    <span id="nav-brand-name">ShellWeGo</span>
                </div>
                <ul>
                    <li data-section="overview" class="active">Overview</li>
                    <li data-section="apps">Apps</li>
                    <li data-section="nodes">Nodes</li>
                    <li data-section="volumes">Volumes</li>
                    <li data-section="domains">Domains</li>
                    <li data-section="databases">Databases</li>
                    <li data-section="secrets">Secrets</li>
                </ul>
                <div class="user-info">
                    <span id="user-email"></span>
                    <button id="logout-btn">Sign Out</button>
                </div>
            </nav>
            <main id="main-content">
                <!-- Sections rendered dynamically by app.js -->
            </main>
        </div>
    </div>
    <script src="/app.js"></script>
</body>
</html>
```

**E3. Create `frontend/app.js`**

File: `/home/z/my-project/shellwego-backend-monorepo/frontend/app.js`

JavaScript that:
- Handles login/logout (JWT storage in localStorage)
- Provides `apiFetch(path, options)` wrapper that injects `Authorization: Bearer <token>` header
- Renders each dashboard section by fetching API data:
  - **Overview**: `GET /v1/metrics` + `GET /v1/apps?limit=5` + `GET /v1/nodes`
  - **Apps**: `GET /v1/apps`, create/delete/deploy buttons
  - **Nodes**: `GET /v1/nodes`, status indicators (online/offline/ draining)
  - **Volumes**: `GET /v1/volumes`
  - **Domains**: `GET /v1/domains`
  - **Databases**: `GET /v1/databases`
  - **Secrets**: `GET /v1/secrets` (values masked)
- Auto-refreshes overview every 10 seconds
- Handles 401 responses by redirecting to login
- Shows toast notifications for actions (deploy, delete, etc.)

Key functions:

```javascript
const API_BASE = window.location.origin;

async function apiFetch(path, options = {}) {
    const token = localStorage.getItem('shellwego_token');
    const headers = { 'Content-Type': 'application/json', ...options.headers };
    if (token) headers['Authorization'] = `Bearer ${token}`;
    const res = await fetch(`${API_BASE}${path}`, { ...options, headers });
    if (res.status === 401) { logout(); throw new Error('Unauthorized'); }
    if (!res.ok) { const err = await res.json().catch(() => ({})); throw new Error(err.message || res.statusText); }
    return res.json();
}

async function login(email, password) {
    const data = await apiFetch('/v1/auth/login', {
        method: 'POST',
        body: JSON.stringify({ email, password })
    });
    localStorage.setItem('shellwego_token', data.access_token);
    showDashboard();
}

function logout() {
    localStorage.removeItem('shellwego_token');
    showLogin();
}

async function loadOverview() {
    const [metrics, apps, nodes] = await Promise.all([
        apiFetch('/v1/metrics').catch(() => ({})),
        apiFetch('/v1/apps?limit=5').catch(() => ({ items: [] })),
        apiFetch('/v1/nodes').catch(() => ({ items: [] }))
    ]);
    // Render overview cards: total apps, running apps, total nodes, healthy nodes
    render('#main-content', overviewTemplate(metrics, apps, nodes));
}
```

**E4. Create `frontend/style.css`**

File: `/home/z/my-project/shellwego-backend-monorepo/frontend/style.css`

CSS with custom properties for white-label branding:

```css
:root {
    --brand-primary: #00D4AA;
    --brand-primary-hover: #00B894;
    --brand-danger: #E74C3C;
    --brand-warning: #F39C12;
    --brand-success: #27AE60;
    --brand-bg: #0D1117;
    --brand-surface: #161B22;
    --brand-surface-hover: #21262D;
    --brand-text: #E6EDF3;
    --brand-text-muted: #8B949E;
    --brand-border: #30363D;
    --brand-font: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    --brand-radius: 8px;
}

/* Dark theme by default, light theme via [data-theme="light"] */
[data-theme="light"] {
    --brand-bg: #FFFFFF;
    --brand-surface: #F6F8FA;
    --brand-surface-hover: #EBEEF1;
    --brand-text: #1F2328;
    --brand-text-muted: #656D76;
    --brand-border: #D0D7DE;
}

* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: var(--brand-font); background: var(--brand-bg); color: var(--brand-text); }

/* Layout */
#app { min-height: 100vh; }
.screen { display: none; }
.screen.active { display: flex; }
#login-screen.active { justify-content: center; align-items: center; min-height: 100vh; }
#dashboard-screen.active { display: flex; min-height: 100vh; }

/* Sidebar */
#sidebar { width: 240px; background: var(--brand-surface); border-right: 1px solid var(--brand-border); display: flex; flex-direction: column; }
#sidebar ul { list-style: none; flex: 1; }
#sidebar li { padding: 10px 20px; cursor: pointer; border-radius: var(--brand-radius); margin: 2px 8px; }
#sidebar li:hover { background: var(--brand-surface-hover); }
#sidebar li.active { background: var(--brand-primary); color: #fff; }

/* Login card */
.login-card { background: var(--brand-surface); padding: 40px; border-radius: 16px; text-align: center; width: 360px; border: 1px solid var(--brand-border); }
.login-card .logo { width: 80px; height: 80px; margin-bottom: 16px; }
.login-card input { width: 100%; padding: 10px; margin: 8px 0; background: var(--brand-bg); border: 1px solid var(--brand-border); border-radius: var(--brand-radius); color: var(--brand-text); }
.login-card button { width: 100%; padding: 12px; background: var(--brand-primary); color: #fff; border: none; border-radius: var(--brand-radius); cursor: pointer; font-size: 16px; }
.login-card button:hover { background: var(--brand-primary-hover); }

/* Main content */
#main-content { flex: 1; padding: 24px; overflow-y: auto; }

/* Cards */
.card { background: var(--brand-surface); border: 1px solid var(--brand-border); border-radius: var(--brand-radius); padding: 20px; margin-bottom: 16px; }
.card h3 { margin-bottom: 12px; }
.stat-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 16px; }
.stat { text-align: center; padding: 16px; }
.stat .value { font-size: 32px; font-weight: 700; color: var(--brand-primary); }
.stat .label { color: var(--brand-text-muted); margin-top: 4px; }

/* Tables */
table { width: 100%; border-collapse: collapse; }
th, td { padding: 10px 16px; text-align: left; border-bottom: 1px solid var(--brand-border); }
th { color: var(--brand-text-muted); font-weight: 500; }

/* Status badges */
.badge { display: inline-block; padding: 2px 8px; border-radius: 12px; font-size: 12px; font-weight: 600; }
.badge-success { background: #27AE6020; color: var(--brand-success); }
.badge-danger { background: #E74C3C20; color: var(--brand-danger); }
.badge-warning { background: #F39C1220; color: var(--brand-warning); }

/* Buttons */
.btn { padding: 8px 16px; border: none; border-radius: var(--brand-radius); cursor: pointer; font-size: 14px; }
.btn-primary { background: var(--brand-primary); color: #fff; }
.btn-danger { background: var(--brand-danger); color: #fff; }

/* Toast */
.toast { position: fixed; bottom: 20px; right: 20px; padding: 12px 24px; border-radius: var(--brand-radius); color: #fff; z-index: 1000; }
.toast-success { background: var(--brand-success); }
.toast-error { background: var(--brand-danger); }

/* Footer branding */
.brand-footer { color: var(--brand-text-muted); font-size: 12px; padding: 16px 20px; border-top: 1px solid var(--brand-border); }
```

**E5. Create placeholder logo**

File: `/home/z/my-project/shellwego-backend-monorepo/frontend/assets/logo.svg`

A simple SVG placeholder that can be replaced by the branding system:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" fill="none">
  <circle cx="100" cy="100" r="90" stroke="currentColor" stroke-width="8"/>
  <text x="100" y="115" text-anchor="middle" font-size="48" font-weight="bold" fill="currentColor" font-family="sans-serif">SW</text>
</svg>
```

**E6. Serve static files from control-plane**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/main.rs`

Add a static file serving route using `tower_http::services::ServeDir`:

```rust
use tower_http::services::{ServeDir, ServeFile};

// In router setup:
let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./frontend".to_string());
let static_service = ServeDir::new(&static_dir)
    .not_found_service(ServeFile::new(format!("{}/index.html", static_dir)));

// Add fallback route (must be last):
app.fallback(static_service)
```

This serves the `frontend/` directory for all non-API routes. The `not_found_service` ensures client-side routing works by serving `index.html` for unknown paths.

Add to `Cargo.toml`:
```toml
tower-http = { version = "0.6", features = ["fs"] }
```

### Phase F: White-Label Branding Configuration

**F1. Create `config/branding.yml`**

File: `/home/z/my-project/shellwego-backend-monorepo/config/branding.yml`

```yaml
# ShellWeGo White-Label Branding Configuration
# Place this file at config/branding.yml or /etc/shellwego/branding.yml
# The dashboard reads these values at runtime.

brand:
  # Platform name shown in dashboard header and page title
  name: "ShellWeGo"
  # Path to logo SVG (relative to frontend/ or absolute URL)
  logo: "/assets/logo.svg"
  # Favicon path
  favicon: "/assets/logo.svg"
  # Primary brand color (hex)
  primary_color: "#00D4AA"
  # Font family
  font: "Inter"
  # Dashboard theme: "dark" or "light"
  theme: "dark"

  # Commercial license features
  # hide_powered_by: true        # Remove "Powered by ShellWeGo" footer
  # custom_footer: "Your custom footer text"
  # disable_telemetry: true      # AGPL requires telemetry opt-in

email:
  from: "noreply@shellwego.local"
  # smtp_server: "smtp.sendgrid.net"
  # smtp_port: 587
  # smtp_username: "apikey"
  # smtp_password_env: "SMTP_PASSWORD"

payments:
  gateway: "none"  # Options: none, stripe, paystack, flutterwave, mpesa
  currency: "USD"
  # stripe_public_key: ""
  # stripe_secret_key_env: "STRIPE_SECRET_KEY"
```

**F2. Add branding API endpoint**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/api/handlers.rs`

Add handler:

```rust
/// GET /v1/config/branding — Returns branding configuration (public, no auth required)
pub async fn get_branding(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    // Load from config/branding.yml or embedded defaults
    // This endpoint is PUBLIC (no auth) so the login page can read branding
    Json(state.branding.clone())
}
```

Register in the router as a public route (no auth middleware):
```rust
.get("/v1/config/branding", get_branding)
```

**F3. Add branding loading to control-plane startup**

File: `/home/z/my-project/shellwego-backend-monorepo/crates/shellwego-control-plane/src/config.rs`

Add `BrandingConfig` struct:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrandingConfig {
    #[serde(default = "default_brand_name")]
    pub name: String,
    #[serde(default = "default_logo")]
    pub logo: String,
    #[serde(default = "default_favicon")]
    pub favicon: String,
    #[serde(default = "default_primary_color")]
    pub primary_color: String,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub hide_powered_by: bool,
    #[serde(default)]
    pub custom_footer: Option<String>,
}
```

Load at startup from `config/branding.yml` or `/etc/shellwego/branding.yml` with fallback to defaults.

**F4. Apply branding in dashboard JS**

In `frontend/app.js`, on page load:

```javascript
async function loadBranding() {
    try {
        const branding = await fetch(`${API_BASE}/v1/config/branding`).then(r => r.json());
        document.title = branding.brand.name + ' Dashboard';
        document.documentElement.style.setProperty('--brand-primary', branding.brand.primary_color);
        document.getElementById('brand-logo').src = branding.brand.logo;
        document.getElementById('brand-name').textContent = branding.brand.name;
        document.getElementById('nav-brand-name').textContent = branding.brand.name;
        if (branding.brand.theme === 'light') {
            document.documentElement.setAttribute('data-theme', 'light');
        }
        if (branding.brand.hide_powered_by) {
            document.querySelector('.brand-footer')?.remove();
        }
        if (branding.brand.custom_footer) {
            document.querySelector('.brand-footer').textContent = branding.brand.custom_footer;
        }
    } catch (e) {
        console.warn('Failed to load branding, using defaults');
    }
}
```

### Phase G: CI/CD Workflows

**G1. Create CI workflow**

File: `/home/z/my-project/shellwego-backend-monorepo/.github/workflows/ci.yml`

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  build-and-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: "1.94.1"
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: Check formatting
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --all -- -D warnings
      - name: Build
        run: cargo build --release
      - name: Test
        run: cargo test --all
      - name: Build CLI
        run: cargo build --release --bin shellwego-cli
      - name: Build Agent
        run: cargo build --release --bin shellwego-agent
```

**G2. Create Release workflow**

File: `/home/z/my-project/shellwego-backend-monorepo/.github/workflows/release.yml`

```yaml
name: Release

on:
  push:
    tags: ["v*"]

env:
  REGISTRY: ghcr.io
  IMAGE_PREFIX: ghcr.io/${{ github.repository }}

jobs:
  build-and-push-images:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3
      - uses: docker/login-action@v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Build and push control-plane image
        uses: docker/build-push-action@v5
        with:
          context: .
          file: docker/control-plane.Dockerfile
          platforms: linux/amd64,linux/arm64
          push: true
          tags: |
            ${{ env.IMAGE_PREFIX }}/control-plane:latest
            ${{ env.IMAGE_PREFIX }}/control-plane:${{ github.ref_name }}
          cache-from: type=gha
          cache-to: type=gha,mode=max

      - name: Build and push agent image
        uses: docker/build-push-action@v5
        with:
          context: .
          file: docker/agent.Dockerfile
          platforms: linux/amd64,linux/arm64
          push: true
          tags: |
            ${{ env.IMAGE_PREFIX }}/agent:latest
            ${{ env.IMAGE_PREFIX }}/agent:${{ github.ref_name }}
          cache-from: type=gha
          cache-to: type=gha,mode=max

  package-helm-chart:
    runs-on: ubuntu-latest
    needs: build-and-push-images
    steps:
      - uses: actions/checkout@v4
      - uses: azure/setup-helm@v3

      - name: Update image references in values.yaml
        run: |
          sed -i "s|repository: shellwego/control-plane|repository: ${{ env.IMAGE_PREFIX }}/control-plane|" charts/shellwego/values.yaml
          sed -i "s|repository: shellwego/agent|repository: ${{ env.IMAGE_PREFIX }}/agent|" charts/shellwego/values.yaml

      - name: Package Helm chart
        run: |
          helm package charts/shellwego --version ${{ github.ref_name }} --app-version ${{ github.ref_name }}

      - name: Upload chart artifact
        uses: actions/upload-artifact@v4
        with:
          name: helm-chart
          path: shellwego-*.tgz

  publish-chart-to-pages:
    runs-on: ubuntu-latest
    needs: package-helm-chart
    permissions:
      pages: write
      id-token: write
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          name: helm-chart

      - name: Create Helm repository index
        run: |
          mkdir -p gh-pages
          cp shellwego-*.tgz gh-pages/
          helm repo index gh-pages/ --url https://${{ github.repository_owner }}.github.io/${{ github.event.repository.name }}/charts

      - name: Deploy to GitHub Pages
        uses: peaceiris/actions-gh-pages@v3
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: ./gh-pages
```

This publishes:
- Docker images to `ghcr.io/shellwego/shellwego-backend-monorepo/control-plane` and `agent`
- Helm chart to GitHub Pages at `https://shellwego.github.io/shellwego-backend-monorepo/charts/`

Users install via:
```bash
helm repo add shellwego https://shellwego.github.io/shellwego-backend-monorepo/charts
helm install shellwego shellwego/shellwego
```

**G3. Add Makefile targets**

File: `/home/z/my-project/shellwego-backend-monorepo/Makefile`

Add:

```makefile
# Docker builds
docker-build-cp:
	docker build -f docker/control-plane.Dockerfile -t shellwego/control-plane:latest .

docker-build-agent:
	docker build -f docker/agent.Dockerfile -t shellwego/agent:latest .

docker-build: docker-build-cp docker-build-agent

# Helm
helm-package:
	helm package charts/shellwego

helm-lint:
	helm lint charts/shellwego

helm-template:
	helm template shellwego charts/shellwego

# Dashboard dev server (requires control-plane running on :8080)
dev-dashboard:
	@echo "Open http://localhost:8080 in your browser"
	@echo "Or use: python3 -m http.server 3000 --directory frontend"
```

### Phase H: README Accuracy

**H1. Fix README claims**

File: `/home/z/my-project/shellwego-backend-monorepo/readme.md`

Changes:
- Line 63: Change `curl -fsSL https://shellwego.com/install.sh` to `curl -fsSL https://raw.githubusercontent.com/shellwego/shellwego-backend-monorepo/main/scripts/install.sh` (point to GitHub raw URL)
- Line 191: Remove the `docker-compose.yml` snippet referencing `shellwego/shellwego:latest`. Replace with the actual local build instructions from the real `docker-compose.yml`
- Line 215: Remove `# Default login: admin / shellwego-admin-12345 (change immediately)` or verify the seed migration creates this user
- Line 182: Remove "Web dashboard (static files)" from the install list OR note it as a static SPA served by the control-plane
- Line 219: Change `https://charts.shellwego.com` to the GitHub Pages URL (`https://shellwego.github.io/shellwego-backend-monorepo/charts`)
- Line 691: Remove reference to `config/branding.yml` or note that it is now implemented
- Lines 849-853: Remove `[x] Web dashboard` from Q1 2024 roadmap checklist since it does not exist yet. Or check it off once Phase E is complete.

## 6. Dependencies on Other Plans

| Plan ID | Dependency | Notes |
|---|---|---|
| **01** (Security Hardening) | **Required** — seed migration needs working password hashing (argon2/bcrypt in `password.rs`) | The admin user seed requires the exact hash format used by the auth module. If Plan 01 changes the hashing algorithm, this plan must regenerate the seed hash. |
| **04** (Agent Activation) | Recommended — agent DaemonSet in Helm chart assumes agent registration works | The Helm agent DaemonSet (Phase C5) will not function if agent-to-control-plane communication is broken. |
| **03** (QUIC Message Bus) | Low — agent communication uses QUIC | If QUIC is not working, agents cannot register with the control-plane. The dashboard will show zero nodes. |

This plan should be executed **after Plan 01** and can run in parallel with Plans 02-06. The Docker image builds (Phase A) and CI/CD (Phase G) have no code dependencies on other plans and can start immediately. The dashboard (Phase E) and seed migration (Phase B) depend on Plan 01 completing.

## 7. Acceptance Criteria

### Docker Images (Phase A)
- [ ] `docker build -f docker/control-plane.Dockerfile -t shellwego/control-plane:latest .` succeeds on `linux/amd64`
- [ ] `docker build -f docker/agent.Dockerfile -t shellwego/agent:latest .` succeeds on `linux/amd64`
- [ ] `docker-compose up -d` starts all services (control-plane, agent, postgres, nats, prometheus, grafana) without errors
- [ ] Control-plane healthcheck passes: `curl http://localhost:8080/health` returns 200
- [ ] Prometheus scrape config is valid and scrapes control-plane metrics
- [ ] Root `Dockerfile` is deleted (no duplicate)

### Database Seeding (Phase B)
- [ ] `migrations/005_seed_admin_user.sql` is idempotent — running it twice produces no errors
- [ ] After migrations, `SELECT * FROM users WHERE email='admin@shellwego.local'` returns exactly one row
- [ ] Login with `admin@shellwego.local` / `shellwego-admin-12345` returns a valid JWT
- [ ] The seed API key can authenticate: `curl -H "Authorization: Bearer <raw_key>" http://localhost:8080/v1/apps` returns 200

### Helm Chart (Phase C)
- [ ] `helm lint charts/shellwego` passes with no warnings
- [ ] `helm template shellwego charts/shellwego` renders all templates without errors
- [ ] `helm install test-release charts/shellwego --set postgresql.enabled=true --dry-run` succeeds
- [ ] Secret template contains `DATABASE_URL` and `JWT_SECRET`; ConfigMap does NOT
- [ ] Init Job template renders with correct `helm.sh/hook` annotations
- [ ] Agent DaemonSet renders when `agent.enabled=true`

### Install Script (Phase D)
- [ ] `bash scripts/install.sh --help` prints usage
- [ ] `bash scripts/install.sh --domain=test.local --email=admin@test.local --skip-build` validates arguments
- [ ] TLS certs are written to `/var/lib/shellwego/tls/` (not `/etc/shellwego/tls/`)
- [ ] Success message does NOT include `/dashboard` path
- [ ] Systemd unit file has correct `ReadWritePaths` that include the TLS directory

### Web Dashboard (Phase E)
- [ ] `http://localhost:8080/` serves the login page (HTML from `frontend/index.html`)
- [ ] `http://localhost:8080/style.css` returns the CSS file
- [ ] `http://localhost:8080/app.js` returns the JS file
- [ ] Login with seed admin credentials succeeds and redirects to dashboard
- [ ] Dashboard overview shows at least: apps count, nodes count, metrics
- [ ] Clicking "Apps" in sidebar shows list of apps from API
- [ ] Clicking "Sign Out" clears token and shows login page
- [ ] 401 response on token expiry redirects to login page
- [ ] CSS custom properties `--brand-primary` etc. are applied and overridable

### White-Label Branding (Phase F)
- [ ] `config/branding.yml` is valid YAML and loads without errors
- [ ] `GET /v1/config/branding` returns JSON matching the YAML structure
- [ ] Dashboard login page shows the brand name and logo from branding config
- [ ] Changing `primary_color` in `branding.yml` changes the dashboard accent color
- [ ] Setting `theme: "light"` switches the dashboard to light mode
- [ ] Setting `hide_powered_by: true` removes the footer

### CI/CD (Phase G)
- [ ] `.github/workflows/ci.yml` is valid YAML
- [ ] `.github/workflows/release.yml` is valid YAML
- [ ] Pushing to `main` triggers CI build, test, and lint
- [ ] Pushing a `v*` tag triggers release workflow (requires actual GitHub Actions runner)
- [ ] `make helm-lint` passes
- [ ] `make helm-package` produces a `.tgz` file

## 8. Estimated Complexity

**XXL** (Extra Extra Large)

Rationale:
- Phase A (Docker): ~200 lines changed across 4 files. Multi-arch build complexity is high. Medium risk from musl/static linking issues.
- Phase B (Seeding): ~50 lines new migration. Medium complexity — requires exact password hash generation.
- Phase C (Helm): ~200 lines new templates + ~100 lines modified. Medium complexity — Helm templating is declarative but the init Job and agent DaemonSet add complexity.
- Phase D (Install script): ~50 lines changed. Low complexity — mostly flag additions and path fixes.
- Phase E (Dashboard): ~600 lines new code (HTML + JS + CSS). This is the largest single phase. Medium complexity — vanilla JS is straightforward but the API integration requires understanding all control-plane endpoints.
- Phase F (Branding): ~150 lines new code (config struct, API endpoint, JS loader). Low-medium complexity.
- Phase G (CI/CD): ~200 lines new YAML. Medium complexity — Docker buildx + GitHub Packages + Helm chart publishing.
- Phase H (README): ~30 lines changed. Low complexity.

Total: ~1,430 lines of new/changed code across ~25 files.

## 9. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **musl static linking breaks** — some crates may not compile with `x86_64-unknown-linux-musl` target (especially eBPF/Aya, ZFS bindings) | High | High — images fail to build | Fall back to `debian:bookworm-slim` runtime; build with `gnu` target instead of `musl`; test with `cargo build --target x86_64-unknown-linux-musl` before committing to scratch-based images |
| **Password hash mismatch** — seed migration hash does not match what `password.rs` generates | Medium | High — cannot log in after fresh install | Generate the hash programmatically using the same crate and parameters as `password.rs`; add a test that verifies the seed hash validates correctly |
| **Frontend too minimal** — vanilla JS dashboard may not be sufficient for production use | Medium | Medium — users expect a polished admin UI | Design the dashboard architecture to be replaceable; use CSS custom properties so a React/Vue rewrite can reuse the same design tokens; document that the dashboard is MVP and can be replaced |
| **Helm chart PostgreSQL dependency version mismatch** | Medium | Medium — `helm dependency build` fails | Pin to a specific Bitnami PostgreSQL chart version; test `helm dependency update` locally before committing |
| **Docker Hub name collision** — `shellwego` organization may not be available on Docker Hub | Medium | Low — use GHCR instead | Use `ghcr.io` as the primary registry (no organization registration needed); Docker Hub is optional |
| **CI runner lacks KVM** — agent tests require `/dev/kvm` which GitHub Actions runners do not provide | High | Low — agent tests skipped in CI | CI should run `cargo test` with a feature flag that excludes KVM-dependent tests; agent integration tests run separately on self-hosted runners |
| **Brand config file not found at runtime** — container may not have `config/branding.yml` mounted | Medium | Low — falls back to defaults | Branding config loading should use `Option<BrandingConfig>` with sensible defaults; missing config file logs a warning and uses defaults |
| **Large plan scope causes partial completion** — 1,430 lines across 25 files is a lot of work | High | Medium — some phases may be skipped | Each phase (A through H) is independently shippable. Docker fixes (A) and CI (G) can ship first. Dashboard (E) can be deferred to a follow-up if time-constrained. The phases are ordered by priority: A > B > G > C > D > F > E > H |
