# frozen_string_literal: true

require "test_helper"
require "jwt"

class ApiLedgerEntriesTest < ActionDispatch::IntegrationTest
  setup do
    @organization_id = SecureRandom.uuid
    @token = JWT.encode(
      { "business_id" => @organization_id, "exp" => Time.now.to_i + 3600 },
      ENV.fetch("JWT_SECRET", "dev_secret"),
      "HS256"
    )
  end

  test "entries index requires authorization" do
    get "/api/v1/ledger/entries", headers: { "X-Environment" => "sandbox" }
    assert_response :unauthorized
  end

  test "entries index returns pagination envelope" do
    LedgerPoster.post_deposit(
      organization_id: @organization_id,
      environment: "sandbox",
      destination_external_account_id: "api_entries",
      amount: 100,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid
    )

    get "/api/v1/ledger/entries",
        headers: {
          "Authorization" => "Bearer #{@token}",
          "X-Environment" => "sandbox"
        }
    assert_response :success
    body = JSON.parse(response.body)
    assert body["data"].is_a?(Array)
    assert body["pagination"]["page"].present?
    assert body["pagination"]["total_count"] >= 1
  end

  test "entries index respects page and per_page" do
    3.times do |i|
      LedgerPoster.post_deposit(
        organization_id: @organization_id,
        environment: "sandbox",
        destination_external_account_id: "api_entries_page_#{i}",
        amount: 50,
        currency: "USD",
        external_transaction_id: SecureRandom.uuid,
        idempotency_key: SecureRandom.uuid
      )
    end

    get "/api/v1/ledger/entries",
        params: { page: 1, per_page: 2 },
        headers: {
          "Authorization" => "Bearer #{@token}",
          "X-Environment" => "sandbox"
        }
    assert_response :success
    body = JSON.parse(response.body)
    assert_operator body["data"].length, :<=, 2
    assert_operator body["pagination"]["total_pages"], :>=, 1
  end
end
