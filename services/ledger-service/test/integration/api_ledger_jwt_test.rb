# frozen_string_literal: true

require "test_helper"
require "jwt"

class ApiLedgerJwtTest < ActionDispatch::IntegrationTest
  setup do
    @org = SecureRandom.uuid
    @secret = ENV.fetch("JWT_SECRET", "dev_secret")
  end

  test "transactions index rejects token missing business_id" do
    token = JWT.encode({ "sub" => "nope", "exp" => Time.now.to_i + 3600 }, @secret, "HS256")
    get "/api/v1/ledger/transactions",
        headers: { "Authorization" => "Bearer #{token}", "X-Environment" => "sandbox" }
    assert_response :unauthorized
    body = JSON.parse(response.body)
    assert_match(/missing business_id/i, body["error"])
  end

  test "transactions index accepts businessId camelCase" do
    token = JWT.encode({ "businessId" => @org, "exp" => Time.now.to_i + 3600 }, @secret, "HS256")
    get "/api/v1/ledger/transactions",
        headers: { "Authorization" => "Bearer #{token}", "X-Environment" => "sandbox" }
    assert_response :success
  end

  test "transactions index rejects expired jwt" do
    token = JWT.encode({ "business_id" => @org, "exp" => Time.now.to_i - 120 }, @secret, "HS256")
    get "/api/v1/ledger/transactions",
        headers: { "Authorization" => "Bearer #{token}", "X-Environment" => "sandbox" }
    assert_response :unauthorized
    assert_match(/expired/i, JSON.parse(response.body)["error"])
  end

  test "transactions index rejects malformed jwt" do
    get "/api/v1/ledger/transactions",
        headers: { "Authorization" => "Bearer not-a-jwt", "X-Environment" => "sandbox" }
    assert_response :unauthorized
  end

  test "transactions index rejects unexpected jwt errors" do
    with_stub(JWT, :decode, proc { raise RuntimeError, "unexpected" }) do
      token = JWT.encode({ "business_id" => @org, "exp" => Time.now.to_i + 3600 }, @secret, "HS256")
      get "/api/v1/ledger/transactions",
          headers: { "Authorization" => "Bearer #{token}", "X-Environment" => "sandbox" }
      assert_response :unauthorized
      assert_equal "Authentication failed", JSON.parse(response.body)["error"]
    end
  end

  test "transactions index ignores invalid status param" do
    token = JWT.encode({ "business_id" => @org, "exp" => Time.now.to_i + 3600 }, @secret, "HS256")
    LedgerPoster.post_deposit(
      organization_id: @org,
      environment: "sandbox",
      destination_external_account_id: "status_mix",
      amount: 10,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid
    )
    pending = LedgerTransaction.create!(
      organization_id: @org,
      environment: "sandbox",
      external_transaction_id: SecureRandom.uuid,
      status: "pending",
      idempotency_key: SecureRandom.uuid
    )

    get "/api/v1/ledger/transactions",
        params: { status: "not_a_real_status" },
        headers: { "Authorization" => "Bearer #{token}", "X-Environment" => "sandbox" }
    assert_response :success
    ids = JSON.parse(response.body)["transactions"].map { |t| t["id"] }
    assert_includes ids, pending.id.to_s
  end

  test "entries index filters by account_id when account exists" do
    token = JWT.encode({ "business_id" => @org, "exp" => Time.now.to_i + 3600 }, @secret, "HS256")
    ext = "filter_acct_#{SecureRandom.hex(4)}"
    LedgerPoster.post_deposit(
      organization_id: @org,
      environment: "sandbox",
      destination_external_account_id: ext,
      amount: 15,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid
    )

    get "/api/v1/ledger/entries",
        params: { account_id: ext },
        headers: { "Authorization" => "Bearer #{token}", "X-Environment" => "sandbox" }
    assert_response :success
    data = JSON.parse(response.body)["data"]
    assert data.any?
    assert(data.all? { |row| row["external_account_id"] == ext })
  end
end
