#!/usr/bin/env bash
# Initialize git submodules for backend services.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MANIFEST="$REPO_ROOT/config/services.json"

if ! git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "error: not a git repository: $REPO_ROOT" >&2
  exit 1
fi

echo "Repository root: $REPO_ROOT"
echo "Initializing submodules from $MANIFEST"
cd "$REPO_ROOT"

while IFS= read -r rel; do
  [[ -z "$rel" ]] && continue
  echo "  -> $rel"
  git submodule update --init "$rel"
done < <(python3 "$SCRIPT_DIR/lib/read_manifest.py" "$MANIFEST")

if [[ ! -f "$REPO_ROOT/.env" ]]; then
  echo ""
  echo "Tip: cp .env.example .env and set database URLs."
fi

echo "Done."
