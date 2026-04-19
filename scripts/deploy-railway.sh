#!/bin/bash
set -e

# =============================================================================
# Railway Deployment Helper (gRPC-only)
# =============================================================================
# Deploys the MVP Rust services (accounts + users) to Railway.
#
# Usage (from repository root):
#   ./scripts/deploy-railway.sh [command]
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
RAILS_CORE="$(cd "$SCRIPT_DIR/.." && pwd)"

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
  cd "${RAILS_CORE}/services/accounts-service"
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
  cd "${RAILS_CORE}/services/users-service"
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

