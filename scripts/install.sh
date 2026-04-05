#!/usr/bin/env bash
# shellcheck enable=all
#
# ============================================================================
#  ShellWeGo — Sovereign Cloud Platform Installer
#  Version: 1.0.0
#  License: AGPL-3.0 / Commercial
# ============================================================================
#
#  Usage:
#    sudo ./install.sh --domain=cloud.example.com --email=admin@example.com
#    sudo ./install.sh --domain=cloud.example.com --email=admin@example.com --mode=kvm
#    sudo ./install.sh --uninstall [--purge-zfs]
#
#  Modes:
#    kvm   — Full hardware virtualization (KVM / QEMU / libvirt / ZFS)
#    pvm   — Paravirtualized machines (QEMU system emulation)
#    wasm  — Lightweight WebAssembly sandboxes only
#    auto  — Auto-detect capabilities (default)
#
#  Options:
#    --domain=DOMAIN    Primary domain for the control plane  (required)
#    --email=EMAIL      Admin / ACME notification email       (required)
#    --mode=MODE        kvm | pvm | wasm | auto              (default: auto)
#    --license=TYPE     agpl | commercial                    (default: agpl)
#    --uninstall        Remove ShellWeGo services and binaries
#    --purge-zfs        Also destroy the shellwego ZFS pool  (only with --uninstall)
#    --help             Show this message and exit
#
# ============================================================================

set -euo pipefail

# ── Paths & constants ────────────────────────────────────────────────────────

readonly INSTALL_VERSION="1.0.0"
readonly INSTALL_DIR="/opt/shellwego"
readonly CONFIG_DIR="/etc/shellwego"
readonly DATA_DIR="/var/lib/shellwego"
readonly LOG_DIR="/var/log/shellwego"
readonly TLS_DIR="${CONFIG_DIR}/tls"
readonly BIN_DIR="/usr/local/bin"
readonly MONOREPO_URL="https://github.com/shellwego/shellwego-backend-monorepo.git"

# ── Colour helpers ───────────────────────────────────────────────────────────

if [[ -t 1 ]] && command -v tput &>/dev/null; then
  RED=$(tput setaf 1)
  GREEN=$(tput setaf 2)
  YELLOW=$(tput setaf 3)
  BLUE=$(tput setaf 4)
  MAGENTA=$(tput setaf 5)
  CYAN=$(tput setaf 6)
  BOLD=$(tput bold)
  DIM=$(tput dim)
  RESET=$(tput sgr0)
else
  RED=""
  GREEN=""
  YELLOW=""
  BLUE=""
  MAGENTA=""
  CYAN=""
  BOLD=""
  DIM=""
  RESET=""
fi

# ── Logging ──────────────────────────────────────────────────────────────────

_ts() { date "+%Y-%m-%d %H:%M:%S"; }

info()  { printf "${GREEN}[INFO]${RESET}  %s %s\n" "$(_ts)" "$*"; }
warn()  { printf "${YELLOW}[WARN]${RESET}  %s %s\n" "$(_ts)" "$*"; }
error() { printf "${RED}[ERROR]${RESET} %s %s\n" "$(_ts)" "$*" >&2; }
step()  { printf "\n${BOLD}${BLUE}==> %s${RESET}\n\n" "$*"; }

die() {
  error "$*"
  exit 1
}

# ── Banner ───────────────────────────────────────────────────────────────────

print_banner() {
  printf "${CYAN}${BOLD}"
cat <<'BANNER'

  ████████╗██████╗  ██████╗  ██████╗███████╗████████╗
  ╚══██╔══╝██╔══██╗██╔═══██╗██╔════╝██╔════╝╚══██╔══╝
     ██║   ██████╔╝██║   ██║██║     ███████╗   ██║
     ██║   ██╔══██╗██║   ██║██║     ╚════██║   ██║
     ██║   ██║  ██║╚██████╔╝╚██████╗███████║   ██║
     ╚═╝   ╚═╝  ╚═╝ ╚═════╝  ╚═════╝╚══════╝   ╚═╝

       Sovereign Cloud Platform  ·  Installer v${INSTALL_VERSION}
       https://shellwego.dev

BANNER
  printf "${RESET}"
}

# ── Argument parsing ─────────────────────────────────────────────────────────

DOMAIN=""
EMAIL=""
MODE="auto"
LICENSE="agpl"
UNINSTALL=false
PURGE_ZFS=false

