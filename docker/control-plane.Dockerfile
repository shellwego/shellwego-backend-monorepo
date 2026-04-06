# =============================================================================
# ShellWeGo - Control Plane Dockerfile
# =============================================================================
# Purpose-built image for the control-plane API server (Axum on port 8080).
# Supports multi-arch builds (linux/amd64, linux/arm64).
# Usage:
#   docker buildx build -f docker/control-plane.Dockerfile --platform linux/amd64 -t shellwego/control-plane:latest .
# =============================================================================

# ---------------------------------------------------------------------------
# Stage 1: Builder
# ---------------------------------------------------------------------------
FROM --platform=$BUILDPLATFORM rust:1.94-slim AS builder

ARG TARGETPLATFORM
ARG BUILDPLATFORM

# Install cross-compilation dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    protobuf-compiler \
    clang \
    llvm \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install musl-tools for static linking
RUN apt-get update && apt-get install -y --no-install-recommends \
    musl-tools \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace manifests for dependency caching
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./

# Determine target triple based on platform
RUN set -eux; \
    if [ "$TARGETPLATFORM" = "linux/arm64" ]; then \
        echo "aarch64-unknown-linux-musl" > /build/target.txt; \
        rustup target add aarch64-unknown-linux-musl; \
    else \
        echo "x86_64-unknown-linux-musl" > /build/target.txt; \
        rustup target add x86_64-unknown-linux-musl; \
    fi

# Stub all crate sources for dependency caching
RUN mkdir -p crates/shellwego-control-plane/src && \
    echo "fn main() {}" > crates/shellwego-control-plane/src/main.rs && \
    mkdir -p crates/shellwego-schema/src && \
    echo "" > crates/shellwego-schema/src/lib.rs && \
    for crate in shellwego-storage shellwego-registry shellwego-network \
                  shellwego-billing shellwego-observability shellwego-edge \
                  shellwego-firecracker shellwego-agent shellwego-cli; do \
        mkdir -p crates/$crate/src && echo "" > crates/$crate/src/lib.rs 2>/dev/null || true; \
    done && \
    cargo build --release --target $(cat /build/target.txt) --bin shellwego-control-plane 2>/dev/null || true

# Copy real source code
COPY crates/ crates/
COPY migrations/ migrations/
COPY frontend/ frontend/

# Rebuild with actual source
RUN touch crates/shellwego-control-plane/src/main.rs && \
    cargo build --release --target $(cat /build/target.txt) --bin shellwego-control-plane

# Strip the binary to reduce image size
RUN strip /build/target/$(cat /build/target.txt)/release/shellwego-control-plane

# ---------------------------------------------------------------------------
# Stage 2: Runtime
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

LABEL maintainer="ShellWeGo Contributors"
LABEL description="ShellWeGo Control Plane - API server for the Sovereign Cloud Platform"
LABEL org.opencontainers.image.source="https://github.com/shellwego/shellwego-backend-monorepo"

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN groupadd --gid 1000 shellwego && \
    useradd --uid 1000 --gid shellwego --shell /bin/false shellwego

COPY --from=builder /build/target/*/release/shellwego-control-plane /usr/local/bin/shellwego-control-plane

# Copy static dashboard files
COPY --from=builder /build/frontend/ /var/lib/shellwego/static/

RUN mkdir -p /var/lib/shellwego/builds/logs \
             /var/lib/shellwego/data \
             /var/lib/shellwego/static && \
    chown -R shellwego:shellwego /var/lib/shellwego

USER shellwego

VOLUME ["/var/lib/shellwego"]

EXPOSE 8080 9090

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

ENV BIND_ADDR=0.0.0.0:8080 \
    LOG_LEVEL=info \
    DEFAULT_REGION=default \
    STATIC_DIR=/var/lib/shellwego/static \
    DATABASE_URL=sqlite:/var/lib/shellwego/control-plane.db \
    SHELLWEGO_DOMAIN=shellwego.local

ENTRYPOINT ["sh", "-c", "shellwego-control-plane"]
