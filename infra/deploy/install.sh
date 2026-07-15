#!/usr/bin/env bash
# ================================================================
# AETHER Terminal — Production Installation Script
# EP-407: Deploy systemd units, environment config, nginx proxy,
# and production data stack (Docker Compose).
#
# Usage:
#   sudo ./install.sh                 # default install
#   sudo ./install.sh --dry-run       # preview without changes
#   sudo ./install.sh --skip-compose  # skip docker compose pull/up
#   sudo ./install.sh --help          # show this message
#
# This script must be run as root on the target brain host.
# Tested on Ubuntu 24.04 LTS / Debian 12 Bookworm.
# ================================================================

set -euo pipefail

# ── Constants ─────────────────────────────────────────────────
AETHER_USER="aether"
AETHER_GROUP="aether"
AETHER_HOME="/opt/aether"
AETHER_SYSTEMD_DIR="/etc/systemd/system"
AETHER_ENV_FILE="/etc/aether/environment"
AETHER_NGINX_SITES="/etc/nginx/sites-available"
AETHER_LOG_DIR="/var/log/aether"
AETHER_DATA_DIR="${AETHER_HOME}/data"
AETHER_KEYSTORE_DIR="/etc/aether/guardian-keystore"
AETHER_SCRIPTS_DIR="${AETHER_HOME}/scripts"

# Self path
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Color helpers
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Counters
OK=0
WARN=0
SKIP=0
FAIL=0

# ── CLI flags ────────────────────────────────────────────────
DRY_RUN=false
SKIP_COMPOSE=false

usage() {
    cat <<'USAGE'
Usage: sudo ./install.sh [OPTIONS]

Options:
  --dry-run       Print actions without executing them
  --skip-compose  Skip Docker Compose pull and stack start
  --help          Show this message and exit
USAGE
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)      DRY_RUN=true; shift ;;
        --skip-compose) SKIP_COMPOSE=true; shift ;;
        --help)         usage ;;
        *)              echo "Unknown option: $1"; usage ;;
    esac
done

# ── Utilities ─────────────────────────────────────────────────
info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; ((OK++)) || true; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; ((WARN++)) || true; }
skip()  { echo -e "${YELLOW}[SKIP]${NC}  $*"; ((SKIP++)) || true; }
fail()  { echo -e "${RED}[FAIL]${NC}  $*"; ((FAIL++)) || true; }
run() {
    if [[ "$DRY_RUN" == true ]]; then
        echo -e "${CYAN}[DRY-RUN]${NC} $*"
    else
        "$@"
    fi
}

# ── Preflight checks ─────────────────────────────────────────
preflight() {
    info "Running preflight checks..."

    if [[ $EUID -ne 0 ]]; then
        fail "This script must be run as root (sudo)."
        exit 1
    fi

    if [[ ! -d "$REPO_ROOT" ]]; then
        fail "Repository root not found at $REPO_ROOT"
        exit 1
    fi

    if [[ ! -d "${SCRIPT_DIR}" ]]; then
        fail "Deploy directory not found at ${SCRIPT_DIR}"
        exit 1
    fi

    # Check required tools
    local missing=()
    for cmd in systemctl nginx docker; do
        if ! command -v "$cmd" &>/dev/null; then
            missing+=("$cmd")
        fi
    done
    if ! command -v docker &>/dev/null || ! docker compose version &>/dev/null; then
        missing+=("docker compose (plugin)")
    fi
    if ! command -v openssl &>/dev/null; then
        missing+=("openssl")
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        fail "Missing required tools: ${missing[*]}"
        fail "Install them first: apt install nginx docker.io docker-compose-v2 openssl"
        exit 1
    fi

    if [[ "$DRY_RUN" == true ]]; then
        warn "DRY-RUN mode: no files will be modified."
    fi

    ok "Preflight checks passed"
}

