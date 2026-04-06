.PHONY: all build test lint fmt clean dev test-integration test-e2e

# Default target
all: build

# Build all crates
build:
        cargo build --release

# Build with all features
build-all:
        cargo build --release --all-features

# Run tests
test:
        cargo test --all

# Run integration tests (requires KVM)
test-integration:
        cargo test -p shellwego-integration-tests --features integration-tests -- --test-threads=1

# Run E2E test (requires running control plane)
test-e2e:
        bash tests/e2e/deploy_test.sh

# Lint
lint:
        cargo clippy --all -- -D warnings

# Format
fmt:
        cargo fmt --all

# Clean
clean:
        cargo clean

# Development environment
dev:
        docker-compose up -d

# Stop dev environment
dev-stop:
        docker-compose down

# Run control plane locally
run-control-plane:
        cargo run --bin shellwego-control-plane

# Run agent locally (requires root for KVM)
run-agent:
        sudo cargo run --bin shellwego-agent

# Generate documentation
docs:
        cargo doc --all --no-deps --open

# Install CLI locally
install-cli:
        cargo install --path crates/shellwego-cli

# Database migrations
migrate:
        sqlx migrate run --source crates/shellwego-control-plane/migrations

# Create new migration
migrate-new:
        sqlx migrate add -s crates/shellwego-control-plane/migrations $(name)

# Security audit
audit:
        cargo audit

# Update dependencies
update:
        cargo update

# Check for outdated dependencies
outdated:
        cargo outdated

# Release build for all targets
release:
        cargo build --release --target x86_64-unknown-linux-musl
        cargo build --release --target aarch64-unknown-linux-musl

# Generate SBOM
sbom:
        bash scripts/sbom.sh

# Vulnerability scan
scan:
        bash scripts/scan.sh

# Generate JWT dev keys
jwt-keys:
        bash scripts/generate-jwt-keys.sh

# =============================================================================
# Docker builds
# =============================================================================
docker-build-cp:
        docker build -f docker/control-plane.Dockerfile -t shellwego/control-plane:latest .

docker-build-agent:
        docker build -f docker/agent.Dockerfile -t shellwego/agent:latest .

docker-build: docker-build-cp docker-build-agent

# =============================================================================
# Helm
# =============================================================================
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