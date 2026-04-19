#!/usr/bin/env bash
# Verify vendored service trees are present (no submodules).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MANIFEST="$REPO_ROOT/config/services.json"

if ! git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "error: not a git repository: $REPO_ROOT" >&2
  exit 1
fi

if ! python3 -c "import tabulate" 2>/dev/null; then
  python3 -m pip install -q -r "$SCRIPT_DIR/lib/requirements.txt"
fi

echo "Repository root: $REPO_ROOT"
echo "Checking vendored services from $MANIFEST"
if ! python3 "$SCRIPT_DIR/lib/print_vendor_services_check.py" "$REPO_ROOT" "$MANIFEST"; then
  exit 1
fi

echo ""
echo "Database environment (Neon or existing .env)..."
if ! python3 "$SCRIPT_DIR/lib/neon_bootstrap.py" --repo-root "$REPO_ROOT"; then
  exit 1
fi

echo "Done."
