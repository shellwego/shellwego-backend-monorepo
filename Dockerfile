# =============================================================================
# ShellWeGo - Multi-stage Dockerfile (Control Plane)
# =============================================================================
# Builds the shellwego-control-plane binary from the workspace.
# Usage:
#   docker build -t shellwego/control-plane:latest .
# =============================================================================

# ---------------------------------------------------------------------------
# Stage 1: Builder
# ---------------------------------------------------------------------------
FROM rust:1.75-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    protobuf-compiler \
    clang \
    llvm \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# 1a. Copy workspace manifests first for dependency caching
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./

# 1b. Create stub source files so cargo can resolve and cache dependencies
#     without needing the real source yet. We only care about the
#     control-plane binary.
RUN mkdir -p crates/shellwego-control-plane/src && \
    echo "fn main() {}" > crates/shellwego-control-plane/src/main.rs && \
    mkdir -p crates/shellwego-schema/src && \
    echo "" > crates/shellwego-schema/src/lib.rs && \
    for crate in shellwego-storage shellwego-registry shellwego-network \
                  shellwego-billing shellwego-observability shellwego-edge \
                  shellwego-firecracker shellwego-agent shellwego-cli; do \
        mkdir -p crates/$crate/src && echo "" > crates/$crate/src/lib.rs 2>/dev/null || true; \
    done && \
    cargo build --release --bin shellwego-control-plane 2>/dev/null || true

# 1c. Copy the real source code
COPY crates/ crates/

# 1d. Touch source files to invalidate the stub cache and rebuild with real code
RUN touch crates/shellwego-control-plane/src/main.rs && \
    cargo build --release --bin shellwego-control-plane

# ---------------------------------------------------------------------------
# Stage 2: Runtime
# ---------------------------------------------------------------------------
# NOTE: In production CI, verify image signatures before deployment:
#   cosign verify --key cosign.pub shellwego/control-plane:$TAG
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    zfs-utils \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for running the service
RUN groupadd --gid 1000 shellwego && \
    useradd --uid 1000 --gid shellwego --shell /bin/false shellwego

# Copy the compiled binary from builder
COPY --from=builder /build/target/release/shellwego-control-plane /usr/local/bin/shellwego-control-plane

# Create data directories
RUN mkdir -p /var/lib/shellwego/builds/logs && \
    mkdir -p /var/lib/shellwego/data && \
    chown -R shellwego:shellwego /var/lib/shellwego

USER shellwego

EXPOSE 8080

ENV BIND_ADDR=0.0.0.0:8080
ENV LOG_LEVEL=info

ENTRYPOINT ["shellwego-control-plane"]
