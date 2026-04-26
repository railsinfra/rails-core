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

  test "get_account_balances returns zeros when both accounts unknown" do
    org = SecureRandom.uuid
    req = Rails::Ledger::V1::GetAccountBalancesRequest.new(
      organization_id: org,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      from_external_account_id: "from-unknown-bal",
      to_external_account_id: "to-unknown-bal",
      currency: "USD"
    )
    resp = LedgerService.new.get_account_balances(req, nil)
    assert_equal "0", resp.from_balance
    assert_equal "0", resp.to_balance
    assert_equal "USD", resp.currency
  end

  test "get_account_balances returns from balance when only from account exists" do
    org = SecureRandom.uuid
    from_ext = "grpc_from_only"
    from_acct = LedgerAccount.resolve(
      organization_id: org,
      environment: "sandbox",
      external_account_id: from_ext,
      currency: "USD",
      account_type: "liability"
    )
    AccountBalance.update_balance!(
      organization_id: org,
      environment: "sandbox",
      ledger_account_id: from_acct.id,
      amount_cents: 7,
      currency: "USD",
      entry_type: "debit"
    )
    req = Rails::Ledger::V1::GetAccountBalancesRequest.new(
      organization_id: org,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      from_external_account_id: from_ext,
      to_external_account_id: "to-missing-only",
      currency: "USD"
    )
    resp = LedgerService.new.get_account_balances(req, nil)
    assert_equal "7", resp.from_balance
    assert_equal "0", resp.to_balance
  end

  test "get_account_balances returns stored balances for both accounts" do
    org = SecureRandom.uuid
    from_ext = "grpc_dual_from"
    to_ext = "grpc_dual_to"
    from_acct = LedgerAccount.resolve(
      organization_id: org,
      environment: "sandbox",
      external_account_id: from_ext,
      currency: "USD",
      account_type: "liability"
    )
    to_acct = LedgerAccount.resolve(
      organization_id: org,
      environment: "sandbox",
      external_account_id: to_ext,
      currency: "USD",
      account_type: "liability"
    )
    AccountBalance.update_balance!(
      organization_id: org,
      environment: "sandbox",
      ledger_account_id: from_acct.id,
      amount_cents: 11,
      currency: "USD",
      entry_type: "debit"
    )
    AccountBalance.update_balance!(
      organization_id: org,
      environment: "sandbox",
      ledger_account_id: to_acct.id,
      amount_cents: 22,
      currency: "USD",
      entry_type: "debit"
    )
    req = Rails::Ledger::V1::GetAccountBalancesRequest.new(
      organization_id: org,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      from_external_account_id: from_ext,
      to_external_account_id: to_ext,
      currency: "USD"
    )
    resp = LedgerService.new.get_account_balances(req, nil)
    assert_equal "11", resp.from_balance
    assert_equal "22", resp.to_balance
    assert_equal "USD", resp.currency
  end

  test "get_account_balances raises on invalid environment" do
    req = Rails::Ledger::V1::GetAccountBalancesRequest.new(
      organization_id: SecureRandom.uuid,
      environment: 99_999,
      from_external_account_id: "a",
      to_external_account_id: "b",
      currency: "USD"
    )
    assert_raises(GRPC::InvalidArgument) do
      LedgerService.new.get_account_balances(req, nil)
    end
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

  test "proto_env_to_string normalizes sandbox and production" do
    svc = LedgerService.new
    assert_equal "sandbox", svc.send(:proto_env_to_string, :SANDBOX)
    assert_equal "sandbox", svc.send(:proto_env_to_string, "sandbox")
    assert_equal "production", svc.send(:proto_env_to_string, Rails::Ledger::V1::Environment::PRODUCTION)
    assert_raises(GRPC::InvalidArgument) do
      svc.send(:proto_env_to_string, :NOT_AN_ENV)
    end
  end

  test "post_transaction succeeds end to end" do
    org = SecureRandom.uuid
    req = Rails::Ledger::V1::PostTransactionRequest.new(
      organization_id: org,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      source_external_account_id: "SYSTEM_CASH_CONTROL",
      destination_external_account_id: "grpc_success_user",
      amount: 2,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid,
      correlation_id: "corr-1"
    )
    resp = LedgerService.new.post_transaction(req, nil)
    assert_equal "posted", resp.status
    assert resp.ledger_transaction_id.present?
  end

  test "post_transaction rejects empty currency" do
    req = Rails::Ledger::V1::PostTransactionRequest.new(
      organization_id: SecureRandom.uuid,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      source_external_account_id: "a",
      destination_external_account_id: "b",
      amount: 1,
      currency: "",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid,
      correlation_id: ""
    )
    resp = LedgerService.new.post_transaction(req, nil)
    assert_equal "failed", resp.status
    assert_match(/currency/i, resp.failure_reason)
  end

  test "post_transaction rejects empty external_transaction_id" do
    req = Rails::Ledger::V1::PostTransactionRequest.new(
      organization_id: SecureRandom.uuid,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      source_external_account_id: "a",
      destination_external_account_id: "b",
      amount: 1,
      currency: "USD",
      external_transaction_id: "",
      idempotency_key: SecureRandom.uuid,
      correlation_id: ""
    )
    resp = LedgerService.new.post_transaction(req, nil)
    assert_equal "failed", resp.status
    assert_match(/external_transaction_id/i, resp.failure_reason)
  end

  test "post_transaction rejects empty idempotency_key" do
    req = Rails::Ledger::V1::PostTransactionRequest.new(
      organization_id: SecureRandom.uuid,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      source_external_account_id: "a",
      destination_external_account_id: "b",
      amount: 1,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: "",
      correlation_id: ""
    )
    resp = LedgerService.new.post_transaction(req, nil)
    assert_equal "failed", resp.status
    assert_match(/idempotency_key/i, resp.failure_reason)
  end

  test "post_transaction fails when proto environment is invalid before other fields bind" do
    req = Rails::Ledger::V1::PostTransactionRequest.new(
      organization_id: SecureRandom.uuid,
      environment: 99_999,
      source_external_account_id: "a",
      destination_external_account_id: "b",
      amount: 1,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid,
      correlation_id: ""
    )
    resp = LedgerService.new.post_transaction(req, nil)
    assert_equal "failed", resp.status
    assert_match(/Invalid environment/i, resp.failure_reason)
  end

  test "post_transaction rejects empty source or destination" do
    svc = LedgerService.new
    base = {
      organization_id: SecureRandom.uuid,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      amount: 1,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid,
      correlation_id: ""
    }
    req_src = Rails::Ledger::V1::PostTransactionRequest.new(
      **base.merge(source_external_account_id: "", destination_external_account_id: "b")
    )
    assert_match(/source_external_account_id/i, svc.post_transaction(req_src, nil).failure_reason)

    req_dst = Rails::Ledger::V1::PostTransactionRequest.new(
      **base.merge(source_external_account_id: "a", destination_external_account_id: "")
    )
    assert_match(/destination_external_account_id/i, svc.post_transaction(req_dst, nil).failure_reason)
  end

  test "get_account_balance returns stored balance for existing account" do
    org = SecureRandom.uuid
    ext = "grpc_balance_acct"
    acct = LedgerAccount.resolve(
      organization_id: org,
      environment: "sandbox",
      external_account_id: ext,
      currency: "USD",
      account_type: "liability"
    )
    AccountBalance.update_balance!(
      organization_id: org,
      environment: "sandbox",
      ledger_account_id: acct.id,
      amount_cents: 42,
      currency: "USD",
      entry_type: "debit"
    )

    req = Rails::Ledger::V1::GetAccountBalanceRequest.new(
      organization_id: org,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      external_account_id: ext,
      currency: "USD"
    )
    resp = LedgerService.new.get_account_balance(req, nil)
    assert_equal "42", resp.balance
  end

  test "post_transaction rescue handles nil sentry scope" do
    req = Rails::Ledger::V1::PostTransactionRequest.new(
      organization_id: SecureRandom.uuid,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      source_external_account_id: "SYSTEM_CASH_CONTROL",
      destination_external_account_id: "grpc_sentry_nil",
      amount: 1,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid,
      correlation_id: "c"
    )
    with_stub(LedgerPoster, :post, proc { raise StandardError, "inner" }) do
      with_stub(Sentry, :with_scope, proc { |&block| block.call(nil) }) do
        with_stub(Sentry, :capture_exception, proc { |_e| nil }) do
          resp = LedgerService.new.post_transaction(req, nil)
          assert_equal "failed", resp.status
          assert_match(/inner/, resp.failure_reason)
        end
      end
    end
  end

  test "post_transaction rescue handles sentry capture failure" do
    scope_obj = Object.new
    def scope_obj.set_context(*); end
    def scope_obj.set_tag(*); end

    req = Rails::Ledger::V1::PostTransactionRequest.new(
      organization_id: SecureRandom.uuid,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      source_external_account_id: "SYSTEM_CASH_CONTROL",
      destination_external_account_id: "grpc_sentry_fail",
      amount: 1,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid,
      correlation_id: "c"
    )
    with_stub(LedgerPoster, :post, proc { raise StandardError, "boom" }) do
      with_stub(Sentry, :with_scope, proc { |&block| block.call(scope_obj) }) do
        with_stub(Sentry, :capture_exception, proc { raise StandardError, "sentry" }) do
          resp = LedgerService.new.post_transaction(req, nil)
          assert_equal "failed", resp.status
          assert_match(/boom/, resp.failure_reason)
        end
      end
    end
  end

  test "post_transaction rescue skips Sentry when with_scope is unavailable" do
    req = Rails::Ledger::V1::PostTransactionRequest.new(
      organization_id: SecureRandom.uuid,
      environment: Rails::Ledger::V1::Environment::SANDBOX,
      source_external_account_id: "SYSTEM_CASH_CONTROL",
      destination_external_account_id: "grpc_skip_sentry_reporting",
      amount: 1,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid,
      correlation_id: "c"
    )
    without_sentry_with_scope_method do
      with_stub(LedgerPoster, :post, proc { raise StandardError, "post_without_sentry" }) do
        resp = LedgerService.new.post_transaction(req, nil)
        assert_equal "failed", resp.status
        assert_match(/post_without_sentry/, resp.failure_reason)
      end
    end
  end
end
