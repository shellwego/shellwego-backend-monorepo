#!/usr/bin/env bash
# Generate SBOM for all ShellWeGo binaries
set -euo pipefail
TARGET_DIR="${1:-target/release}"
OUT_DIR="${2:-sbom}"
mkdir -p "$OUT_DIR"
for bin in shellwego-control-plane shellwego-agent shellwego-cli; do
    if [ -f "$TARGET_DIR/$bin" ]; then
        syft "$TARGET_DIR/$bin" -o spdx-json="$OUT_DIR/$bin.spdx.json" -o cyclonedx-json="$OUT_DIR/$bin.cdx.json"
        echo "SBOM generated: $OUT_DIR/$bin.*"
    fi
done
