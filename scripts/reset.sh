#!/usr/bin/env bash
# Tear down local docker-compose stack for rails-core (does not drop external databases by default).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RAILS_CORE="$(cd "$SCRIPT_DIR/.." && pwd)"

if [[ "${1:-}" == "--clear-env" ]]; then
  if ! python3 -c "import tabulate" 2>/dev/null; then
    python3 -m pip install -q -r "$SCRIPT_DIR/lib/requirements.txt"
  fi
  python3 "$SCRIPT_DIR/lib/neon_bootstrap.py" --repo-root "$RAILS_CORE" --clear-env
  exit $?
fi

if [[ "${1:-}" == "--purge-neon" ]]; then
  if ! python3 -c "import tabulate" 2>/dev/null; then
    python3 -m pip install -q -r "$SCRIPT_DIR/lib/requirements.txt"
  fi
  python3 "$SCRIPT_DIR/lib/neon_bootstrap.py" --repo-root "$RAILS_CORE" --purge-neon
  exit $?
fi

cd "$RAILS_CORE"
if [[ -f .env ]]; then
  docker compose --env-file .env down --remove-orphans "$@"
else
  docker compose down --remove-orphans "$@"
fi
echo "Compose stack stopped."
