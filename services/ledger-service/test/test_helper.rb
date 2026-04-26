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
    # Floor stays slightly below 100% so tiny drift (new branches) does not fail CI; raise tests—not the floor—when adding app code.
    minimum_coverage line: 99.42
  end
end

require_relative "../config/environment"
require "rails/test_help"
require_relative "support/test_stubbing"

class ActiveSupport::TestCase
  include TestStubbing

  # CI provides a single `ledger_test` DB; parallel workers expect `ledger_test-0`, etc.
  parallelize(workers: if ENV["COVERAGE"] == "true" || ENV["CI"] == "true"
                          1
                        else
                          :number_of_processors
                        end)
end
