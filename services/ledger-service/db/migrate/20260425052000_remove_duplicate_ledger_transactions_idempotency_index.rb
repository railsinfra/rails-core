class RemoveDuplicateLedgerTransactionsIdempotencyIndex < ActiveRecord::Migration[7.1]
  def up
    remove_index :ledger_transactions,
                 name: "index_ledger_transactions_on_org_env_idempotency",
                 if_exists: true
  end

  def down
    add_index :ledger_transactions,
              [:organization_id, :environment, :idempotency_key],
              unique: true,
              name: "index_ledger_transactions_on_org_env_idempotency",
              if_not_exists: true
  end
end
