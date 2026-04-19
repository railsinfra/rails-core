# frozen_string_literal: true

module Api
  module V1
    module Ledger
      class TransactionsController < ApplicationController
        before_action :authenticate_request
        before_action :set_organization_and_environment

        # GET /api/v1/ledger/transactions
        def index
          transactions = LedgerTransaction
            .where(organization_id: @organization_id, environment: @environment)
            .includes(:ledger_entries)
            .order(created_at: :desc)
            .limit(100)

          # Optional filtering by status
          if params[:status].present? && %w[pending posted failed].include?(params[:status])
            transactions = transactions.where(status: params[:status])
          end

          render json: {
            transactions: transactions.map { |tx|
              {
                id: tx.id,
                organization_id: tx.organization_id,
                environment: tx.environment,
                external_transaction_id: tx.external_transaction_id,
                status: tx.status,
                idempotency_key: tx.idempotency_key,
                failure_reason: tx.failure_reason,
                created_at: tx.created_at.iso8601,
                updated_at: tx.updated_at.iso8601,
                entries: tx.ledger_entries.map { |entry|
                  {
                    id: entry.id,
                    ledger_account_id: entry.ledger_account_id,
                    external_account_id: entry.ledger_account&.external_account_id,
                    entry_type: entry.entry_type,
                    amount: entry.amount,
                    currency: entry.currency
                  }
                }
              }
            }
          }
        end

        # GET /api/v1/ledger/transactions/:id
        def show
          transaction = LedgerTransaction
            .where(id: params[:id], organization_id: @organization_id, environment: @environment)
            .includes(:ledger_entries)
            .first

          unless transaction
            render json: { error: 'Transaction not found' }, status: :not_found
            return
          end

          render json: {
            id: transaction.id,
            organization_id: transaction.organization_id,
            environment: transaction.environment,
            external_transaction_id: transaction.external_transaction_id,
            status: transaction.status,
            idempotency_key: transaction.idempotency_key,
            failure_reason: transaction.failure_reason,
            created_at: transaction.created_at.iso8601,
            updated_at: transaction.updated_at.iso8601,
            entries: transaction.ledger_entries.map { |entry|
              {
                id: entry.id,
                ledger_account_id: entry.ledger_account_id,
                external_account_id: entry.ledger_account&.external_account_id,
                entry_type: entry.entry_type,
                amount: entry.amount,
                currency: entry.currency,
                created_at: entry.created_at.iso8601
              }
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
