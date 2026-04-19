# frozen_string_literal: true

# Clear PostgreSQL prepared statements on startup to avoid type casting issues
# after schema migrations (e.g., when changing column types from UUID to string).
# This ensures fresh prepared statements are created with the current schema.
Rails.application.config.after_initialize do
  ActiveRecord::Base.connection_pool.with_connection do |conn|
    begin
      conn.execute("DEALLOCATE ALL")
      Rails.logger.info "Cleared all PostgreSQL prepared statements on startup"
    rescue => e
      Rails.logger.warn "Failed to clear prepared statements: #{e.message}"
    end
  end
end
