.PHONY: all build test lint fmt clean dev

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
	cargo test --features integration-tests -- --test-threads=1

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