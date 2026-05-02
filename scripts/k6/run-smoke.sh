#!/usr/bin/env bash
# Run from repo root via Makefile. Sources .env so INTERNAL_SERVICE_TOKEN_ALLOWLIST / K6_INTERNAL_SERVICE_TOKEN work.
# Uses native `k6` when installed; otherwise runs `grafana/k6` via Docker (reaches host gateway via host.docker.internal).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ -f "${ROOT}/.env" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "${ROOT}/.env"
  set +a
fi
export K6_TARGET="${K6_TARGET:-docker}"

SCRIPT="${ROOT}/scripts/k6/smoke-user-account-money.js"

run_native() {
  exec k6 run "$SCRIPT"
}

run_docker() {
  # Inside a container, localhost is not the host; default gateway unless caller set K6_GATEWAY_URL.
  if [[ -z "${K6_GATEWAY_URL:-}" && "${K6_TARGET}" == "docker" ]]; then
    export K6_GATEWAY_URL="http://host.docker.internal:8080"
  fi
  local env_file
  env_file="$(mktemp)"
  # shellcheck disable=SC2046
  env | grep -E '^K6_' | grep -v '^K6_$' >"$env_file" || true
  docker run --rm -i \
    --add-host=host.docker.internal:host-gateway \
    --env-file "$env_file" \
    -v "${ROOT}/scripts/k6:/k6:ro" \
    -w /k6 \
    grafana/k6:latest run smoke-user-account-money.js
  rm -f "$env_file"
}

if command -v k6 >/dev/null 2>&1; then
  run_native
elif command -v docker >/dev/null 2>&1; then
  run_docker
else
  echo "Neither k6 nor docker is available. Install k6: https://k6.io/docs/get-started/installation/" >&2
  echo "Or install Docker to run k6 in grafana/k6 automatically." >&2
  exit 1
fi
