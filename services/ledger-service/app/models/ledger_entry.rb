# frozen_string_literal: true

class LedgerEntry < ApplicationRecord
  self.table_name = 'ledger_entries'

  validates :organization_id, presence: true
  validates :environment, presence: true, inclusion: { in: %w[sandbox production] }
  validates :ledger_account_id, presence: true
  validates :transaction_id, presence: true
  validates :entry_type, presence: true, inclusion: { in: %w[debit credit] }
  validates :amount, presence: true, numericality: { greater_than: 0 }
  validates :currency, presence: true

  belongs_to :ledger_account
  belongs_to :ledger_transaction, class_name: 'LedgerTransaction', foreign_key: 'transaction_id'

  enum entry_type: {
    debit: 'debit',
    credit: 'credit'
  }
end
