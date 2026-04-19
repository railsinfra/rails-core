#!/bin/bash
# Run SQLx migrations using the same .env the service uses.
# Use this if the accounts service reports "column holder_id does not exist"
# or other schema errors - ensures migrations run against the correct database.

set -e
cd "$(dirname "${BASH_SOURCE[0]}")"

if [[ ! -f .env ]]; then
    echo "No .env found. Create one from .env.dev.example and set DATABASE_URL."
    exit 1
fi

set -a
source .env
set +a

if [[ -z "$DATABASE_URL" ]]; then
    echo "DATABASE_URL not set in .env"
    exit 1
fi

echo "Running migrations against database..."
sqlx migrate run --source migrations_accounts

echo "Migrations complete."
