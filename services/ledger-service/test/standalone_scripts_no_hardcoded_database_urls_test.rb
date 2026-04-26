# frozen_string_literal: true

# Static guard: no full Rails stack (avoids DB boot for this file alone).
require "minitest/autorun"

class StandaloneScriptsNoHardcodedDatabaseUrlsTest < Minitest::Test
  def test_test_hardened_ledger_rb_does_not_embed_postgres_credentials_in_a_connection_url
    root = File.expand_path("..", __dir__)
    path = File.join(root, "test_hardened_ledger.rb")
    skip "test_hardened_ledger.rb is not present" unless File.file?(path)

    body = File.read(path)
    non_comment = body.lines.reject { |line| line.lstrip.start_with?("#") }.join

    refute_match(
      %r{postgresql://[A-Za-z0-9_.-]+:[^"'\s/@]+@}i,
      non_comment,
      "Do not hardcode DATABASE_URL with a password; use ENV[\"DATABASE_URL\"] or the localhost default from config/database.yml."
    )
  end
end
