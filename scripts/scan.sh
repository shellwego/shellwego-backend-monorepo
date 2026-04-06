#!/usr/bin/env bash
# Run Trivy filesystem scan on the ShellWeGo binary
set -euo pipefail
BINARY="${1:-target/release/shellwego-control-plane}"
trivy fs --config .trivy.yml "$BINARY"
