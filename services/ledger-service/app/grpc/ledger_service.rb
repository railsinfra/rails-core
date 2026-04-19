# frozen_string_literal: true

require 'grpc'
require 'google/protobuf'
# Protobuf service definitions are loaded from `lib/grpc/**/*_pb.rb` by
# `config/initializers/grpc.rb` before the server starts.

class LedgerService < Rails::Ledger::V1::LedgerService::Service
  def post_transaction(request, _call)
    raw_org = request.organization_id
    raw_source = request.source_external_account_id
    raw_dest = request.destination_external_account_id

    organization_id = raw_org.to_s
    environment = proto_env_to_string(request.environment)
    source_external_account_id = raw_source.to_s
    destination_external_account_id = raw_dest.to_s
    amount = request.amount
    currency = request.currency.to_s
    external_transaction_id = request.external_transaction_id.to_s
    idempotency_key = request.idempotency_key.to_s
    correlation_id = request.correlation_id.to_s

    Rails.logger.info "[LEDGER_GRPC] post_transaction START org=#{organization_id} env=#{environment} amount=#{amount} #{currency} source=#{source_external_account_id} dest=#{destination_external_account_id} tx_id=#{external_transaction_id} idem=#{idempotency_key} correlation=#{correlation_id}"

    # Reject malformed requests early (avoids creating half-baked ledger rows).
    raise GRPC::InvalidArgument.new('organization_id is required') if organization_id.empty?
    raise GRPC::InvalidArgument.new('currency is required') if currency.empty?
    raise GRPC::InvalidArgument.new('external_transaction_id is required') if external_transaction_id.empty?
    raise GRPC::InvalidArgument.new('idempotency_key is required') if idempotency_key.empty?
    raise GRPC::InvalidArgument.new('source_external_account_id is required') if source_external_account_id.empty?
    raise GRPC::InvalidArgument.new('destination_external_account_id is required') if destination_external_account_id.empty?

    result = LedgerPoster.post(
      organization_id: organization_id,
      environment: environment,
      source_external_account_id: source_external_account_id,
      destination_external_account_id: destination_external_account_id,
      amount: amount,
      currency: currency,
      external_transaction_id: external_transaction_id,
      idempotency_key: idempotency_key,
      correlation_id: correlation_id
    )

    Rails.logger.info "[LEDGER_GRPC] post_transaction SUCCESS org=#{organization_id} env=#{environment} tx_id=#{external_transaction_id} ledger_id=#{result.id}"
    Rails::Ledger::V1::PostTransactionResponse.new(
      status: 'posted',
      ledger_transaction_id: result.id.to_s,
      failure_reason: ''
    )
  rescue => e
    # Report gRPC errors to Sentry
    if defined?(Sentry) && Sentry.respond_to?(:with_scope) && Sentry.respond_to?(:capture_exception)
      begin
        Sentry.with_scope do |scope|
          if scope
            scope.set_context('grpc_request', {
              organization_id: organization_id,
              environment: environment,
              external_transaction_id: external_transaction_id,
              idempotency_key: idempotency_key,
              correlation_id: correlation_id,
              method: 'post_transaction'
            })
            scope.set_tag('error_type', 'grpc_ledger_posting_failed')
            scope.set_tag('organization_id', organization_id.to_s)
            scope.set_tag('environment', environment)
          end
          Sentry.capture_exception(e)
        end
      rescue => sentry_error
        Rails.logger.warn "Sentry reporting failed: #{sentry_error.message}"
      end
    end

    Rails.logger.warn "[LEDGER_GRPC] post_transaction FAILED tx_id=#{external_transaction_id rescue 'unknown'} error=#{e.class}: #{e.message}"
    Rails::Ledger::V1::PostTransactionResponse.new(
      status: 'failed',
      ledger_transaction_id: '',
      failure_reason: e.message
    )
  end

  def get_account_balance(request, _call)
    organization_id = request.organization_id
    environment = proto_env_to_string(request.environment)
    external_account_id = request.external_account_id
    currency = request.currency

    Rails.logger.info "[LEDGER_GRPC] get_account_balance START org=#{organization_id} env=#{environment} account=#{external_account_id} currency=#{currency}"

    account = LedgerAccount.find_by(
      organization_id: organization_id,
      environment: environment,
      external_account_id: external_account_id,
      currency: currency
    )

    # Industry standard: return 0 for new/unknown accounts (no prior activity)
    balance = if account
      AccountBalance.get_balance(
        organization_id: organization_id,
        environment: environment,
        ledger_account_id: account.id
      )
    else
      0
    end

    Rails.logger.info "[LEDGER_GRPC] get_account_balance SUCCESS org=#{organization_id} account=#{external_account_id} balance=#{balance}"
    Rails::Ledger::V1::GetAccountBalanceResponse.new(
      balance: balance.to_s,
      currency: currency
    )
  end

  private

  def proto_env_to_string(proto_env)
    # Ruby protobuf enum fields can come through as Symbols (e.g. :SANDBOX)
    # or Integers (e.g. 1). Normalize defensively.
    case proto_env.to_s.downcase
    when 'sandbox', Rails::Ledger::V1::Environment::SANDBOX.to_s
      'sandbox'
    when 'production', Rails::Ledger::V1::Environment::PRODUCTION.to_s
      'production'
    else
      raise GRPC::InvalidArgument.new('Invalid environment')
    end
  end
end
