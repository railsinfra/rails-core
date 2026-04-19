#!/bin/bash
# Run the pending migration SQL directly

DATABASE_URL=$(grep DATABASE_URL .env | cut -d= -f2-)

echo "Running migration 20260122000007_intent_transactions_and_remove_balance.sql..."

# Read the migration file and execute each statement
cat migrations_accounts/20260122000007_intent_transactions_and_remove_balance.sql | \
  sqlx database execute --database-url "$DATABASE_URL" 2>&1

if [ $? -eq 0 ]; then
  echo "Migration applied successfully!"
  # Mark migration as applied in _sqlx_migrations table
  sqlx migrate add --ignore-missing --source ./migrations_accounts 20260122000007_intent_transactions_and_remove_balance 2>/dev/null || true
else
  echo "Migration failed. Check the error above."
  exit 1
fi
