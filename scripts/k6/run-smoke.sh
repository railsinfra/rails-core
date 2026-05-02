#!/usr/bin/env bash
# Run from repo root via Makefile. Sources .env so INTERNAL_SERVICE_TOKEN_ALLOWLIST / K6_INTERNAL_SERVICE_TOKEN work.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ -f "${ROOT}/.env" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "${ROOT}/.env"
  set +a
fi
export K6_TARGET="${K6_TARGET:-docker}"
exec k6 run "${ROOT}/scripts/k6/smoke-user-account-money.js"
