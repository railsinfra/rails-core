ENV["RAILS_ENV"] ||= "test"

if ENV["COVERAGE"] == "true"
  require "simplecov"
  require "simplecov_json_formatter"
  SimpleCov.formatter = SimpleCov::Formatter::JSONFormatter
  SimpleCov.start "rails" do
    add_filter "/test/"
  end
end

require_relative "../config/environment"
require "rails/test_help"

class ActiveSupport::TestCase
  parallelize(workers: ENV["COVERAGE"] == "true" ? 1 : :number_of_processors)
end