# ── Create system user and directories ───────────────────────
setup_user_and_dirs() {
    info "Setting up aether user and directories..."

    # Create aether group and user if they don't exist
    if ! getent group "$AETHER_GROUP" &>/dev/null; then
        run groupadd --system "$AETHER_GROUP"
        ok "Created group: $AETHER_GROUP"
    else
        skip "Group $AETHER_GROUP already exists"
    fi

    if ! id "$AETHER_USER" &>/dev/null; then
        run useradd --system --gid "$AETHER_GROUP" --home-dir "$AETHER_HOME" \
            --no-create-home --shell /usr/sbin/nologin "$AETHER_USER"
        ok "Created user: $AETHER_USER"
    else
        skip "User $AETHER_USER already exists"
    fi

    # Create required directories
    local dirs=(
        "$AETHER_HOME"
        "$AETHER_DATA_DIR"
        "$AETHER_LOG_DIR"
        "$AETHER_KEYSTORE_DIR"
        "$AETHER_SCRIPTS_DIR"
        "/etc/aether"
        "/var/www/acme"
    )

    for d in "${dirs[@]}"; do
        if [[ ! -d "$d" ]]; then
            run mkdir -p "$d"
            ok "Created directory: $d"
        else
            skip "Directory exists: $d"
        fi
    done

    # Set ownership
    run chown -R "$AETHER_USER":"$AETHER_GROUP" "$AETHER_HOME"
    run chown -R "$AETHER_USER":"$AETHER_GROUP" "$AETHER_LOG_DIR"
    run chown -R "$AETHER_USER":"$AETHER_GROUP" "$AETHER_KEYSTORE_DIR"
    run chmod 750 "$AETHER_KEYSTORE_DIR"

    # Log directory must be writable by the service user
    run chmod 755 "$AETHER_LOG_DIR"

    ok "User and directories ready"
}

# ── Install environment file ─────────────────────────────────
install_environment() {
    info "Installing environment configuration..."

    local src="${SCRIPT_DIR}/environment.example"
    if [[ ! -f "$src" ]]; then
        fail "Environment template not found: $src"
        return 1
    fi

    if [[ -f "$AETHER_ENV_FILE" ]]; then
        warn "Environment file already exists at $AETHER_ENV_FILE — NOT overwriting"
        warn "  Compare with ${src} and merge any new variables manually."
    else
        run cp "$src" "$AETHER_ENV_FILE"
        run chown root:"$AETHER_GROUP" "$AETHER_ENV_FILE"
        run chmod 640 "$AETHER_ENV_FILE"
        ok "Installed: $AETHER_ENV_FILE"
        warn "  EDIT THIS FILE with production values before starting services!"
        warn "  File: $AETHER_ENV_FILE"
    fi
}

# ── Install systemd units ────────────────────────────────────
install_systemd_units() {
    info "Installing systemd unit files..."

    local units=(
        aether.target
        aether-gateway.service
        aether-brain.service
        aether-llm-router.service
        aether-risk-engine.service
        aether-order-router.service
        aether-wallet-guardian.service
        aether-scanner.service
        aether-simulator.service
        aether-paper-ledger.service
        aether-alerts.service
        aether-inbox.service
        audit-verify.service
        audit-verify.timer
    )

    local count=0
    for unit in "${units[@]}"; do
        local src="${SCRIPT_DIR}/${unit}"
        local dst="${AETHER_SYSTEMD_DIR}/${unit}"

        if [[ ! -f "$src" ]]; then
            warn "Unit file not found: $src (SKIPPING)"
            continue
        fi

        run cp "$src" "$dst"
        run chown root:root "$dst"
        run chmod 644 "$dst"
        ((count++)) || true
    done

    run systemctl daemon-reload
    ok "Installed $count systemd unit files"
}

# ── Install nginx configuration ──────────────────────────────
install_nginx_config() {
    info "Installing nginx configuration..."

    local src="${SCRIPT_DIR}/nginx-aether.conf"
    local dst="${AETHER_NGINX_SITES}/aether.conf"

    if [[ ! -f "$src" ]]; then
        fail "Nginx config not found: $src"
        return 1
    fi

    run cp "$src" "$dst"
    run chown root:root "$dst"
    run chmod 644 "$dst"

    # Enable site if not already enabled
    local enabled_link="/etc/nginx/sites-enabled/aether.conf"
    if [[ ! -L "$enabled_link" ]]; then
        run ln -sf "$dst" "$enabled_link"
        ok "Enabled nginx site: aether"
    else
        skip "Nginx site already enabled: aether"
    fi

    # Validate nginx config
    if [[ "$DRY_RUN" == false ]]; then
        if nginx -t 2>&1; then
            ok "Nginx configuration valid"
        else
            fail "Nginx configuration invalid — check $dst"
        fi
    else
        skip "Nginx config validation skipped (dry-run)"
    fi
}

