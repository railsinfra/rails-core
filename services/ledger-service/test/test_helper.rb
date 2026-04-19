ENV["RAILS_ENV"] ||= "test"

if ENV["COVERAGE"] == "true"
  require "simplecov"
  require "simplecov_json_formatter"
  require "simplecov-lcov"

  SimpleCov::Formatter::LcovFormatter.config do |c|
    c.report_with_single_file = true
    c.single_report_path = File.expand_path("../coverage/lcov.info", __dir__)
  end

  SimpleCov.formatter = SimpleCov::Formatter::MultiFormatter.new(
    [
      SimpleCov::Formatter::JSONFormatter,
      SimpleCov::Formatter::LcovFormatter,
    ],
  )

  SimpleCov.start "rails" do
    add_filter "/test/"
    # Generated gRPC/protobuf stubs: exercised indirectly via LedgerService; excluding avoids skewing totals.
    add_filter "/lib/grpc/"
    add_filter "/app/channels/"
    add_filter "/app/jobs/"
    add_filter "/app/mailers/"
  end
end

require_relative "../config/environment"
require "rails/test_help"

class ActiveSupport::TestCase
  parallelize(workers: ENV["COVERAGE"] == "true" ? 1 : :number_of_processors)
end
