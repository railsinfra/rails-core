# frozen_string_literal: true

# Load protobuf files early, before Zeitwerk tries to eager load app/grpc files
# These files define constants that app/grpc/ledger_service.rb depends on
# Load them directly here to ensure they're available before any eager loading
Dir[Rails.root.join('lib', 'grpc', '**', '*_pb.rb')].each { |f| require f }

# Configure structured logging
Rails.application.configure do
  config.lograge.enabled = true
  config.lograge.formatter = Lograge::Formatters::Json.new

  config.lograge.custom_options = lambda do |event|
    {
      organization_id: event.payload[:organization_id],
      environment: event.payload[:environment],
      correlation_id: event.payload[:correlation_id],
      service: 'ledger'
    }.compact
  end
end

# Initialize gRPC server in a background thread (only when running `rails server`)
# IMPORTANT: Do NOT start gRPC during rake tasks (e.g. db:migrate, db:prepare), as loading
# native extensions + background threads can crash the Ruby process, especially on macOS.
# Only start gRPC when explicitly running the Rails server command
#
# Detection method: Use Rails.const_defined?('Server') which is the industry-standard way
# to detect server mode. This works reliably because Rails defines Rails::Server constant
# when running via `rails server` command, but not during rake tasks, console, or runner.
is_server_mode = Rails.const_defined?('Server')
is_console_mode = Rails.const_defined?('Console')
is_rake_task = File.basename($0) == 'rake' || 
               ARGV.any? { |arg| arg.include?('rake') || arg.start_with?('db:') }

# Start gRPC only in server mode, not console or rake
# Keep START_GRPC_SERVER env var as emergency override (set to '0' to disable)
should_start_grpc = is_server_mode && !is_console_mode && !is_rake_task && ENV['START_GRPC_SERVER'] != '0'

# Log detection results for debugging
if Rails.env.development?
  Rails.logger.debug "gRPC initialization check: server_mode=#{is_server_mode}, console_mode=#{is_console_mode}, rake_task=#{is_rake_task}, should_start=#{should_start_grpc}"
end

if (Rails.env.development? || Rails.env.production?) && should_start_grpc
  Rails.application.config.after_initialize do
    require 'grpc'
    require 'google/protobuf'
    
    # Manually require the service file (it's excluded from autoloading)
    require Rails.root.join('app', 'grpc', 'ledger_service').to_s

    Thread.new do
      begin
        grpc_port = ENV.fetch('GRPC_PORT', 50053).to_i
        server = GRPC::RpcServer.new
        server.add_http2_port("0.0.0.0:#{grpc_port}", :this_port_is_insecure)
        server.handle(LedgerService.new)

        Rails.logger.info "Ledger gRPC server starting on 0.0.0.0:#{grpc_port}"
        Rails.logger.info "Ledger gRPC server thread started (Thread ID: #{Thread.current.object_id})"
        server.run_till_terminated
      rescue => e
        Rails.logger.error "Failed to start Ledger gRPC server: #{e.class}: #{e.message}"
        Rails.logger.error e.backtrace.join("\n")
      end
    end
  end
end

