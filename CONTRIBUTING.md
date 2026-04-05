# Contributing to ShellWeGo

Thank you for your interest in contributing to ShellWeGo! This document provides the guidelines and processes you need to follow to contribute code, documentation, or other improvements to the project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Building from Source](#building-from-source)
- [Code Style](#code-style)
- [Pull Request Process](#pull-request-process)
- [Testing Requirements](#testing-requirements)
- [Commit Message Convention](#commit-message-convention)
- [Issue Reporting](#issue-reporting-guidelines)
- [Contributor License Agreement](#contributor-license-agreement)

---

## Code of Conduct

This project and everyone participating in it is governed by the [ShellWeGo Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this standard of respectful and constructive behavior.

---

## Getting Started

ShellWeGo is a sovereign cloud platform written in Rust that enables you to deploy your own PaaS infrastructure. It uses Firecracker microVMs and/or Wasmtime for workload isolation, custom eBPF programs (via Aya) for networking, and ZFS for storage.

The project is organized as a Cargo workspace with the following crates:

| Crate | Description |
|-------|-------------|
| `shellwego-control-plane` | API server, scheduler, and management plane |
| `shellwego-agent` | Worker node daemon that manages microVMs and WASM functions |
| `shellwego-network` | Custom eBPF data plane for firewalling and QoS |
| `shellwego-storage` | ZFS and S3 storage interactions |
| `shellwego-firecracker` | MicroVM lifecycle management |
| `shellwego-schema` | Shared types, entities, and API definitions |
| `shellwego-observability` | Logging, metrics, and tracing |
| `shellwego-billing` | Usage metering and invoicing |
| `shellwego-registry` | OCI container registry cache and distribution |
| `shellwego-edge` | High-performance edge proxy (Traefik replacement) |
| `shellwego-cli` | Command-line interface tool |

---

## Development Setup

### Prerequisites

| Dependency | Minimum Version | Purpose |
|-----------|----------------|---------|
| **Rust** | 1.75+ | Core language toolchain |
| **LLVM / Clang** | 15+ | eBPF program compilation (Aya framework) |
| **Protobuf compiler** (`protoc`) | 3.x | gRPC service definitions |
| **Linux kernel headers** | 5.10+ | KVM and VMM development |
| **libssl-dev** | - | TLS/HTTPS support |
| **pkg-config** | - | Build dependency discovery |

### Installing Prerequisites on Ubuntu/Debian

```bash
# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Ensure the correct toolchain
rustup install 1.75
rustup default 1.75

# Install system dependencies
sudo apt-get update
sudo apt-get install -y \
  llvm-dev \
  libclang-dev \
  clang \
  protobuf-compiler \
  libssl-dev \
  pkg-config \
  linux-headers-$(uname -r) \
  build-essential
```

### Installing Prerequisites on Fedora/RHEL

```bash
# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Install system dependencies
sudo dnf install -y \
  llvm-devel \
  clang-devel \
  clang \
  protobuf-compiler \
  openssl-devel \
  pkgconfig \
  kernel-devel \
  gcc
```

### Verifying Your Setup

```bash
# Verify Rust toolchain
rustc --version    # Should be 1.75.0 or newer
cargo --version

# Verify clippy and rustfmt
cargo clippy --version
cargo fmt --version

# Verify system dependencies
llvm-config --version
protoc --version
```

---

## Building from Source

```bash
# Clone the repository
git clone https://github.com/shellwego/shellwego.git
cd shellwego

# Build the entire workspace (debug mode)
cargo build --all

# Build in release mode with optimizations
cargo build --release --all

# Build a specific binary
cargo build --release --bin shellwego-control-plane
cargo build --release --bin shellwego-agent
cargo build --release --bin shellwego-cli

# Build static binaries for deployment (requires musl target)
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl --bin shellwego-agent
```

---

## Code Style

ShellWeGo enforces strict code style rules to maintain consistency and quality across the codebase.

### Formatting

We use **rustfmt** with the default configuration. All code must pass formatting checks:

```bash
# Check formatting (CI will fail if this does)
cargo fmt --all -- --check

# Auto-format code
cargo fmt --all
```

### Linting

We use **Clippy** with `pedantic` lints enabled. All warnings are treated as errors:

```bash
# Run clippy with all warnings denied
cargo clippy --all -- -D warnings
```

The workspace `Cargo.toml` configures Clippy at the lints level:

```toml
[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
```

The `clippy.toml` file in the repository root defines additional configuration:
- `msrv = "1.75"` (minimum supported Rust version)
- `cognitive-complexity-threshold = 50`
- Allowed doc identifiers for proper names (ShellWeGo, Kubernetes, Docker, etc.)

### General Guidelines

- **Prefer idiomatic Rust**: Use `Option`, `Result`, pattern matching, and iterators over imperative loops.
- **Error handling**: Use the `thiserror` crate for library error types and `anyhow` for application-level errors.
- **Async**: Use `tokio` as the async runtime. Prefer `async/await` over manual futures.
- **Documentation**: Public items must have doc comments. Use `///` for item documentation and `//!` for module-level documentation.
- **No `unwrap()` in library code**: Use proper error propagation. `unwrap()` is acceptable in tests and CLI entry points only.
- **Feature flags**: Keep crate dependencies behind feature flags where appropriate to reduce compile times.

---

## Pull Request Process

### 1. Fork the Repository

```bash
# Fork the repository on GitHub, then clone your fork
git clone https://github.com/<your-username>/shellwego.git
cd shellwego

# Add the upstream remote
git remote add upstream https://github.com/shellwego/shellwego.git
```

### 2. Create a Branch

```bash
# Sync with upstream
git fetch upstream
git checkout main
git merge upstream/main

# Create a feature branch
git checkout -b feature/your-feature-name
# Or for bug fixes:
git checkout -b fix/issue-description
```

Branch naming convention:
- `feature/<short-description>` — New features
- `fix/<short-description>` — Bug fixes
- `docs/<short-description>` — Documentation changes
- `refactor/<short-description>` — Code refactoring
- `perf/<short-description>` — Performance improvements
- `ci/<short-description>` — CI/CD changes

### 3. Make Your Changes

- Write clear, concise code following the style guidelines above.
- Add tests for new functionality.
- Update documentation where applicable.
- Keep commits small and focused.

### 4. Run Checks Locally

Before pushing, ensure everything passes:

```bash
# Format code
cargo fmt --all

# Run clippy
cargo clippy --all -- -D warnings

# Run all tests
cargo test --all

# Run integration tests (requires KVM access)
cargo test --features integration-tests -- --test-threads=1
```

### 5. Commit Your Changes

Follow the [Conventional Commits](#commit-message-convention) specification (see below).

### 6. Push and Open a Pull Request

```bash
# Push to your fork
git push origin feature/your-feature-name

# Open a PR on GitHub targeting the main branch
```

### Pull Request Template

When opening a PR, please include:

```markdown
## Summary

Brief description of the changes and their purpose.

## Type of Change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would break existing functionality)
- [ ] Documentation update
- [ ] Refactoring (no functional change)
- [ ] Performance improvement

## Testing

Describe the tests you ran to verify the changes:

- [ ] Unit tests pass (`cargo test --all`)
- [ ] Clippy passes (`cargo clippy --all -- -D warnings`)
- [ ] Formatting passes (`cargo fmt --all -- --check`)
- [ ] Integration tests pass (if applicable)

## Related Issues

Closes #<issue-number>
```

### Review Process

1. At least one maintainer must approve the PR before merging.
2. All CI checks must pass (formatting, clippy, tests).
3. Address review feedback promptly. If a review is stale for 30+ days, it may be closed.
4. Squash merges are the default. Preserve meaningful commit messages.

---

## Testing Requirements

### Unit Tests

Every new feature or bug fix must include unit tests. Tests should be placed in the same module as the code they test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        let result = something();
        assert!(result.is_ok());
    }
}
```

Run unit tests:

```bash
cargo test --all
```

### Integration Tests

Integration tests live in the `tests/` directory of each crate. Some integration tests require privileged access (KVM, network namespaces):

```bash
# Run integration tests (requires root and KVM)
sudo cargo test --features integration-tests -- --test-threads=1
```

### Feature Flags

When adding feature flags, ensure:

1. The feature is documented in the crate's `Cargo.toml`.
2. Code behind the feature flag is gated with `#[cfg(feature = "...")]`.
3. Tests exist both with and without the feature enabled:

```bash
# Test without the feature
cargo test --all

# Test with the feature
cargo test --all --features <feature-name>
```

### Test Coverage

- Aim for meaningful test coverage on business logic and error paths.
- Tests for trivial getters/setters are not required but welcome.
- Use `#[should_panic]` for tests that verify panic behavior.
- Use `cargo test -- --nocapture` for debugging test output.

---

## Commit Message Convention

ShellWeGo uses [Conventional Commits](https://www.conventionalcommits.org/) for all commit messages. This convention is enforced via tooling.

### Format

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | A new feature |
| `fix` | A bug fix |
| `docs` | Documentation changes only |
| `style` | Code style changes (formatting, semicolons, etc.) |
| `refactor` | Code changes that neither fix bugs nor add features |
| `perf` | Performance improvements |
| `test` | Adding or updating tests |
| `build` | Changes to build system or dependencies |
| `ci` | Changes to CI configuration |
| `chore` | Other changes (maintenance, tooling) |
| `revert` | Reverts a previous commit |

### Scopes

Use the crate name as the scope:

- `control-plane`
- `agent`
- `network`
- `storage`
- `firecracker`
- `schema`
- `observability`
- `billing`
- `registry`
- `edge`
- `cli`

### Examples

```
feat(agent): add live-migration support for microVMs

Implement the ability to migrate running Firecracker microVMs
between worker nodes with minimal downtime (<500ms).

Closes #234
```

```
fix(network): resolve eBPF program unload race condition

The firewall eBPF program could be unloaded while packets were
still being processed, causing a kernel panic. This adds a
reference counting mechanism to ensure safe unload.

Fixes #189
```

```
docs(readme): update installation instructions for PVM mode
```

```
ci(workflows): add aarch64-unknown-linux-musl to release matrix
```

### Breaking Changes

For breaking changes, append `!` after the type and include a `BREAKING CHANGE:` footer:

```
feat(control-plane)!: change API response format to envelope style

BREAKING CHANGE: All API responses now use the format
{"data": ..., "meta": {...}} instead of returning data directly.
Migration guide: docs/api-migration-v2.md
```

---

## Issue Reporting Guidelines

### Bug Reports

When reporting a bug, please include:

1. **ShellWeGo version**: `shellwego --version` or git commit hash
2. **Operating system and kernel version**: `uname -a`
3. **Rust version**: `rustc --version`
4. **Steps to reproduce**: Minimal, reproducible example
5. **Expected behavior**: What you expected to happen
6. **Actual behavior**: What actually happened
7. **Logs**: Relevant output from `journalctl -u shellwego-agent -f` or the CLI
8. **Configuration**: Redacted configuration files if relevant

### Feature Requests

When requesting a feature, please include:

1. **Use case**: Describe the problem you are trying to solve
2. **Proposed solution**: Your idea for how it should work
3. **Alternatives considered**: Other approaches you have thought about
4. **Scope**: Which crate(s) would be affected

### Security Vulnerabilities

Do **not** report security vulnerabilities via public issues. Instead, send an email to **security@shellwego.com** with PGP encryption (PGP key available on our website).

---

## Contributor License Agreement

All contributors must sign the [Contributor License Agreement (CLA)](CLA.md) before their pull request can be merged. By submitting code, you grant ShellWeGo Inc. a perpetual license to use your contributions in both open-source and commercial products.

The CLA bot will automatically check for a signed CLA when you open a pull request. If you have not yet signed, you will receive a comment with a link to do so.

---

## Questions?

- **Discord**: [discord.gg/shellwego](https://discord.gg/shellwego)
- **Forum**: [community.shellwego.com](https://community.shellwego.com)
- **Email**: contributors@shellwego.com

Thank you for contributing to ShellWeGo!