usage() {
  sed -n '2,/^#  ===/p' "$0" | sed 's/^# \?//'
  exit 0
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --domain=*)  DOMAIN="${1#*=}" ;;
      --email=*)   EMAIL="${1#*=}" ;;
      --mode=*)    MODE="${1#*=}" ;;
      --license=*) LICENSE="${1#*=}" ;;
      --uninstall) UNINSTALL=true ;;
      --purge-zfs) PURGE_ZFS=true ;;
      --help|-h)   usage ;;
      *)
        die "Unknown option: $1. Run with --help for usage."
        ;;
    esac
    shift
  done
}

validate_args() {
  if [[ "$UNINSTALL" == true ]]; then
    return 0
  fi

  if [[ -z "$DOMAIN" ]]; then
    die "Missing required --domain=<FQDN>."
  fi
  if [[ -z "$EMAIL" ]]; then
    die "Missing required --email=<address>."
  fi
  if [[ "$EMAIL" != *@* ]]; then
    die "Invalid email format: $EMAIL"
  fi
  if [[ "$MODE" != "auto" && "$MODE" != "kvm" && "$MODE" != "pvm" && "$MODE" != "wasm" ]]; then
    die "Invalid mode: $MODE. Choose from: auto, kvm, pvm, wasm."
  fi
  if [[ "$LICENSE" != "agpl" && "$LICENSE" != "commercial" ]]; then
    die "Invalid license: $LICENSE. Choose from: agpl, commercial."
  fi
}

# ── Root check ───────────────────────────────────────────────────────────────

require_root() {
  if [[ "${EUID}" -ne 0 ]]; then
    die "This script must run as root (use sudo)."
  fi
}

# ── OS & package manager detection ───────────────────────────────────────────

OS_ID=""
OS_VERSION=""
PKG_MANAGER=""
PKG_UPDATE=""
PKG_INSTALL=""

detect_os() {
  step "Detecting operating system"

  if [[ -f /etc/os-release ]]; then
    # shellcheck disable=SC1091
    source /etc/os-release
    OS_ID="${ID}"
    OS_VERSION="${VERSION_ID:-unknown}"
  elif command -v sw_vers &>/dev/null; then
    OS_ID="macos"
    OS_VERSION="$(sw_vers -productVersion)"
  else
    die "Unable to detect operating system."
  fi

  info "Detected: ${OS_ID} ${OS_VERSION} ($(uname -m))"

  # Detect package manager
  if command -v apt-get &>/dev/null; then
    PKG_MANAGER="apt"
    PKG_UPDATE="apt-get update -qq"
    PKG_INSTALL="apt-get install -y -qq"
  elif command -v dnf &>/dev/null; then
    PKG_MANAGER="dnf"
    PKG_UPDATE="dnf check-update --quiet || true"
    PKG_INSTALL="dnf install -y -q"
  elif command -v yum &>/dev/null; then
    PKG_MANAGER="yum"
    PKG_UPDATE="yum check-update --quiet || true"
    PKG_INSTALL="yum install -y -q"
  elif command -v apk &>/dev/null; then
    PKG_MANAGER="apk"
    PKG_UPDATE="apk update --quiet"
    PKG_INSTALL="apk add --quiet"
  else
    die "No supported package manager found (apt / dnf / yum / apk)."
  fi

  info "Package manager: ${PKG_MANAGER}"
}

# ── Mode auto-detection ─────────────────────────────────────────────────────

resolve_mode() {
  if [[ "$MODE" != "auto" ]]; then
    info "Mode explicitly set to: ${MODE}"
    return 0
  fi

  step "Auto-detecting virtualization mode"

  # Check for KVM support
  if [[ -e /dev/kvm ]] && command -v qemu-kvm &>/dev/null; then
    MODE="kvm"
    info "KVM detected — using KVM mode"
    return 0
  fi

  # Check for KVM device (kernel support) even if qemu not yet installed
  if [[ -e /dev/kvm ]]; then
    MODE="kvm"
    info "/dev/kvm present — using KVM mode"
    return 0
  fi

  # Check CPU virtualization extensions
  if command -v grep &>/dev/null && grep -qE 'vmx|svm' /proc/cpuinfo 2>/dev/null; then
    if [[ "$(uname -m)" == "x86_64" ]]; then
      MODE="kvm"
      warn "CPU supports virtualization but /dev/kvm not found."
      warn "KVM modules may need to be loaded. Falling back to KVM mode."
      return 0
    fi
  fi

  # Check if wasmtime is present for WASM
  if command -v wasmtime &>/dev/null; then
    MODE="wasm"
    info "Wasmtime detected — using WASM mode"
    return 0
  fi

  # Default to WASM — safest lightweight option
  MODE="wasm"
  warn "Could not detect KVM support — defaulting to WASM mode."
  warn "Install qemu-kvm and enable KVM for full virtualization."
}

