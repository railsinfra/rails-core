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

echo "Repository root: $REPO_ROOT"
echo "Checking vendored services from $MANIFEST"

missing=0
while IFS= read -r rel; do
  [[ -z "$rel" ]] && continue
  if [[ ! -d "$REPO_ROOT/$rel" ]]; then
    echo "MISSING: $rel" >&2
    missing=$((missing + 1))
    continue
  fi
  echo "  OK $rel"
done < <(python3 "$SCRIPT_DIR/lib/read_manifest.py" "$MANIFEST")

if [[ "$missing" -gt 0 ]]; then
  exit 1
fi

if [[ ! -f "$REPO_ROOT/.env" ]]; then
  echo ""
  echo "Tip: cp .env.example .env and set database URLs."
fi

echo "Done."
