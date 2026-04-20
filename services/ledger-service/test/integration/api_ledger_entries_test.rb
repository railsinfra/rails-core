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
    deposit_count = 3
    per_page = 2
    # Each successful deposit creates two ledger rows (debit + credit).
    entries_per_deposit = 2
    total_entries = deposit_count * entries_per_deposit
    expected_total_pages = (total_entries + per_page - 1) / per_page

    deposit_count.times do |i|
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
        params: { page: 1, per_page: per_page },
        headers: {
          "Authorization" => "Bearer #{@token}",
          "X-Environment" => "sandbox"
        }
    assert_response :success
    body = JSON.parse(response.body)

    assert_equal total_entries, body["pagination"]["total_count"],
                 "expected #{total_entries} ledger rows for #{deposit_count} deposits"
    assert_equal expected_total_pages, body["pagination"]["total_pages"],
                 "expected ceil(#{total_entries}/#{per_page}) = #{expected_total_pages} pages"
    assert_equal per_page, body["data"].length,
                 "first page should be capped at per_page, not return all rows"
    assert_equal 1, body["pagination"]["page"]
  end

  test "entries index rejects invalid environment" do
    get "/api/v1/ledger/entries",
        headers: {
          "Authorization" => "Bearer #{@token}",
          "X-Environment" => "staging"
        }
    assert_response :bad_request
  end

  test "entries index rejects unexpected jwt errors" do
    with_stub(JWT, :decode, proc { raise RuntimeError, "unexpected" }) do
      get "/api/v1/ledger/entries",
          headers: { "Authorization" => "Bearer #{@token}", "X-Environment" => "sandbox" }
      assert_response :unauthorized
      assert_equal "Authentication failed", JSON.parse(response.body)["error"]
    end
  end

  test "entries index rejects expired jwt" do
    token = JWT.encode(
      { "business_id" => @organization_id, "exp" => Time.now.to_i - 120 },
      ENV.fetch("JWT_SECRET", "dev_secret"),
      "HS256"
    )
    get "/api/v1/ledger/entries",
        headers: { "Authorization" => "Bearer #{token}", "X-Environment" => "sandbox" }
    assert_response :unauthorized
    assert_match(/expired/i, JSON.parse(response.body)["error"])
  end

  test "entries index rejects malformed jwt with decode error" do
    get "/api/v1/ledger/entries",
        headers: { "Authorization" => "Bearer not-a-jwt", "X-Environment" => "sandbox" }
    assert_response :unauthorized
    assert_match(/Invalid token/i, JSON.parse(response.body)["error"])
  end
end
