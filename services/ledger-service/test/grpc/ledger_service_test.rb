# frozen_string_literal: true

require "test_helper"

class LedgerServiceTest < ActiveSupport::TestCase
  test "post_transaction returns failed response when organization_id empty" do
    req = Rails::Ledger::V1::PostTransactionRequest.new(
      organization_id: "",
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      source_external_account_id: "a",
      destination_external_account_id: "b",
      amount: 1,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid,
      correlation_id: "c"
    )
    resp = LedgerService.new.post_transaction(req, nil)
    assert_equal "failed", resp.status
    assert_match(/organization_id/i, resp.failure_reason)
  end

  test "get_account_balance returns zero for unknown account" do
    req = Rails::Ledger::V1::GetAccountBalanceRequest.new(
      organization_id: SecureRandom.uuid,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      external_account_id: "missing-account",
      currency: "USD"
    )
    resp = LedgerService.new.get_account_balance(req, nil)
    assert_equal "0", resp.balance
    assert_equal "USD", resp.currency
  end

  test "get_account_balance raises on invalid environment" do
    req = Rails::Ledger::V1::GetAccountBalanceRequest.new(
      organization_id: SecureRandom.uuid,
      environment: 99_999,
      external_account_id: "x",
      currency: "USD"
    )
    assert_raises(GRPC::InvalidArgument) do
      LedgerService.new.get_account_balance(req, nil)
    end
  end
end
