# frozen_string_literal: true

require "test_helper"

class ApplicationControllerHealthBranchesTest < ActionDispatch::IntegrationTest
  test "health marks grpc not_ready when connection refused" do
    with_stub(Socket, :tcp, proc { raise Errno::ECONNREFUSED }) do
      get "/health"
      assert_response :success
      body = JSON.parse(response.body)
      assert_equal "not_ready", body.dig("grpc", "status")
    end
  end

  test "health marks grpc error on unexpected socket errors" do
    with_stub(Socket, :tcp, proc { raise IOError, "weird" }) do
      get "/health"
      assert_response :success
      body = JSON.parse(response.body)
      assert_equal "error", body.dig("grpc", "status")
    end
  end

  test "health marks grpc ok when tcp succeeds" do
    with_stub(Socket, :tcp, proc { |*_args, **_kwargs| true }) do
      get "/health"
      assert_response :success
      body = JSON.parse(response.body)
      assert_equal "ok", body.dig("grpc", "status")
    end
  end

  test "health marks grpc down when tcp returns without raising but is falsy" do
    with_stub(Socket, :tcp, proc { |*_args, **_kwargs| false }) do
      get "/health"
      assert_response :success
      body = JSON.parse(response.body)
      assert_equal "down", body.dig("grpc", "status")
    end
  end
end
