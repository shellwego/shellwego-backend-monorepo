# =============================================================================
# ShellWeGo - Control Plane Dockerfile
# =============================================================================
# Purpose-built image for the control-plane API server (Axum on port 8080).
# Usage:
#   docker build -f docker/control-plane.Dockerfile -t shellwego/control-plane:latest ..
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

# Dependency caching layer - copy workspace manifests
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./

# Stub all crate sources so dependency resolution works without real code
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

# Copy real source code
COPY crates/ crates/

# Rebuild with actual source
RUN touch crates/shellwego-control-plane/src/main.rs && \
    cargo build --release --bin shellwego-control-plane

# Strip the binary to reduce image size
RUN strip /build/target/release/shellwego-control-plane

# ---------------------------------------------------------------------------
# Stage 2: Runtime
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

LABEL maintainer="ShellWeGo Contributors"
LABEL description="ShellWeGo Control Plane - API server for the Sovereign Cloud Platform"
LABEL org.opencontainers.image.source="https://github.com/shellwego/shellwego"

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN groupadd --gid 1000 shellwego && \
    useradd --uid 1000 --gid shellwego --shell /bin/false shellwego

COPY --from=builder /build/target/release/shellwego-control-plane /usr/local/bin/shellwego-control-plane

RUN mkdir -p /var/lib/shellwego/builds/logs && \
    mkdir -p /var/lib/shellwego/data && \
    chown -R shellwego:shellwego /var/lib/shellwego

USER shellwego

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

ENV BIND_ADDR=0.0.0.0:8080 \
    LOG_LEVEL=info \
    DEFAULT_REGION=default \
    DATABASE_URL=sqlite:/var/lib/shellwego/control-plane.db \
    SHELLWEGO_DOMAIN=shellwego.local

ENTRYPOINT ["sh", "-c", "shellwego-control-plane"]
