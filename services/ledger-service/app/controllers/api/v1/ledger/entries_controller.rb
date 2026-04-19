# frozen_string_literal: true

module Api
  module V1
    module Ledger
      class EntriesController < ApplicationController
        before_action :authenticate_request
        before_action :set_organization_and_environment

        # GET /api/v1/ledger/entries
        def index
          # Parse and validate pagination params with defaults
          page = [params[:page].to_i, 1].max
          per_page = [[params[:per_page].to_i, 100].min, 1].max
          offset = (page - 1) * per_page

          # Build base query
          base_query = LedgerEntry
            .where(organization_id: @organization_id, environment: @environment)
            .includes(:ledger_account, :ledger_transaction)

          # Optional filtering by account_id
          if params[:account_id].present?
            ledger_account = LedgerAccount.find_by(
              organization_id: @organization_id,
              environment: @environment,
              external_account_id: params[:account_id]
            )
            base_query = base_query.where(ledger_account_id: ledger_account.id) if ledger_account
          end

          # Get total count
          total_count = base_query.count

          # Calculate total pages
          total_pages = (total_count.to_f / per_page).ceil

          # Fetch paginated results with deterministic ordering
          entries = base_query
            .order(created_at: :desc, id: :desc)
            .limit(per_page)
            .offset(offset)

          render json: {
            data: entries.map { |entry|
              {
                id: entry.id,
                ledger_account_id: entry.ledger_account_id,
                external_account_id: entry.ledger_account&.external_account_id,
                transaction_id: entry.transaction_id,
                external_transaction_id: entry.ledger_transaction&.external_transaction_id,
                entry_type: entry.entry_type,
                amount: entry.amount,
                currency: entry.currency,
                created_at: entry.created_at.iso8601
              }
            },
            pagination: {
              page: page,
              per_page: per_page,
              total_count: total_count,
              total_pages: total_pages
            }
          }
        end

        private

        def authenticate_request
          auth_header = request.headers['Authorization']
          return render json: { error: 'Unauthorized' }, status: :unauthorized unless auth_header

          token = auth_header.split(' ').last
          return render json: { error: 'Unauthorized' }, status: :unauthorized unless token

          # Decode JWT to get business_id (which we use as organization_id)
          begin
            require 'jwt'
            secret = ENV.fetch('JWT_SECRET', 'dev_secret')
            decoded = JWT.decode(token, secret, true, algorithm: 'HS256')
            payload = decoded.first
            @organization_id = payload['business_id'] || payload['businessId']
            
            unless @organization_id
              render json: { error: 'Invalid token: missing business_id' }, status: :unauthorized
              return
            end
            
            # Convert to UUID if it's a string
            @organization_id = @organization_id.to_s
          rescue JWT::DecodeError => e
            Rails.logger.error("JWT decode error: #{e.class}: #{e.message}")
            render json: { error: "Invalid token: #{e.message}" }, status: :unauthorized
            return
          rescue JWT::ExpiredSignature => e
            Rails.logger.error("JWT expired: #{e.message}")
            render json: { error: 'Token has expired' }, status: :unauthorized
            return
          rescue => e
            Rails.logger.error("Unexpected authentication error: #{e.class}: #{e.message}")
            render json: { error: 'Authentication failed' }, status: :unauthorized
            return
          end
        end

        def set_organization_and_environment
          # Get environment from JWT or header
          auth_header = request.headers['Authorization']
          token = auth_header&.split(' ')&.last if auth_header
          
          if token
            begin
              require 'jwt'
              secret = ENV.fetch('JWT_SECRET', 'dev_secret')
              decoded = JWT.decode(token, secret, true, algorithm: 'HS256')
              payload = decoded.first
              
              # Try to get environment from JWT env field (which is environment_id)
              # For now, we'll use a header to specify sandbox vs production
              # In the future, we could look up the environment type from the users service
              @environment = request.headers['X-Environment']&.downcase || 'sandbox'
            rescue
              @environment = request.headers['X-Environment']&.downcase || 'sandbox'
            end
          else
            @environment = request.headers['X-Environment']&.downcase || 'sandbox'
          end
          
          # Validate environment
          unless %w[sandbox production].include?(@environment)
            render json: { error: 'Invalid environment. Must be sandbox or production' }, status: :bad_request
            return
          end
        end
      end
    end
  end
end
