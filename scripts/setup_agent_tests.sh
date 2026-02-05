#!/bin/bash
set -e

# Setup script for ShellWeGo Agent integration tests
# Needs root for installation and ZFS/Network configuration

if [ "$EUID" -ne 0 ]; then
  echo "Please run as root"
  exit 1
fi

echo ">>> Installing dependencies..."
apt-get update && apt-get install -y \
    zfsutils-linux \
    bridge-utils \
    curl \
    iproute2

echo ">>> Setting up Firecracker..."
if [ ! -f /usr/local/bin/firecracker ]; then
    ARCH="$(uname -m)"
    release_url="https://github.com/firecracker-microvm/firecracker/releases/download/v1.7.0/firecracker-v1.7.0-${ARCH}.tgz"
    curl -L "$release_url" | tar -xz
    mv "release-v1.7.0-${ARCH}/firecracker-v1.7.0-${ARCH}" /usr/local/bin/firecracker
    chmod +x /usr/local/bin/firecracker
    rm -rf "release-v1.7.0-${ARCH}"
    echo "Firecracker installed to /usr/local/bin/firecracker"
else
    echo "Firecracker already installed."
fi

echo ">>> Setting up ZFS test pool 'shellwego'..."
if ! zpool list shellwego > /dev/null 2>&1; then
    # Create a loopback file for the pool
    truncate -s 1G /tmp/shellwego-test.img
    zpool create shellwego /tmp/shellwego-test.img
    echo "ZFS pool 'shellwego' created backed by /tmp/shellwego-test.img"
else
    echo "ZFS pool 'shellwego' already exists."
fi

echo ">>> Checking KVM..."
if [ -e /dev/kvm ]; then
    chmod 666 /dev/kvm
    echo "KVM is available."
else
    echo "WARNING: /dev/kvm not found. Firecracker tests will be skipped."
fi

echo ">>> Done! You can now run: cargo test -p shellwego-agent --test integration_tests"