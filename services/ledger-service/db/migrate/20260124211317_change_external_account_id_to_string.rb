class ChangeExternalAccountIdToString < ActiveRecord::Migration[7.1]
  def up
    # Change external_account_id from uuid to string to support both UUIDs and system account IDs
    change_column :ledger_accounts, :external_account_id, :string, null: false
  end

  def down
    # Revert to uuid (this will fail if there are non-UUID values)
    change_column :ledger_accounts, :external_account_id, :uuid, null: false
  end
end
