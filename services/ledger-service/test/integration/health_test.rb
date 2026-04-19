require "test_helper"

class HealthTest < ActionDispatch::IntegrationTest
  test "health endpoint returns ok and service metadata" do
    get "/health"

    assert_response :success
    body = JSON.parse(response.body)
    assert_equal "ok", body["status"]
    assert_equal "ledger", body["service"]
    assert_equal "ok", body.dig("http", "status")
    assert body.dig("http", "port")
  end
end