# ── Install utility scripts ─────────────────────────────────
install_scripts() {
    info "Installing utility scripts..."

    # Deploy management script
    local deploy_mgr="${AETHER_SCRIPTS_DIR}/aetherctl"
    if [[ "$DRY_RUN" == false ]]; then
        cat > "$deploy_mgr" <<'CTL'
#!/usr/bin/env bash
# aetherctl — Manage AETHER service stack
set -euo pipefail

AETHER_ENV_FILE="/etc/aether/environment"
COMPOSE_FILE="/opt/aether/infra/deploy/docker-compose.prod.yml"

usage() {
    cat <<'USAGE'
Usage: aetherctl <command>

Commands:
  start         Start all AETHER services (systemd)
  stop          Stop all AETHER services
  status        Show status of all AETHER services
  restart       Restart all AETHER services
  logs [svc]    Tail logs for a service (default: all)
  env           Print environment summary (secrets redacted)
  compose-up    Start production data stack (docker compose)
  compose-down  Stop production data stack
  compose-logs  Tail compose container logs
  verify        Check all services health
  migrate       Run pending database migrations
  version       Print installed version
USAGE
}

case "${1:-help}" in
    start)
        systemctl start aether.target
        ;;
    stop)
        systemctl stop aether.target
        ;;
    status)
        systemctl list-units --type=service 'aether-*'
        ;;
    restart)
        systemctl restart aether.target
        ;;
    logs)
        if [[ -n "${2:-}" ]]; then
            journalctl -fu "$2" -n 50
        else
            journalctl -fu 'aether-*' -n 100
        fi
        ;;
    env)
        if [[ -f "$AETHER_ENV_FILE" ]]; then
            echo "=== AETHER Environment (redacted) ==="
            grep -v '^\s*#' "$AETHER_ENV_FILE" | grep -v '^\s*$' | \
                while IFS='=' read -r key value; do
                    if [[ "$key" =~ (PASSWORD|SECRET|TOKEN|KEY|PRIVATE) ]]; then
                        echo "${key}=***REDACTED***"
                    else
                        echo "${key}=${value}"
                    fi
                done
        else
            echo "Environment file not found: $AETHER_ENV_FILE"
        fi
        ;;
    compose-up)
        docker compose -f "$COMPOSE_FILE" up -d --wait
        ;;
    compose-down)
        docker compose -f "$COMPOSE_FILE" down
        ;;
    compose-logs)
        docker compose -f "$COMPOSE_FILE" logs -f
        ;;
    verify)
        echo "=== AETHER Service Health ==="
        for svc in $(systemctl list-units --type=service 'aether-*' --no-legend 2>/dev/null | awk '{print $1}'); do
            status=$(systemctl is-active "$svc" 2>/dev/null || echo "not-found")
            echo "  $svc  [$status]"
        done
        ;;
    migrate)
        echo "Running database migrations..."
        cd /opt/aether
        # Assumes cargo sqlx is available on the build host
        cargo sqlx migrate run --source infra/migrations
        ;;
    version)
        cat /opt/aether/VERSION 2>/dev/null || echo "unknown"
        ;;
    *)
        usage
        ;;
esac
CTL
        chmod +x "$deploy_mgr"
        chown root:root "$deploy_mgr"
        ok "Installed: $deploy_mgr"
    else
        skip "Would install: aetherctl script"
    fi

    # Symlink into PATH
    if [[ ! -L "/usr/local/bin/aetherctl" ]]; then
        run ln -sf "$deploy_mgr" "/usr/local/bin/aetherctl"
        ok "Symlinked: /usr/local/bin/aetherctl"
    else
        skip "aetherctl already in PATH"
    fi
}

# ── Install binary artifacts ─────────────────────────────────
install_binaries() {
    info "Installing AETHER binaries..."

    # Check for pre-built binaries in the deploy directory
    local binary_dir="${SCRIPT_DIR}/bin"
    if [[ ! -d "$binary_dir" ]]; then
        warn "No pre-built binaries found at ${binary_dir}"
        warn "  Build them first with: cargo build --workspace --release --target-dir ${binary_dir}"
        warn "  Or copy release binaries to: ${binary_dir}/"
        return
    fi

    local binaries=(
        aether-gateway
        aether-risk-engine
        aether-order-router
        aether-wallet-guardian
        aether-scanner
        aether-simulator
        aether-paper-ledger
    )

    local count=0
    for bin in "${binaries[@]}"; do
        local src="${binary_dir}/${bin}"
        if [[ ! -f "$src" ]]; then
            warn "Binary not found: $src (SKIPPING)"
            continue
        fi
        run cp "$src" "/usr/local/bin/${bin}"
        run chown root:"$AETHER_GROUP" "/usr/local/bin/${bin}"
        run chmod 750 "/usr/local/bin/${bin}"
        ((count++)) || true
    done

    if [[ $count -gt 0 ]]; then
        ok "Installed $count binaries to /usr/local/bin/"
    fi
}

