class ApplicationController < ActionController::API
  def health
    # Health check should return 200 OK as long as Rails is running
    # gRPC status is informational only - don't fail health check if gRPC isn't ready yet
    grpc_port = ENV.fetch('GRPC_PORT', 50053).to_i
    grpc_status = 'unknown'
    
    begin
      # Try to connect to gRPC server (non-blocking, short timeout)
      require 'socket'
      grpc_ok = Socket.tcp('127.0.0.1', grpc_port, connect_timeout: 0.1) { true }
      grpc_status = grpc_ok ? 'ok' : 'down'
    rescue SocketError, Errno::ECONNREFUSED, Errno::ETIMEDOUT, Errno::EHOSTUNREACH
      # gRPC server not ready yet or not running - this is OK for health check
      grpc_status = 'not_ready'
    rescue => e
      # Any other error - log but don't fail health check
      Rails.logger.debug "Health check gRPC probe failed: #{e.class}: #{e.message}"
      grpc_status = 'error'
    end

    render json: {
      status: 'ok',
      service: 'ledger',
      http: {
        port: ENV.fetch('PORT', 3000).to_i,
        status: 'ok'
      },
      grpc: {
        port: grpc_port,
        status: grpc_status
      },
      timestamp: Time.current.iso8601
    }
  end
end
