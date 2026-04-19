# frozen_string_literal: true

require "test_helper"

class ApplicationMailerTest < ActiveSupport::TestCase
  test "application mailer subclasses ActionMailer::Base" do
    assert_operator ApplicationMailer, :<, ActionMailer::Base
  end

  test "default from applies to messages" do
    mailer_class = Class.new(ApplicationMailer) do
      def hello
        mail(to: "user@example.com", subject: "Test", body: "Hello")
      end
    end

    message = mailer_class.hello
    assert_includes Array(message.from).join, "from@example.com"
  end
end