# ── Set up Python virtual environment ────────────────────────
setup_python_env() {
    info "Setting up Python virtual environment..."

    local venv_path="${AETHER_HOME}/.venv"

    if [[ -d "$venv_path" ]]; then
        skip "Virtual environment already exists at $venv_path"
        warn "  To recreate: rm -rf $venv_path && $0"
        return
    fi

    # Check for uv or pip
    if command -v uv &>/dev/null; then
        run uv venv "$venv_path"
        run uv sync --directory "${REPO_ROOT}" --link-mode copy
        ok "Python environment created with uv"
    elif command -v python3 &>/dev/null; then
        run python3 -m venv "$venv_path"
        run "$venv_path/bin/pip" install -r "${REPO_ROOT}/requirements.txt"
        ok "Python environment created with venv+pip"
    else
        fail "No Python toolchain found (uv or python3)"
        return 1
    fi

    run chown -R "$AETHER_USER":"$AETHER_GROUP" "$venv_path"
}

# ── Start Docker Compose data stack ──────────────────────────
start_compose_stack() {
    if [[ "$SKIP_COMPOSE" == true ]]; then
        skip "Docker Compose stack (--skip-compose)"
        return
    fi

    info "Starting production data stack..."

    local compose_file="${SCRIPT_DIR}/docker-compose.prod.yml"
    if [[ ! -f "$compose_file" ]]; then
        fail "Compose file not found: $compose_file"
        return 1
    fi

    # Source env file for compose variables
    if [[ -f "$AETHER_ENV_FILE" ]]; then
        set -a
        source "$AETHER_ENV_FILE"
        set +a
    else
        warn "Environment file not found at $AETHER_ENV_FILE"
        warn "  Compose may use defaults. Create $AETHER_ENV_FILE first."
    fi

    run docker compose -f "$compose_file" pull
    run docker compose -f "$compose_file" up -d --wait

    ok "Production data stack is running"
}

# ── Enable and start systemd services ────────────────────────
enable_services() {
    info "Enabling systemd services..."

    local target="aether.target"
    run systemctl enable "$target"

    # Don't start automatically in dry run or if compose hasn't started
    if [[ "$DRY_RUN" == false ]]; then
        run systemctl start "$target" || {
            fail "Failed to start aether.target — check service status with: systemctl status aether.target"
            return 1
        }
        ok "All AETHER services started"
    else
        skip "Service start skipped (dry-run)"
    fi
}

# ── Final summary ────────────────────────────────────────────
summary() {
    echo ""
    echo "========================================================"
    echo -e "${GREEN}AETHER Terminal — Installation Complete${NC}"
    echo "========================================================"
    echo ""
    echo "  Results:  ${OK} OK, ${WARN} Warnings, ${SKIP} Skipped, ${FAIL} Errors"
    echo ""

    if [[ "$DRY_RUN" == true ]]; then
        echo "  DRY RUN — no changes were made."
        echo ""
    fi

    echo "  Next steps:"
    echo "  1. Edit the environment file:"
    echo "       sudo nano $AETHER_ENV_FILE"
    echo ""
    echo "  2. Review and start the data stack:"
    echo "       sudo aetherctl compose-up"
    echo ""
    echo "  3. Start AETHER services:"
    echo "       sudo aetherctl start"
    echo ""
    echo "  4. Check status:"
    echo "       sudo aetherctl status"
    echo "       sudo aetherctl verify"
    echo ""
    echo "  5. Tail logs:"
    echo "       sudo aetherctl logs"
    echo ""
    echo "  6. For per-service logs:"
    echo "       journalctl -fu aether-gateway.service -n 100"
    echo "       journalctl -fu aether-brain.service -n 100"
    echo ""

    if [[ $FAIL -gt 0 ]]; then
        echo -e "  ${RED}${FAIL} step(s) failed — review output above.${NC}"
        exit 1
    fi

    echo "  AETHER terminal ready — authorized personnel only."
    echo "========================================================"
}

# ═══════════════════════════════════════════════════════════════
# Main
# ═══════════════════════════════════════════════════════════════
main() {
    echo ""
    echo "========================================================"
    echo " AETHER Terminal — Production Installation"
    echo " EP-407: Deployment & Release Engineering"
    echo "========================================================"
    echo ""

    preflight
    setup_user_and_dirs
    install_environment
    install_systemd_units
    install_nginx_config
    install_scripts
    install_binaries
    setup_python_env

    # Optional steps that depend on environment being ready
    if [[ "$DRY_RUN" == false ]]; then
        start_compose_stack
        enable_services
    else
        skip "Compose stack and service start (dry-run)"
    fi

    summary
}

main "$@"