# ── Install system dependencies ──────────────────────────────────────────────

install_deps() {
  step "Installing system dependencies (mode: ${MODE})"

  # Refresh package index
  info "Updating package index..."
  eval "$PKG_UPDATE" || warn "Package index update returned non-zero (may be non-fatal)"

  # ── Common dependencies (all modes) ──
  local common_pkgs=()
  case "$PKG_MANAGER" in
    apt)
      common_pkgs=(curl git build-essential pkg-config libssl-dev protobuf-compiler)
      ;;
    dnf|yum)
      common_pkgs=(curl git gcc gcc-c++ make openssl-devel protobuf-compiler pkgconfig)
      ;;
    apk)
      common_pkgs=(curl git build-base openssl-dev protobuf-dev pkgconfig)
      ;;
  esac

  info "Installing common dependencies: ${common_pkgs[*]}"
  eval "$PKG_INSTALL ${common_pkgs[*]}" || die "Failed to install common dependencies."

  # ── Mode-specific dependencies ──
  local mode_pkgs=()
  case "$MODE" in
    kvm)
      case "$PKG_MANAGER" in
        apt)
          mode_pkgs=(qemu-kvm libvirt-daemon-system libvirt-clients zfsutils-linux libguestfs-tools)
          ;;
        dnf|yum)
          mode_pkgs=(qemu-kvm libvirt libvirt-client zfs nmap)
          ;;
        apk)
          mode_pkgs=(qemu-system-x86_64 libvirt zfs-utils)
          ;;
      esac
      ;;
    pvm)
      case "$PKG_MANAGER" in
        apt)
          mode_pkgs=(qemu-system-x86 libguestfs-tools)
          ;;
        dnf|yum)
          mode_pkgs=(qemu-system-x86)
          ;;
        apk)
          mode_pkgs=(qemu-system-x86_64)
          ;;
      esac
      ;;
    wasm)
      case "$PKG_MANAGER" in
        apt)
          mode_pkgs=(wasmtime)
          ;;
        dnf|yum)
          # wasmtime may not be in default repos — install via cargo later
          mode_pkgs=()
          ;;
        apk)
          mode_pkgs=(wasmtime)
          ;;
      esac
      ;;
  esac

  if [[ ${#mode_pkgs[@]} -gt 0 ]]; then
    info "Installing ${MODE}-mode dependencies: ${mode_pkgs[*]}"
    eval "$PKG_INSTALL ${mode_pkgs[*]}" || \
      warn "Some ${MODE}-mode packages failed to install. This may be non-fatal."
  fi

  info "System dependencies installed."
}

# ── Rust toolchain ───────────────────────────────────────────────────────────

install_rust() {
  step "Ensuring Rust toolchain is available"

  if command -v rustc &>/dev/null && command -v cargo &>/dev/null; then
    local rust_version
    rust_version="$(rustc --version)"
    info "Rust already installed: ${rust_version}"
    return 0
  fi

  info "Rust not found. Installing via rustup..."

  # Ensure curl is present (should be, from common deps)
  command -v curl &>/dev/null || die "curl is required to install Rust."

  # Check for existing rustup
  if [[ -x "${HOME}/.cargo/bin/rustup" ]] || [[ -x "/usr/local/cargo/bin/rustup" ]]; then
    warn "rustup found but Rust not in PATH. Attempting to source..."
    if [[ -f "${HOME}/.cargo/env" ]]; then
      # shellcheck disable=SC1091
      source "${HOME}/.cargo/env"
    fi
    if command -v rustc &>/dev/null; then
      info "Rust activated after sourcing cargo env."
      return 0
    fi
  fi

  # Install rustup non-interactively
  local rustup_sh="/tmp/rustup-init.sh"
  info "Downloading rustup installer..."
  curl --proto '=https' --tlsv1.2 -sSf "https://sh.rustup.rs" -o "$rustup_sh" || \
    die "Failed to download rustup installer."

  chmod +x "$rustup_sh"
  RUSTUP_INIT_SKIP_PATH_CHECK=yes "$rustup_sh" -y --default-toolchain stable \
    --profile minimal 2>&1 || die "rustup installation failed."

  # Source cargo env for this shell session
  if [[ -f "${HOME}/.cargo/env" ]]; then
    # shellcheck disable=SC1091
    source "${HOME}/.cargo/env"
  elif [[ -f "/root/.cargo/env" ]]; then
    # shellcheck disable=SC1091
    source "/root/.cargo/env"
  fi

  if ! command -v rustc &>/dev/null; then
    die "Rust installation completed but rustc not found in PATH."
  fi

  info "Rust installed: $(rustc --version)"
  info "Cargo installed: $(cargo --version)"
}

# ── Install wasmtime for WASM mode (fallback for distros without package) ────

install_wasmtime_fallback() {
  if [[ "$MODE" != "wasm" ]]; then
    return 0
  fi

  if command -v wasmtime &>/dev/null; then
    info "wasmtime already available."
    return 0
  fi

  info "Installing wasmtime via official installer..."
  curl https://wasmtime.dev/install.sh -sSf | bash 2>&1 || \
    warn "wasmtime installation via installer failed."

  # Try to source installed wasmtime
  if [[ -f "${HOME}/.wasmtime/bin/wasmtime" ]]; then
    export PATH="${HOME}/.wasmtime/bin:${PATH}"
    info "wasmtime installed to ~/.wasmtime/bin/"
  elif ! command -v wasmtime &>/dev/null; then
    warn "wasmtime could not be installed. WASM mode may not work correctly."
    warn "Install wasmtime manually: https://wasmtime.dev"
  fi
}

# ── Build from source ────────────────────────────────────────────────────────

build_from_source() {
  step "Building ShellWeGo from source"

  local src_dir="${INSTALL_DIR}/src"

  # Detect if we're running from inside the monorepo
  local script_dir
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  local possible_repo_root
  possible_repo_root="$(cd "${script_dir}/.." && pwd)"

  if [[ -f "${possible_repo_root}/Cargo.toml" ]]; then
    info "Detected local monorepo at ${possible_repo_root}"
    src_dir="${possible_repo_root}"
  fi

  # If not a local build, clone from remote
  if [[ ! -f "${src_dir}/Cargo.toml" ]]; then
    if [[ -d "${INSTALL_DIR}/src" ]]; then
      info "Source directory exists at ${INSTALL_DIR}/src — updating..."
      git -C "${INSTALL_DIR}/src" pull --ff-only 2>&1 || \
        warn "Git pull failed. Using existing source tree."
    else
      info "Cloning ShellWeGo monorepo..."
      mkdir -p "${INSTALL_DIR}"
      git clone --depth 1 "$MONOREPO_URL" "${INSTALL_DIR}/src" 2>&1 || \
        die "Failed to clone ${MONOREPO_URL}."
    fi
    src_dir="${INSTALL_DIR}/src"
  fi

  # Build
  info "Building ShellWeGo (release mode)..."
  info "Source: ${src_dir}"
  info "This may take several minutes on first build..."

  (
    cd "$src_dir"

    # Respect rust-toolchain.toml if present
    if [[ -f rust-toolchain.toml ]]; then
      info "Using rust-toolchain.toml for version pinning."
    fi

    cargo build --release \
      --bin shellwego-control-plane \
      --bin shellwego-agent \
      --bin shellwego-cli \
      2>&1 || die "Build failed. Check the output above for errors."
  )

  info "Build completed successfully."

  # Install binaries
  step "Installing binaries to ${BIN_DIR}"

  local -a bins=(shellwego-control-plane shellwego-agent shellwego-cli)
  for bin in "${bins[@]}"; do
    local src_bin="${src_dir}/target/release/${bin}"
    if [[ -f "$src_bin" ]]; then
      cp -f "$src_bin" "${BIN_DIR}/${bin}" || die "Failed to install ${bin}"
      chmod 755 "${BIN_DIR}/${bin}"
      info "Installed ${BIN_DIR}/${bin}"
    else
      warn "Binary not found: ${src_bin}"
    fi
  done

  # Create convenience symlink
  ln -sf "${BIN_DIR}/shellwego-cli" "${BIN_DIR}/shellwego" 2>/dev/null || true
  info "Created symlink: ${BIN_DIR}/shellwego -> shellwego-cli"
}

# ── ZFS pool initialization ──────────────────────────────────────────────────

init_zfs_pool() {
  if [[ "$MODE" == "wasm" ]]; then
    info "WASM mode — skipping ZFS pool initialization."
    return 0
  fi

  step "Initializing ZFS pool"

  if ! command -v zpool &>/dev/null; then
    warn "zpool command not found. ZFS may not be installed."
    warn "Skipping ZFS initialization. Storage features may be limited."
    return 0
  fi

  # Check if pool already exists
  if zpool list shellwego &>/dev/null 2>&1; then
    warn "ZFS pool 'shellwego' already exists. Skipping creation."
    return 0
  fi

  # Find a suitable disk for ZFS
  local zfs_disk=""
  local zfs_vdev=""

  # Look for unused block devices (simplified heuristic)
  for dev in /dev/sdb /dev/sdc /dev/vdb /dev/vdc /dev/nvme1n1; do
    if [[ -b "$dev" ]]; then
      # Check if device is mounted or in use
      if ! lsblk -no MOUNTPOINT "$dev" 2>/dev/null | grep -q .; then
        zfs_disk="$dev"
        break
      fi
    fi
  done

  if [[ -n "$zfs_disk" ]]; then
    zfs_vdev="$zfs_disk"
    warn "Found unused disk: ${zfs_disk}"
    warn "ALL DATA on ${zfs_disk} WILL BE DESTROYED."
  else
    info "No unused block device found. Creating a sparse file-backed ZFS pool for testing."
    info "For production, provide a dedicated disk."

    local zfs_file="${DATA_DIR}/shellwego-zfs.img"
    mkdir -p "$DATA_DIR"
    info "Creating 10 GB sparse ZFS backing file at ${zfs_file}..."
    truncate -s 10G "$zfs_file"
    zfs_vdev="$zfs_file"
  fi

  info "Creating ZFS pool 'shellwego' on ${zfs_vdev}..."
  zpool create -f \
    -O compression=lz4 \
    -O atime=off \
    -O xattr=sa \
    -O mountpoint="${DATA_DIR}/zfs" \
    shellwego "$zfs_vdev" 2>&1 || \
    die "Failed to create ZFS pool. Check that ${zfs_vdev} is available."

  info "ZFS pool 'shellwego' created successfully."

  # Create dataset for app storage
  zfs create shellwego/apps 2>/dev/null || true
  zfs create shellwego/volumes 2>/dev/null || true
  zfs create shellwego/images 2>/dev/null || true

  info "ZFS datasets created: apps, volumes, images"
}

# ── TLS certificate generation ───────────────────────────────────────────────

generate_tls_cert() {
  step "Provisioning TLS certificates"

  mkdir -p "$TLS_DIR"

  # Check for existing certificates
  if [[ -f "${TLS_DIR}/cert.pem" && -f "${TLS_DIR}/key.pem" ]]; then
    warn "TLS certificates already exist at ${TLS_DIR}/"
    warn "Skipping generation. Remove them to regenerate."
    return 0
  fi

  # Try certbot / ACME first (requires port 80 access and real domain)
  if command -v certbot &>/dev/null && [[ "$DOMAIN" != "localhost" && "$DOMAIN" != *"local"* ]]; then
    info "certbot found. Attempting Let's Encrypt certificate..."
    if certbot certonly \
        --standalone \
        --non-interactive \
        --agree-tos \
        --email "$EMAIL" \
        -d "$DOMAIN" 2>&1; then
      # Copy LE certs to our directory
      cp -f "/etc/letsencrypt/live/${DOMAIN}/fullchain.pem" "${TLS_DIR}/cert.pem"
      cp -f "/etc/letsencrypt/live/${DOMAIN}/privkey.pem" "${TLS_DIR}/key.pem"
      chmod 640 "${TLS_DIR}/key.pem"
      info "Let's Encrypt certificate obtained successfully."
      return 0
    fi
    warn "Let's Encrypt failed (port 80 may be blocked or domain not resolving)."
    warn "Falling back to self-signed certificate."
  fi

  # Generate self-signed certificate
  info "Generating self-signed TLS certificate for ${DOMAIN}..."
  openssl req -x509 -newkey rsa:4096 \
    -keyout "${TLS_DIR}/key.pem" \
    -out "${TLS_DIR}/cert.pem" \
    -days 365 \
    -nodes \
    -subj "/CN=${DOMAIN}/O=ShellWeGo/C=US" \
    -addext "subjectAltName=DNS:${DOMAIN},DNS:*.${DOMAIN},IP:127.0.0.1" \
    2>&1 || die "Failed to generate TLS certificate."

  chmod 644 "${TLS_DIR}/cert.pem"
  chmod 640 "${TLS_DIR}/key.pem"

  info "Self-signed TLS certificate generated:"
  info "  Certificate: ${TLS_DIR}/cert.pem"
  info "  Private Key: ${TLS_DIR}/key.pem"
  warn "Self-signed certificates will cause browser warnings."
  warn "For production, configure Let's Encrypt or provide your own certificates."
}

# ── Configuration ────────────────────────────────────────────────────────────

write_config() {
  step "Writing configuration"

  mkdir -p "$CONFIG_DIR" "$DATA_DIR" "$LOG_DIR"

  cat > "${CONFIG_DIR}/config.toml" <<EOF
# ShellWeGo Control Plane Configuration
# Generated by install.sh on $(date -u +"%Y-%m-%dT%H:%M:%SZ")

[general]
domain = "${DOMAIN}"
email  = "${EMAIL}"
mode   = "${MODE}"
license = "${LICENSE}"
data_dir = "${DATA_DIR}"
log_dir  = "${LOG_DIR}"

[tls]
cert_path = "${TLS_DIR}/cert.pem"
key_path  = "${TLS_DIR}/key.pem"

[api]
bind_addr = "0.0.0.0:8443"

[grpc]
bind_addr = "0.0.0.0:9090"

EOF

  # ZFS section for non-WASM modes
  if [[ "$MODE" != "wasm" ]]; then
    cat >> "${CONFIG_DIR}/config.toml" <<EOF

[storage]
backend = "zfs"
pool = "shellwego"
EOF
  else
    cat >> "${CONFIG_DIR}/config.toml" <<EOF

[storage]
backend = "filesystem"
base_path = "${DATA_DIR}/storage"
EOF
  fi

  info "Configuration written to ${CONFIG_DIR}/config.toml"
}

# ── Systemd service files ───────────────────────────────────────────────────

create_systemd_services() {
  step "Creating systemd service files"

  # ── shellwego-control-plane.service ──
  cat > /etc/systemd/system/shellwego-control-plane.service <<'EOF'
[Unit]
Description=ShellWeGo Control Plane
Documentation=https://docs.shellwego.dev
After=network-online.target
Wants=network-online.target
ConditionPathExists=/usr/local/bin/shellwego-control-plane

[Service]
Type=simple
ExecStart=/usr/local/bin/shellwego-control-plane --config /etc/shellwego/config.toml
Restart=on-failure
RestartSec=5
StartLimitBurst=5
StartLimitIntervalSec=60
LimitNOFILE=65535
LimitNPROC=4096
Environment=RUST_LOG=info
Environment=RUST_BACKTRACE=1
WorkingDirectory=/var/lib/shellwego
StateDirectory=shellwego
LogsDirectory=shellwego
ConfigurationDirectory=shellwego

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/shellwego /var/log/shellwego
PrivateTmp=true
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictRealtime=true

[Install]
WantedBy=multi-user.target
EOF

  # ── shellwego-agent.service ──
  cat > /etc/systemd/system/shellwego-agent.service <<'EOF'
[Unit]
Description=ShellWeGo Agent
Documentation=https://docs.shellwego.dev
After=network-online.target shellwego-control-plane.service
Wants=network-online.target
ConditionPathExists=/usr/local/bin/shellwego-agent

[Service]
Type=simple
ExecStart=/usr/local/bin/shellwego-agent --config /etc/shellwego/config.toml
Restart=on-failure
RestartSec=5
StartLimitBurst=5
StartLimitIntervalSec=60
LimitNOFILE=65535
LimitNPROC=4096
Environment=RUST_LOG=info
Environment=RUST_BACKTRACE=1
WorkingDirectory=/var/lib/shellwego
StateDirectory=shellwego
LogsDirectory=shellwego
ConfigurationDirectory=shellwego

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/shellwego /var/log/shellwego
PrivateTmp=true
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictRealtime=true

[Install]
WantedBy=multi-user.target
EOF

  # Reload systemd daemon
  systemctl daemon-reload 2>&1 || die "Failed to reload systemd daemon."

  info "Created systemd units:"
  info "  /etc/systemd/system/shellwego-control-plane.service"
  info "  /etc/systemd/system/shellwego-agent.service"
}

# ── Initialize the platform ──────────────────────────────────────────────────

init_platform() {
  step "Initializing ShellWeGo control plane"

  if command -v shellwego &>/dev/null; then
    info "Running: shellwego init --role=control-plane --domain=${DOMAIN} --email=${EMAIL}"
    shellwego init \
      --role=control-plane \
      --domain="$DOMAIN" \
      --email="$EMAIL" \
      2>&1 || warn "shellwego init returned non-zero. Platform may need manual setup."
  else
    warn "shellwego CLI not found in PATH. Skipping init."
    warn "Run manually after install: shellwego init --role=control-plane --domain=${DOMAIN} --email=${EMAIL}"
    return 0
  fi

  info "Platform initialization completed."
}

# ── Enable and start services ────────────────────────────────────────────────

enable_services() {
  step "Enabling and starting ShellWeGo services"

  info "Enabling shellwego-control-plane.service..."
  systemctl enable shellwego-control-plane.service 2>&1 || \
    warn "Failed to enable shellwego-control-plane.service"

  info "Enabling shellwego-agent.service..."
  systemctl enable shellwego-agent.service 2>&1 || \
    warn "Failed to enable shellwego-agent.service"

  info "Starting shellwego-control-plane.service..."
  systemctl start shellwego-control-plane.service 2>&1 || \
    warn "shellwego-control-plane.service failed to start. Check: journalctl -u shellwego-control-plane"

  # Brief pause to let the control plane come up
  sleep 2

  info "Starting shellwego-agent.service..."
  systemctl start shellwego-agent.service 2>&1 || \
    warn "shellwego-agent.service failed to start. Check: journalctl -u shellwego-agent"
}

# ── Print success summary ────────────────────────────────────────────────────

print_success() {
  local proto="https"
  if [[ -f "${TLS_DIR}/cert.pem" ]]; then
    if openssl x509 -in "${TLS_DIR}/cert.pem" -noout -issuer 2>/dev/null | grep -q "self"; then
      proto="https (self-signed)"
    fi
  fi

  local cp_status="unknown"
  if systemctl is-active --quiet shellwego-control-plane.service 2>/dev/null; then
    cp_status="${GREEN}running${RESET}"
  else
    cp_status="${RED}stopped${RESET}"
  fi

  local agent_status="unknown"
  if systemctl is-active --quiet shellwego-agent.service 2>/dev/null; then
    agent_status="${GREEN}running${RESET}"
  else
    agent_status="${RED}stopped${RESET}"
  fi

  printf "\n"
  printf "${GREEN}${BOLD}═══════════════════════════════════════════════════════════════${RESET}\n"
  printf "${GREEN}${BOLD}  ShellWeGo Sovereign Cloud — Installation Complete!${RESET}\n"
  printf "${GREEN}${BOLD}═══════════════════════════════════════════════════════════════${RESET}\n"
  printf "\n"
  printf "  ${BOLD}Dashboard:${RESET}    ${CYAN}${proto}://${DOMAIN}/dashboard${RESET}\n"
  printf "  ${BOLD}API Endpoint:${RESET} ${CYAN}${proto}://${DOMAIN}:8443${RESET}\n"
  printf "  ${BOLD}gRPC:${RESET}         ${CYAN}${DOMAIN}:9090${RESET}\n"
  printf "\n"
  printf "  ${BOLD}Mode:${RESET}         %s\n" "$MODE"
  printf "  ${BOLD}License:${RESET}      %s\n" "$LICENSE"
  printf "  ${BOLD}Config:${RESET}       /etc/shellwego/config.toml\n"
  printf "  ${BOLD}Data:${RESET}         %s\n" "$DATA_DIR"
  printf "  ${BOLD}Logs:${RESET}         %s\n" "$LOG_DIR\n"
  printf "\n"
  printf "  ${BOLD}Services:${RESET}\n"
  printf "    control-plane: %b\n" "$cp_status"
  printf "    agent:         %b\n" "$agent_status"
  printf "\n"
  printf "  ${BOLD}Useful Commands:${RESET}\n"
  printf "    shellwego status          View cluster status\n"
  printf "    shellwego node list       List connected nodes\n"
  printf "    shellwego app deploy .    Deploy an application\n"
  printf "    journalctl -u shellwego-control-plane -f   Follow control plane logs\n"
  printf "    journalctl -u shellwego-agent -f           Follow agent logs\n"
  printf "\n"
  printf "  ${BOLD}Uninstall:${RESET}     sudo $0 --uninstall [--purge-zfs]\n"
  printf "\n"
  printf "${GREEN}${BOLD}═══════════════════════════════════════════════════════════════${RESET}\n"
  printf "\n"
}

# ── Uninstall ────────────────────────────────────────────────────────────────

do_uninstall() {
  step "Uninstalling ShellWeGo"

  # Stop services
  info "Stopping ShellWeGo services..."
  systemctl stop shellwego-agent.service 2>/dev/null || true
  systemctl stop shellwego-control-plane.service 2>/dev/null || true

  info "Disabling ShellWeGo services..."
  systemctl disable shellwego-agent.service 2>/dev/null || true
  systemctl disable shellwego-control-plane.service 2>/dev/null || true

  # Remove systemd unit files
  info "Removing systemd unit files..."
  rm -f /etc/systemd/system/shellwego-control-plane.service
  rm -f /etc/systemd/system/shellwego-agent.service
  systemctl daemon-reload 2>/dev/null || true

  # Remove binaries
  info "Removing ShellWeGo binaries..."
  rm -f "${BIN_DIR}/shellwego-control-plane"
  rm -f "${BIN_DIR}/shellwego-agent"
  rm -f "${BIN_DIR}/shellwego-cli"
  rm -f "${BIN_DIR}/shellwego"

  # Optionally purge ZFS pool
  if [[ "$PURGE_ZFS" == true ]]; then
    if command -v zpool &>/dev/null && zpool list shellwego &>/dev/null 2>&1; then
      warn "DESTROYING ZFS pool 'shellwego' — ALL DATA WILL BE LOST!"
      zpool destroy -f shellwego 2>&1 || warn "Failed to destroy ZFS pool."
      info "ZFS pool 'shellwego' destroyed."
    else
      info "ZFS pool 'shellwego' does not exist. Nothing to purge."
    fi
  else
    info "ZFS pool 'shellwego' preserved. Use --purge-zfs to destroy it."
  fi

  # Remove config, data, and logs
  info "Removing configuration, data, and logs..."
  rm -rf "${CONFIG_DIR}" 2>/dev/null || true
  rm -rf "${DATA_DIR}"    2>/dev/null || true
  rm -rf "${LOG_DIR}"     2>/dev/null || true

  printf "\n"
  printf "${YELLOW}${BOLD}ShellWeGo has been removed.${RESET}\n"
  printf "\n"
  if [[ "$PURGE_ZFS" != true ]]; then
    printf "  ${BOLD}Note:${RESET} The ZFS pool 'shellwego' and source tree at ${INSTALL_DIR}/src were preserved.\n"
    printf "  Run with ${BOLD}--purge-zfs${RESET} to destroy the pool, or manually remove ${INSTALL_DIR}.\n"
  else
    printf "  All ShellWeGo data has been purged.\n"
  fi
  printf "\n"
}

# ── Firewall hints ───────────────────────────────────────────────────────────

check_firewall() {
  step "Checking firewall configuration"

  local ports=(8443 9090)
  local tool=""

  if command -v ufw &>/dev/null && ufw status 2>/dev/null | grep -q "active"; then
    tool="ufw"
    for port in "${ports[@]}"; do
      if ! ufw status | grep -q "${port}"; then
        warn "Port ${port} is not open in ufw."
        warn "  Run: ufw allow ${port}/tcp"
      fi
    done
  elif command -v firewall-cmd &>/dev/null && firewall-cmd --state 2>/dev/null | grep -q "running"; then
    tool="firewalld"
    for port in "${ports[@]}"; do
      if ! firewall-cmd --list-ports 2>/dev/null | grep -q "${port}/tcp"; then
        warn "Port ${port} is not open in firewalld."
        warn "  Run: firewall-cmd --permanent --add-port=${port}/tcp && firewall-cmd --reload"
      fi
    done
  elif command -v iptables &>/dev/null; then
    warn "iptables detected. Ensure ports 8443 and 9090 are accessible."
  fi

  if [[ -n "$tool" ]]; then
    info "Firewall (${tool}) is active — verify required ports are open."
  fi
}

# ── Main ─────────────────────────────────────────────────────────────────────

main() {
  parse_args "$@"
  validate_args
  require_root
  print_banner

  if [[ "$UNINSTALL" == true ]]; then
    do_uninstall
    exit 0
  fi

  printf "${DIM}Domain:   %s\nEmail:    %s\nMode:     %s\nLicense:  %s${RESET}\n\n" \
    "$DOMAIN" "$EMAIL" "$MODE" "$LICENSE"

  detect_os
  resolve_mode
  install_deps
  install_rust
  install_wasmtime_fallback
  build_from_source
  init_zfs_pool
  generate_tls_cert
  write_config
  create_systemd_services
  init_platform
  enable_services
  check_firewall
  print_success
}

main "$@"
