# frozen_string_literal: true

# Log database connection pool configuration
# Pool size is configured in config/database.yml
Rails.application.config.after_initialize do
  pool = ActiveRecord::Base.connection_pool
  Rails.logger.info "Database connection pool: size=#{pool.size}, checkout_timeout=#{pool.checkout_timeout}s"
end
