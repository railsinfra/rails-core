#!/usr/bin/env bash
# Optional smoke checks via nginx gateway (see docs/quickstart.md).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$SCRIPT_DIR/lib/health_check.py"
