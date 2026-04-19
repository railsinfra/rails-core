#!/usr/bin/env bash
# Full-stack smoke: gateway health JSON, then cross-service contract flow.
# Expects Docker Compose (or equivalent) already listening on GATEWAY_URL.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export GATEWAY_URL="${GATEWAY_URL:-http://127.0.0.1:8080}"

echo "== Health (gateway) =="
python3 "$ROOT/scripts/lib/health_check.py"

echo ""
echo "== Contract: register → API key → account → deposit → ledger entries =="
python3 "$ROOT/tests/contracts/smoke_flow.py"

echo ""
echo "OK  system test passed"
