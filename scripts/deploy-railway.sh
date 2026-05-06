#!/bin/bash
set -e

# =============================================================================
# Railway Deployment Helper (gRPC-only)
# =============================================================================
# Deploys the MVP Rust services (accounts + users) to Railway.
#
# Usage (from monorepo app root — parent of scripts/):
#   ./scripts/deploy-railway.sh [command]
#
# If each Rust service is a standalone repo, set ACCOUNTS_SERVICE_DIR / USERS_SERVICE_DIR
# to the folder that contains Dockerfile + Cargo.toml, then run this script from anywhere.
#
# Commands:
#   accounts    Deploy accounts service
#   users       Deploy users service
#   all         Deploy accounts then users
#   status      Show project status
#   logs        Tail logs for a service
#   help        Show help
# =============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Parent of scripts/ is the monorepo app root (contains gateway, docker-compose, etc.).
MONO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Override with absolute paths when each service is its own clone (no shared parent layout).
accounts_crate_dir() {
  if [ -n "${ACCOUNTS_SERVICE_DIR:-}" ]; then
    printf '%s' "${ACCOUNTS_SERVICE_DIR}"
    return
  fi
  if [ -f "${MONO_ROOT}/accounts-service/Cargo.toml" ]; then
    printf '%s' "${MONO_ROOT}/accounts-service"
    return
  fi
  printf '%s' "${MONO_ROOT}/services/accounts-service"
}

users_crate_dir() {
  if [ -n "${USERS_SERVICE_DIR:-}" ]; then
    printf '%s' "${USERS_SERVICE_DIR}"
    return
  fi
  if [ -f "${MONO_ROOT}/users-service/Cargo.toml" ]; then
    printf '%s' "${MONO_ROOT}/users-service"
    return
  fi
  printf '%s' "${MONO_ROOT}/services/users-service"
}

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

require_railway() {
  if ! command -v railway >/dev/null 2>&1; then
    log_error "Railway CLI not found. Install with: npm install -g @railway/cli"
    exit 1
  fi
  if ! railway whoami >/dev/null 2>&1; then
    log_error "Not logged in to Railway. Run: railway login"
    exit 1
  fi
}

deploy_accounts() {
  log_info "Deploying accounts service..."
  local dir
  dir="$(accounts_crate_dir)"
  if [ ! -f "${dir}/Dockerfile" ]; then
    log_error "Accounts crate not found at ${dir}. Set ACCOUNTS_SERVICE_DIR or run from a monorepo with the default layout."
    exit 1
  fi
  cd "${dir}"
  railway link
  railway up --detach
  log_success "Accounts deployment initiated"
  echo ""
  echo "Set required variables in Railway Dashboard:"
  echo "  DATABASE_URL"
  echo "  PORT"
  echo "  GRPC_PORT"
  echo "  HOST"
  echo "  RUST_LOG"
}

deploy_users() {
  log_info "Deploying users service..."
  local dir
  dir="$(users_crate_dir)"
  if [ ! -f "${dir}/Dockerfile" ]; then
    log_error "Users crate not found at ${dir}. Set USERS_SERVICE_DIR or run from a monorepo with the default layout."
    exit 1
  fi
  cd "${dir}"
  railway link
  railway up --detach
  log_success "Users deployment initiated"
  echo ""
  echo "Set required variables in Railway Dashboard:"
  echo "  DATABASE_URL"
  echo "  SERVER_ADDR (or HOST + PORT depending on your setup)"
  echo "  RUST_LOG"
  echo "  ACCOUNTS_GRPC_URL"
  echo "  API_KEY_HASH_SECRET"
  echo "  INTERNAL_SERVICE_TOKEN_ALLOWLIST (recommended)"
}

show_status() {
  railway status
}

show_logs() {
  read -p "Enter Railway service name (e.g. users-service): " service_name
  if [ -z "$service_name" ]; then
    log_error "No service name provided"
    exit 1
  fi
  railway logs --service "$service_name"
}

show_help() {
  cat <<'EOF'
Usage: ./deploy-railway.sh [command]

Commands:
  accounts    Deploy accounts service
  users       Deploy users service
  all         Deploy accounts then users
  status      Show project status
  logs        Tail logs for a service
  help        Show help
EOF
}

require_railway

case "${1:-help}" in
  accounts) deploy_accounts ;;
  users) deploy_users ;;
  all) deploy_accounts; echo ""; deploy_users ;;
  status) show_status ;;
  logs) show_logs ;;
  help|--help|-h) show_help ;;
  *) log_error "Unknown command: ${1}"; show_help; exit 1 ;;
esac

