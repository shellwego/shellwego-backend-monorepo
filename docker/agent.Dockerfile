# =============================================================================
# ShellWeGo - Agent Dockerfile
# =============================================================================
# Purpose-built image for the shellwego-agent (worker node daemon).
# Requires KVM/QEMU for VM management and Firecracker support.
# Usage:
#   docker build -f docker/agent.Dockerfile -t shellwego/agent:latest ..
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

# Dependency caching layer
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./

# Stub crate sources for dependency caching
RUN mkdir -p crates/shellwego-agent/src && \
    echo "fn main() {}" > crates/shellwego-agent/src/main.rs && \
    echo "" > crates/shellwego-agent/src/lib.rs && \
    mkdir -p crates/shellwego-schema/src && \
    echo "" > crates/shellwego-schema/src/lib.rs && \
    for crate in shellwego-storage shellwego-registry shellwego-network \
                  shellwego-billing shellwego-observability shellwego-edge \
                  shellwego-firecracker shellwego-control-plane shellwego-cli; do \
        mkdir -p crates/$crate/src && echo "" > crates/$crate/src/lib.rs 2>/dev/null || true; \
    done && \
    cargo build --release --bin shellwego-agent 2>/dev/null || true

# Copy real source code
COPY crates/ crates/

# Rebuild with actual source
RUN touch crates/shellwego-agent/src/main.rs && \
    cargo build --release --bin shellwego-agent

# Strip binary
RUN strip /build/target/release/shellwego-agent

# ---------------------------------------------------------------------------
# Stage 2: Runtime
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

LABEL maintainer="ShellWeGo Contributors"
LABEL description="ShellWeGo Agent - Worker node daemon for VM and workload management"
LABEL org.opencontainers.image.source="https://github.com/shellwego/shellwego"

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    qemu-kvm \
    libvirt-daemon-system \
    libvirt-clients \
    virtinst \
    cpu-checker \
    zfsutils-linux \
    bridge-utils \
    iproute2 \
    iptables \
    ethtool \
    jq \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user (will need privileged access at runtime for KVM)
RUN groupadd --gid 1000 shellwego && \
    useradd --uid 1000 --gid shellwego --create-home --shell /bin/bash shellwego && \
    usermod -aG kvm shellwego && \
    usermod -aG libvirt shellwego

COPY --from=builder /build/target/release/shellwego-agent /usr/local/bin/shellwego-agent

RUN mkdir -p /var/lib/shellwego/vms \
             /var/lib/shellwego/snapshots \
             /var/lib/shellwego/images \
             /var/lib/shellwego/wasm \
             /var/lib/shellwego/builds/logs \
             /var/log/shellwego && \
    chown -R shellwego:shellwego /var/lib/shellwego /var/log/shellwego

USER shellwego

EXPOSE 9090 50051

ENV LOG_LEVEL=info \
    SHELLWEGO_AGENT_REGION=default \
    VMM_DRIVER=kvm \
    SHELLWEGO_DATA_DIR=/var/lib/shellwego

ENTRYPOINT ["sh", "-c", "shellwego-agent"]
