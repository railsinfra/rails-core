# frozen_string_literal: true

# Sentry error tracking and performance monitoring
# Configure via SENTRY_DSN environment variable
if ENV['SENTRY_DSN'].present?
  Sentry.init do |config|
    config.dsn = ENV['SENTRY_DSN']
    config.breadcrumbs_logger = [:active_support_logger, :http_logger]
    
    # Set environment (development, staging, production)
    config.environment = ENV.fetch('ENVIRONMENT', Rails.env)
    
    # Performance monitoring
    config.traces_sample_rate = 0.1 # Sample 10% of transactions for MVP
    
    # Filter sensitive data
    config.before_send = lambda do |event, hint|
      # Remove sensitive headers
      if event.request && event.request.headers
        event.request.headers.delete('Authorization')
        event.request.headers.delete('X-Internal-Service-Token')
      end
      event
    end
    
    # Set release version (useful for tracking deployments)
    config.release = ENV.fetch('RAILS_RELEASE', nil)
    
    # Tag all events with service name
    config.tags = {
      service: 'ledger',
      environment: config.environment
    }
  end
  
  Rails.logger.info "Sentry initialized for environment: #{Sentry.configuration.environment}"
else
  Rails.logger.info "Sentry DSN not configured, skipping error tracking"
end
