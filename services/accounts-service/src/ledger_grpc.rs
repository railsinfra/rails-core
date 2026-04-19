use std::time::Duration;

use crate::errors::AppError;
use crate::grpc::ledger_proto::{
    ledger_service_client::LedgerServiceClient, Environment, GetAccountBalanceRequest,
    PostTransactionRequest,
};
use tonic::transport::Endpoint;

#[derive(Clone)]
pub struct LedgerGrpc {
    endpoint: String,
    timeout: Duration,
}

impl LedgerGrpc {
    pub fn new(endpoint: String) -> Self {
        let timeout_secs = std::env::var("LEDGER_GRPC_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(10);

        Self {
            endpoint,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    fn env_to_proto(environment: &str) -> Result<i32, AppError> {
        match environment {
            "sandbox" => Ok(Environment::Sandbox as i32),
            "production" => Ok(Environment::Production as i32),
            other => Err(AppError::Validation(format!(
                "invalid environment for ledger gRPC: {}",
                other
            ))),
        }
    }

    pub async fn post_transaction(
        &self,
        organization_id: uuid::Uuid,
        environment: &str,
        source_external_account_id: String,
        destination_external_account_id: String,
        amount: i64,
        currency: String,
        external_transaction_id: uuid::Uuid,
        idempotency_key: String,
        correlation_id: String,
    ) -> Result<(), AppError> {
        let env = Self::env_to_proto(environment)?;

        let channel = Endpoint::from_shared(self.endpoint.clone())
            .map_err(|e| AppError::Internal(format!("invalid LEDGER_GRPC_URL: {}", e)))?
            .connect_timeout(self.timeout)
            .timeout(self.timeout)
            .connect()
            .await
            .map_err(|e| AppError::Internal(format!("ledger gRPC connect failed: {}", e)))?;

        let mut client = LedgerServiceClient::new(channel);

        let req = PostTransactionRequest {
            organization_id: organization_id.to_string(),
            environment: env,
            source_external_account_id,
            destination_external_account_id,
            amount,
            currency,
            external_transaction_id: external_transaction_id.to_string(),
            idempotency_key,
            correlation_id,
        };

        let resp = tokio::time::timeout(
            self.timeout,
            client.post_transaction(tonic::Request::new(req))
        )
        .await
        .map_err(|_| AppError::Internal("ledger gRPC post timeout expired".to_string()))?
        .map_err(|e| AppError::Internal(format!("ledger gRPC post failed: {}", e)))?
        .into_inner();

        if resp.status == "posted" {
            Ok(())
        } else {
            Err(AppError::BusinessLogic(format!(
                "ledger post failed: {}",
                resp.failure_reason
            )))
        }
    }

    pub async fn get_account_balance(
        &self,
        organization_id: uuid::Uuid,
        environment: &str,
        external_account_id: uuid::Uuid,
        currency: &str,
    ) -> Result<String, AppError> {
        let env = Self::env_to_proto(environment)?;

        let channel = Endpoint::from_shared(self.endpoint.clone())
            .map_err(|e| AppError::Internal(format!("invalid LEDGER_GRPC_URL: {}", e)))?
            .connect_timeout(self.timeout)
            .timeout(self.timeout)
            .connect()
            .await
            .map_err(|e| AppError::Internal(format!("ledger gRPC connect failed: {}", e)))?;

        let mut client = LedgerServiceClient::new(channel);

        let req = GetAccountBalanceRequest {
            organization_id: organization_id.to_string(),
            environment: env,
            external_account_id: external_account_id.to_string(),
            currency: currency.to_string(),
        };

        let resp = tokio::time::timeout(
            self.timeout,
            client.get_account_balance(tonic::Request::new(req))
        )
        .await
        .map_err(|_| AppError::Internal("ledger gRPC get_account_balance timeout expired".to_string()))?
        .map_err(|e| AppError::Internal(format!("ledger gRPC get_account_balance failed: {}", e)))?
        .into_inner();

        Ok(resp.balance)
    }
}

