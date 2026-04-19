# frozen_string_literal: true

require "test_helper"

class ApplicationJobTest < ActiveSupport::TestCase
  test "application job inherits ActiveJob::Base" do
    assert_operator ApplicationJob, :<, ActiveJob::Base
  end
end
