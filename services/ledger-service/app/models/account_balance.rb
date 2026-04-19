# frozen_string_literal: true

class AccountBalance < ApplicationRecord
  self.table_name = 'account_balances'

  validates :organization_id, presence: true
  validates :environment, presence: true, inclusion: { in: %w[sandbox production] }
  validates :ledger_account_id, presence: true
  # Signed balance: debits increase (+), credits decrease (-).
  # Liability/equity/income accounts will naturally trend negative.
  validates :balance_cents, presence: true, numericality: true
  validates :currency, presence: true
  validates :last_updated_at, presence: true

  belongs_to :ledger_account

  # Update balance transactionally
  def self.update_balance!(organization_id:, environment:, ledger_account_id:, amount_cents:, currency:, entry_type:)
    ActiveRecord::Base.transaction do
      balance = find_or_initialize_by(
        organization_id: organization_id,
        environment: environment,
        ledger_account_id: ledger_account_id
      )

      # Calculate new balance based on entry type
      current_balance = balance.balance_cents || 0
      case entry_type
      when 'debit'
        new_balance = current_balance + amount_cents
      when 'credit'
        new_balance = current_balance - amount_cents
      else
        raise ArgumentError, "Invalid entry_type: #{entry_type}"
      end

      balance.balance_cents = new_balance
      balance.currency = currency
      balance.last_updated_at = Time.current
      balance.save!
    end
  end

  def self.get_balance(organization_id:, environment:, ledger_account_id:)
    find_by(
      organization_id: organization_id,
      environment: environment,
      ledger_account_id: ledger_account_id
    )&.balance_cents || 0
  end
end
