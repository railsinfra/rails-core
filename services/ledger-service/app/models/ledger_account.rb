# frozen_string_literal: true

class LedgerAccount < ApplicationRecord
  self.table_name = 'ledger_accounts'

  validates :organization_id, presence: true
  validates :environment, presence: true, inclusion: { in: %w[sandbox production] }
  validates :external_account_id, presence: true
  validates :account_type, presence: true, inclusion: { in: %w[asset liability equity income expense] }
  validates :currency, presence: true

  has_many :debit_entries, class_name: 'LedgerEntry', foreign_key: 'ledger_account_id'
  has_many :credit_entries, class_name: 'LedgerEntry', foreign_key: 'ledger_account_id'
  has_one :account_balance, dependent: :destroy

  enum account_type: {
    asset: 'asset',
    liability: 'liability', 
    equity: 'equity',
    income: 'income',
    expense: 'expense'
  }

  def self.resolve(organization_id:, environment:, external_account_id:, currency:, account_type: 'liability')
    Rails.logger.info(
      "LedgerAccount.resolve called " \
      "external_account_id=#{external_account_id.inspect} " \
      "type=#{external_account_id.class} " \
      "org=#{organization_id.inspect} env=#{environment.inspect}"
    )
    result = find_or_create_by!(
      organization_id: organization_id,
      environment: environment,
      external_account_id: external_account_id,
      currency: currency
    ) do |account|
      account.account_type = account_type
    end
    Rails.logger.info("LedgerAccount.resolve result: id=#{result.id} external_account_id=#{result.external_account_id.inspect}")
    result
  end

  def current_balance
    balance = account_balance&.balance_cents || 0
    # Apply account type semantics for display
    case account_type
    when 'asset', 'expense'
      balance # Debits increase, credits decrease
    when 'liability', 'equity', 'income'
      -balance # Credits increase, debits decrease
    else
      balance
    end
  end

  def self.create_control_accounts(organization_id:, environment:, currency:)
    {
      cash_control: resolve(
        organization_id: organization_id,
        environment: environment,
        external_account_id: 'SYSTEM_CASH_CONTROL',
        currency: currency,
        account_type: 'asset'
      ),
      bank_clearing: resolve(
        organization_id: organization_id,
        environment: environment,
        external_account_id: 'SYSTEM_BANK_CLEARING', 
        currency: currency,
        account_type: 'asset'
      ),
      fee_income: resolve(
        organization_id: organization_id,
        environment: environment,
        external_account_id: 'SYSTEM_FEE_INCOME',
        currency: currency,
        account_type: 'income'
      )
    }
  end
end
