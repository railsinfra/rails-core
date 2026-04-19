#!/usr/bin/env bash
# Tear down local docker-compose stack for rails-core (does not drop external databases).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RAILS_CORE="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$RAILS_CORE"
if [[ -f .env ]]; then
  docker compose --env-file .env down --remove-orphans "$@"
else
  docker compose down --remove-orphans "$@"
fi
echo "Compose stack stopped."
