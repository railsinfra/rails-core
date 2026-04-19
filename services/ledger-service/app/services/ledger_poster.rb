# frozen_string_literal: true

class LedgerPoster
  class PostingError < StandardError; end
  class IdempotencyError < StandardError; end
  class InvalidAccountTypeError < StandardError; end

  def self.post(organization_id:, environment:, source_external_account_id:, destination_external_account_id:, amount:, currency:, external_transaction_id:, idempotency_key:, correlation_id: nil)
    new(
      organization_id: organization_id,
      environment: environment,
      source_external_account_id: source_external_account_id,
      destination_external_account_id: destination_external_account_id,
      amount: amount,
      currency: currency,
      external_transaction_id: external_transaction_id,
      idempotency_key: idempotency_key,
      correlation_id: correlation_id
    ).call
  end

  def self.post_deposit(organization_id:, environment:, destination_external_account_id:, amount:, currency:, external_transaction_id:, idempotency_key:, correlation_id: nil)
    new(
      organization_id: organization_id,
      environment: environment,
      source_external_account_id: 'SYSTEM_CASH_CONTROL',
      destination_external_account_id: destination_external_account_id,
      amount: amount,
      currency: currency,
      external_transaction_id: external_transaction_id,
      idempotency_key: idempotency_key,
      correlation_id: correlation_id,
      is_deposit: true
    ).call
  end

  def initialize(organization_id:, environment:, source_external_account_id:, destination_external_account_id:, amount:, currency:, external_transaction_id:, idempotency_key:, correlation_id: nil, is_deposit: false)
    @organization_id = organization_id
    @environment = environment
    @source_external_account_id = source_external_account_id
    @destination_external_account_id = destination_external_account_id
    @amount = amount
    @currency = currency
    @external_transaction_id = external_transaction_id
    @idempotency_key = idempotency_key
    @correlation_id = correlation_id
    @is_deposit = is_deposit
  end

  def call
    validate_inputs!
    existing = LedgerTransaction.find_existing(
      organization_id: @organization_id,
      environment: @environment,
      idempotency_key: @idempotency_key
    )

    return existing_result(existing) if existing

    LedgerTransaction.transaction do
      ledger_transaction = create_ledger_transaction!
      source_account = resolve_ledger_account(@source_external_account_id, determine_source_account_type)
      destination_account = resolve_ledger_account(@destination_external_account_id, determine_destination_account_type)

      validate_account_types!(source_account, destination_account)
      source_entry_type, destination_entry_type = create_double_entry!(ledger_transaction, source_account, destination_account)
      update_balances!(source_account, destination_account, source_entry_type, destination_entry_type)
      ledger_transaction.mark_as_posted!

      ledger_transaction
    end
  rescue => e
    Rails.logger.error(
      "Ledger posting failed: #{e.message} " \
      "(organization_id: #{@organization_id}, environment: #{@environment}, " \
      "external_transaction_id: #{@external_transaction_id}, idempotency_key: #{@idempotency_key}, " \
      "correlation_id: #{@correlation_id})"
    )
    
    # Report critical ledger errors to Sentry
    if defined?(Sentry) && Sentry.respond_to?(:with_scope) && Sentry.respond_to?(:capture_exception)
      begin
        Sentry.with_scope do |scope|
          # Some misconfigured Sentry setups can yield nil scope; don't crash ledger posting.
          if scope
            scope.set_context('ledger_posting', {
              organization_id: @organization_id,
              environment: @environment,
              external_transaction_id: @external_transaction_id,
              idempotency_key: @idempotency_key,
              correlation_id: @correlation_id,
              source_account: @source_external_account_id,
              destination_account: @destination_external_account_id,
              amount: @amount,
              currency: @currency
            })
            scope.set_tag('error_type', 'ledger_posting_failed')
            scope.set_tag('organization_id', @organization_id.to_s)
            scope.set_tag('environment', @environment)
          end
          Sentry.capture_exception(e)
        end
      rescue => sentry_error
        Rails.logger.warn "Sentry reporting failed: #{sentry_error.message}"
      end
    end
    
    raise PostingError, e.message
  end

  private

  def validate_inputs!
    raise PostingError, "Amount must be positive" unless @amount > 0
    raise PostingError, "Invalid environment" unless %w[sandbox production].include?(@environment)
    raise PostingError, "Currency must match" unless @currency.present?
  end

  def existing_result(transaction)
    return transaction if transaction.posted?

    # Idempotent retry: Ledger row exists but was never posted (e.g. crash after create).
    # Complete the double-entry and mark posted instead of raising.
    complete_posting_for_existing!(transaction)
  end

  def complete_posting_for_existing!(ledger_transaction)
    if ledger_transaction.ledger_entries.any?
      raise IdempotencyError, "Transaction already exists with partial state; manual intervention required"
    end

    source_account = resolve_ledger_account(@source_external_account_id, determine_source_account_type)
    destination_account = resolve_ledger_account(@destination_external_account_id, determine_destination_account_type)

    validate_account_types!(source_account, destination_account)
    source_entry_type, destination_entry_type = create_double_entry!(
      ledger_transaction, source_account, destination_account
    )
    update_balances!(source_account, destination_account, source_entry_type, destination_entry_type)
    ledger_transaction.mark_as_posted!

    ledger_transaction
  end

  def create_ledger_transaction!
    LedgerTransaction.create!(
      organization_id: @organization_id,
      environment: @environment,
      external_transaction_id: @external_transaction_id,
      status: 'pending',
      idempotency_key: @idempotency_key
    )
  end

  def determine_source_account_type
    if @is_deposit || @source_external_account_id.start_with?('SYSTEM_')
      'asset'
    else
      'liability'
    end
  end

  def determine_destination_account_type
    if @destination_external_account_id.start_with?('SYSTEM_')
      case @destination_external_account_id
      when 'SYSTEM_CASH_CONTROL', 'SYSTEM_BANK_CLEARING'
        'asset'
      when 'SYSTEM_FEE_INCOME'
        'income'
      else
        'asset'
      end
    else
      'liability'
    end
  end

  def resolve_ledger_account(external_account_id, account_type)
    Rails.logger.info(
      "resolve_ledger_account called " \
      "external_account_id=#{external_account_id.inspect} " \
      "account_type=#{account_type.inspect} " \
      "org=#{@organization_id.inspect} env=#{@environment.inspect}"
    )
    LedgerAccount.resolve(
      organization_id: @organization_id,
      environment: @environment,
      external_account_id: external_account_id,
      currency: @currency,
      account_type: account_type
    )
  end

  def validate_account_types!(source_account, destination_account)
    # Validate that we're not doing self-transfers (except for control accounts)
    if source_account.id == destination_account.id && !@is_deposit
      raise PostingError, "Self-transfers are not allowed"
    end

    # Deposits/withdrawals are routed via SYSTEM_CASH_CONTROL.
    # Transfers are between distinct accounts.
  end

  def create_double_entry!(transaction, source_account, destination_account)
    operation = detect_operation(source_account, destination_account)

    source_change = account_change_for(operation, :source)
    dest_change = account_change_for(operation, :destination)

    source_entry_type = entry_type_for(source_account.account_type, source_change)
    destination_entry_type = entry_type_for(destination_account.account_type, dest_change)

    # Create both entries (must balance: one debit, one credit, same amount).
    LedgerEntry.create!(
      organization_id: @organization_id,
      environment: @environment,
      ledger_account_id: source_account.id,
      transaction_id: transaction.id,
      entry_type: source_entry_type,
      amount: @amount,
      currency: @currency
    )

    LedgerEntry.create!(
      organization_id: @organization_id,
      environment: @environment,
      ledger_account_id: destination_account.id,
      transaction_id: transaction.id,
      entry_type: destination_entry_type,
      amount: @amount,
      currency: @currency
    )

    # Return which entry type was applied to each side for balance updates.
    [source_entry_type, destination_entry_type]
  end

  def update_balances!(source_account, destination_account, source_entry_type, destination_entry_type)
    # Signed balance convention is handled in AccountBalance (debit +, credit -).
    AccountBalance.update_balance!(
      organization_id: @organization_id,
      environment: @environment,
      ledger_account_id: source_account.id,
      amount_cents: @amount,
      currency: @currency,
      entry_type: source_entry_type
    )

    AccountBalance.update_balance!(
      organization_id: @organization_id,
      environment: @environment,
      ledger_account_id: destination_account.id,
      amount_cents: @amount,
      currency: @currency,
      entry_type: destination_entry_type
    )
  end

  def detect_operation(source_account, destination_account)
    source_is_cash = source_account.external_account_id == 'SYSTEM_CASH_CONTROL'
    dest_is_cash = destination_account.external_account_id == 'SYSTEM_CASH_CONTROL'

    # Accounts service uses:
    # - deposit:  SYSTEM_CASH_CONTROL -> user_account
    # - withdraw: user_account -> SYSTEM_CASH_CONTROL
    if source_is_cash && !dest_is_cash
      :deposit
    elsif dest_is_cash && !source_is_cash
      :withdraw
    else
      :transfer
    end
  end

  def account_change_for(operation, side)
    case operation
    when :transfer
      side == :source ? :decrease : :increase
    when :deposit
      # Bank deposit increases both cash (asset) and customer liability.
      :increase
    when :withdraw
      # Withdrawal decreases both cash (asset) and customer liability.
      :decrease
    else
      raise PostingError, "Unknown operation"
    end
  end

  def entry_type_for(account_type, change)
    normal_balance = case account_type
                     when 'asset', 'expense'
                       :debit
                     when 'liability', 'equity', 'income'
                       :credit
                     else
                       raise PostingError, "Invalid account type: #{account_type}"
                     end

    if change == :increase
      normal_balance == :debit ? 'debit' : 'credit'
    else
      normal_balance == :debit ? 'credit' : 'debit'
    end
  end

end
