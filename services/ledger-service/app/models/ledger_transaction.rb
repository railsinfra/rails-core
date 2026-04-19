# frozen_string_literal: true

class LedgerTransaction < ApplicationRecord
  self.table_name = 'ledger_transactions'

  validates :organization_id, presence: true
  validates :environment, presence: true, inclusion: { in: %w[sandbox production] }
  validates :external_transaction_id, presence: true
  validates :status, presence: true, inclusion: { in: %w[pending posted failed] }
  validates :idempotency_key, presence: true

  has_many :ledger_entries, dependent: :destroy

  enum status: {
    pending: 'pending',
    posted: 'posted',
    failed: 'failed'
  }

  def self.find_existing(organization_id:, environment:, idempotency_key:)
    find_by(
      organization_id: organization_id,
      environment: environment,
      idempotency_key: idempotency_key
    )
  end

  def mark_as_posted!
    update!(status: 'posted', failure_reason: nil)
  end

  def mark_as_failed!(reason:)
    update!(status: 'failed', failure_reason: reason)
  end
end
