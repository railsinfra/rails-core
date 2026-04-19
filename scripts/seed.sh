#!/usr/bin/env bash
# Placeholder: seed reference data when services expose a seed API/CLI.
# Today, data is provisioned in external Postgres (Neon); use each service's own tooling.
set -euo pipefail
echo "rails-core/scripts/seed.sh: no-op (use service-level seeds / migrations)."
echo "  - Rust: sqlx migrate run (inside each service repo)"
echo "  - Rails: rails db:seed (inside ledger-service)"
