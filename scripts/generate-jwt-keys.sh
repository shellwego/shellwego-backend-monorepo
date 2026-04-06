#!/usr/bin/env bash
# Generate RSA-2048 PEM key pair for ShellWeGo development
set -euo pipefail
OUT_DIR="${1:-.}"
openssl genrsa -out "$OUT_DIR/jwt-private.pem" 2048
openssl rsa -in "$OUT_DIR/jwt-private.pem" -pubout -out "$OUT_DIR/jwt-public.pem"
echo "Generated: $OUT_DIR/jwt-private.pem, $OUT_DIR/jwt-public.pem"
