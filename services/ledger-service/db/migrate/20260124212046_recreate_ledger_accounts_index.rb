class RecreateLedgerAccountsIndex < ActiveRecord::Migration[7.1]
  def up
    # Drop the old index that was created with UUID type
    remove_index :ledger_accounts, 
      name: "index_ledger_accounts_on_org_env_external_currency", 
      if_exists: true
    
    # Recreate it with the new string type
    add_index :ledger_accounts, 
      [:organization_id, :environment, :external_account_id, :currency], 
      unique: true, 
      name: "index_ledger_accounts_on_org_env_external_currency"
  end

  def down
    # This migration is safe to reverse - just drop and recreate
    remove_index :ledger_accounts, 
      name: "index_ledger_accounts_on_org_env_external_currency", 
      if_exists: true
    
    add_index :ledger_accounts, 
      [:organization_id, :environment, :external_account_id, :currency], 
      unique: true, 
      name: "index_ledger_accounts_on_org_env_external_currency"
  end
end
