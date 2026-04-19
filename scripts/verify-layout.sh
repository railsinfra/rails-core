#!/usr/bin/env bash
# Verify each path in config/services.json exists under the repo root (submodule checkout).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MANIFEST="$REPO_ROOT/config/services.json"

if ! git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "error: not a git repository: $REPO_ROOT" >&2
  exit 1
fi

errors=0
while IFS= read -r rel; do
  [[ -z "$rel" ]] && continue
  full="$REPO_ROOT/$rel"
  if [[ ! -d "$full" ]]; then
    echo "MISSING directory: $rel" >&2
    errors=$((errors + 1))
    continue
  fi
  if [[ ! -e "$full/.git" ]]; then
    echo "WARN: no .git under (expected submodule): $rel" >&2
  fi
  echo "OK $rel"
done < <(python3 "$SCRIPT_DIR/lib/read_manifest.py" "$MANIFEST")

if [[ "$errors" -gt 0 ]]; then
  echo "verify-layout: $errors path(s) missing. Run: make bootstrap" >&2
  exit 1
fi
echo "All declared paths present."
