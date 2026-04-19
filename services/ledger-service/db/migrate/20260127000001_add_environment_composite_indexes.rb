# frozen_string_literal: true

class AddEnvironmentCompositeIndexes < ActiveRecord::Migration[7.1]
  def change
    # Add composite indexes with environment as leading column for efficient filtering
    # These indexes optimize queries that filter by environment + other criteria

    # Index for ledger_accounts queries with environment filter
    # Query pattern: WHERE organization_id = $1 AND environment = $2 AND external_account_id = $3
    add_index :ledger_accounts, [:environment, :organization_id, :external_account_id, :currency],
              name: 'idx_ledger_accounts_env_org_external_currency',
              unique: false # Note: unique constraint already exists on org_env_external_currency

    # Index for ledger_entries queries with environment filter
    # Query pattern: WHERE organization_id = $1 AND environment = $2 AND ledger_account_id = $3
    add_index :ledger_entries, [:environment, :organization_id, :ledger_account_id, :created_at],
              name: 'idx_ledger_entries_env_org_account_created'

    # Index for ledger_transactions queries with environment filter
    # Query pattern: WHERE organization_id = $1 AND environment = $2 ORDER BY created_at DESC
    add_index :ledger_transactions, [:environment, :organization_id, :created_at],
              name: 'idx_ledger_transactions_env_org_created'

    # Index for account_balances queries with environment filter
    # Query pattern: WHERE organization_id = $1 AND environment = $2 AND ledger_account_id = $3
    add_index :account_balances, [:environment, :organization_id, :ledger_account_id],
              name: 'idx_account_balances_env_org_account'
  end
end
