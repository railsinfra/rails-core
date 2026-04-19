# frozen_string_literal: true

require "test_helper"
require "jwt"

class ApiLedgerTransactionsTest < ActionDispatch::IntegrationTest
  setup do
    @organization_id = SecureRandom.uuid
    @token = JWT.encode(
      { "business_id" => @organization_id, "exp" => Time.now.to_i + 3600 },
      ENV.fetch("JWT_SECRET", "dev_secret"),
      "HS256"
    )
  end

  test "transactions index requires authorization" do
    get "/api/v1/ledger/transactions", headers: { "X-Environment" => "sandbox" }
    assert_response :unauthorized
  end

  test "transactions index rejects invalid environment" do
    get "/api/v1/ledger/transactions",
        headers: {
          "Authorization" => "Bearer #{@token}",
          "X-Environment" => "staging"
        }
    assert_response :bad_request
  end

  test "transactions index returns posted transaction" do
    LedgerPoster.post_deposit(
      organization_id: @organization_id,
      environment: "sandbox",
      destination_external_account_id: "api_tx_dest",
      amount: 500,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid
    )

    get "/api/v1/ledger/transactions",
        headers: {
          "Authorization" => "Bearer #{@token}",
          "X-Environment" => "sandbox"
        }
    assert_response :success
    body = JSON.parse(response.body)
    assert body["transactions"].is_a?(Array)
    assert(body["transactions"].any? { |t| t["status"] == "posted" })
  end

  test "transactions index filters by status when valid" do
    LedgerPoster.post_deposit(
      organization_id: @organization_id,
      environment: "sandbox",
      destination_external_account_id: "api_tx_filter",
      amount: 100,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid
    )

    get "/api/v1/ledger/transactions",
        params: { status: "posted" },
        headers: {
          "Authorization" => "Bearer #{@token}",
          "X-Environment" => "sandbox"
        }
    assert_response :success
    body = JSON.parse(response.body)
    assert(body["transactions"].all? { |t| t["status"] == "posted" })
  end

  test "transactions show returns 404 for unknown id" do
    get "/api/v1/ledger/transactions/#{SecureRandom.uuid}",
        headers: {
          "Authorization" => "Bearer #{@token}",
          "X-Environment" => "sandbox"
        }
    assert_response :not_found
  end

  test "transactions show returns transaction json" do
    tx = LedgerPoster.post_deposit(
      organization_id: @organization_id,
      environment: "sandbox",
      destination_external_account_id: "api_tx_show",
      amount: 300,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid
    )

    get "/api/v1/ledger/transactions/#{tx.id}",
        headers: {
          "Authorization" => "Bearer #{@token}",
          "X-Environment" => "sandbox"
        }
    assert_response :success
    body = JSON.parse(response.body)
    assert_equal tx.id.to_s, body["id"]
    assert body["entries"].is_a?(Array)
  end
end
