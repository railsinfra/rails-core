use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tonic::transport::{Channel, Endpoint};

use crate::errors::AppError;
use crate::grpc::ledger_proto::{
    ledger_service_client::LedgerServiceClient, Environment, GetAccountBalanceRequest,
    GetAccountBalancesRequest, PostTransactionRequest,
};

#[derive(Clone)]
pub struct LedgerGrpc {
    endpoint: String,
    timeout: Duration,
    channel: Arc<Mutex<Option<Channel>>>,
}

impl LedgerGrpc {
    pub fn new(endpoint: String) -> Self {
        let timeout_secs = std::env::var("LEDGER_GRPC_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(60);

        Self {
            endpoint,
            timeout: Duration::from_secs(timeout_secs),
            channel: Arc::new(Mutex::new(None)),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    async fn connect_channel(&self) -> Result<Channel, AppError> {
        let mut slot = self.channel.lock().await;
        if let Some(ch) = slot.as_ref() {
            return Ok(ch.clone());
        }
        let ch = Endpoint::from_shared(self.endpoint.clone())
            .map_err(|e| AppError::Internal(format!("invalid LEDGER_GRPC_URL: {}", e)))?
            .connect_timeout(self.timeout)
            .timeout(self.timeout)
            .connect()
            .await
            .map_err(|e| AppError::Internal(format!("ledger gRPC connect failed: {}", e)))?;
        *slot = Some(ch.clone());
        Ok(ch)
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

        let channel = self.connect_channel().await?;

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
            client.post_transaction(tonic::Request::new(req)),
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

        let channel = self.connect_channel().await?;

        let mut client = LedgerServiceClient::new(channel);

        let req = GetAccountBalanceRequest {
            organization_id: organization_id.to_string(),
            environment: env,
            external_account_id: external_account_id.to_string(),
            currency: currency.to_string(),
        };

        let resp = tokio::time::timeout(
            self.timeout,
            client.get_account_balance(tonic::Request::new(req)),
        )
        .await
        .map_err(|_| {
            AppError::Internal("ledger gRPC get_account_balance timeout expired".to_string())
        })?
        .map_err(|e| AppError::Internal(format!("ledger gRPC get_account_balance failed: {}", e)))?
        .into_inner();

        Ok(resp.balance)
    }

    pub async fn get_account_balances(
        &self,
        organization_id: uuid::Uuid,
        environment: &str,
        from_external_account_id: uuid::Uuid,
        to_external_account_id: uuid::Uuid,
        currency: &str,
    ) -> Result<(String, String), AppError> {
        let env = Self::env_to_proto(environment)?;
        let channel = self.connect_channel().await?;
        let mut client = LedgerServiceClient::new(channel);

        let req = GetAccountBalancesRequest {
            organization_id: organization_id.to_string(),
            environment: env,
            from_external_account_id: from_external_account_id.to_string(),
            to_external_account_id: to_external_account_id.to_string(),
            currency: currency.to_string(),
        };

        let resp = tokio::time::timeout(
            self.timeout,
            client.get_account_balances(tonic::Request::new(req)),
        )
        .await
        .map_err(|_| {
            AppError::Internal("ledger gRPC get_account_balances timeout expired".to_string())
        })?
        .map_err(|e| AppError::Internal(format!("ledger gRPC get_account_balances failed: {}", e)))?
        .into_inner();

        Ok((resp.from_balance, resp.to_balance))
    }
}

#[cfg(test)]
mod ledger_grpc_tests {
    use super::*;
    use crate::grpc::ledger_proto::ledger_service_server::{LedgerService, LedgerServiceServer};
    use crate::grpc::ledger_proto::{
        GetAccountBalanceRequest, GetAccountBalanceResponse, GetAccountBalancesRequest,
        GetAccountBalancesResponse, PostTransactionRequest, PostTransactionResponse,
    };
    use std::sync::Mutex;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::{transport::Server, Request, Response, Status};

    static LEDGER_TIMEOUT_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Clone, Default)]
    struct MockOk;

    #[tonic::async_trait]
    impl LedgerService for MockOk {
        async fn post_transaction(
            &self,
            _req: Request<PostTransactionRequest>,
        ) -> Result<Response<PostTransactionResponse>, Status> {
            Ok(Response::new(PostTransactionResponse {
                status: "posted".into(),
                ledger_transaction_id: String::default(),
                failure_reason: String::default(),
            }))
        }

        async fn get_account_balance(
            &self,
            _req: Request<GetAccountBalanceRequest>,
        ) -> Result<Response<GetAccountBalanceResponse>, Status> {
            Ok(Response::new(GetAccountBalanceResponse {
                balance: "-100".into(),
                currency: "USD".into(),
            }))
        }

        async fn get_account_balances(
            &self,
            _req: Request<GetAccountBalancesRequest>,
        ) -> Result<Response<GetAccountBalancesResponse>, Status> {
            Ok(Response::new(GetAccountBalancesResponse {
                from_balance: "-100".into(),
                to_balance: "-250".into(),
                currency: "USD".into(),
            }))
        }
    }

    #[derive(Clone, Default)]
    struct MockBizFail;

    #[tonic::async_trait]
    impl LedgerService for MockBizFail {
        async fn post_transaction(
            &self,
            _req: Request<PostTransactionRequest>,
        ) -> Result<Response<PostTransactionResponse>, Status> {
            Ok(Response::new(PostTransactionResponse {
                status: "rejected".into(),
                ledger_transaction_id: String::default(),
                failure_reason: "insufficient funds".into(),
            }))
        }

        async fn get_account_balance(
            &self,
            _req: Request<GetAccountBalanceRequest>,
        ) -> Result<Response<GetAccountBalanceResponse>, Status> {
            Ok(Response::new(GetAccountBalanceResponse {
                balance: "0".into(),
                currency: "USD".into(),
            }))
        }

        async fn get_account_balances(
            &self,
            _req: Request<GetAccountBalancesRequest>,
        ) -> Result<Response<GetAccountBalancesResponse>, Status> {
            Ok(Response::new(GetAccountBalancesResponse {
                from_balance: "0".into(),
                to_balance: "0".into(),
                currency: "USD".into(),
            }))
        }
    }

    async fn ledger_base_url(mock: impl LedgerService + Send + Sync + 'static + Clone) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let incoming = TcpListenerStream::new(listener);
        tokio::spawn(async move {
            Server::builder()
                .add_service(LedgerServiceServer::new(mock))
                .serve_with_incoming(incoming)
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        format!("http://{}", addr)
    }

    fn sample_post_args() -> (
        uuid::Uuid,
        String,
        String,
        String,
        i64,
        String,
        uuid::Uuid,
        String,
        String,
    ) {
        let org = uuid::Uuid::new_v4();
        let ext = uuid::Uuid::new_v4();
        (
            org,
            "sandbox".to_string(),
            ext.to_string(),
            ext.to_string(),
            1,
            "USD".to_string(),
            ext,
            "idem".to_string(),
            "corr".to_string(),
        )
    }

    #[tokio::test]
    async fn post_and_balance_reuse_cached_channel() {
        let url = ledger_base_url(MockOk::default()).await;
        let client = LedgerGrpc::new(url);
        let (org, env, src, dst, amt, cur, ext, idem, corr) = sample_post_args();
        client
            .post_transaction(
                org,
                &env,
                src.clone(),
                dst.clone(),
                amt,
                cur.clone(),
                ext,
                idem.clone(),
                corr.clone(),
            )
            .await
            .unwrap();
        client
            .post_transaction(org, &env, src, dst, amt, cur, ext, idem, corr)
            .await
            .unwrap();
        let bal = client
            .get_account_balance(org, &env, ext, "USD")
            .await
            .unwrap();
        assert_eq!(bal, "-100");
        let (from_bal, to_bal) = client
            .get_account_balances(org, &env, ext, ext, "USD")
            .await
            .unwrap();
        assert_eq!(from_bal, "-100");
        assert_eq!(to_bal, "-250");
        let _ = client.endpoint();
        assert_eq!(client.timeout().as_secs(), 60);
    }

    #[tokio::test]
    async fn post_non_posted_status_returns_business_error() {
        let url = ledger_base_url(MockBizFail::default()).await;
        let client = LedgerGrpc::new(url);
        let (org, env, src, dst, amt, cur, ext, idem, corr) = sample_post_args();
        let err = client
            .post_transaction(org, &env, src, dst, amt, cur, ext, idem, corr)
            .await
            .unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("ledger post failed"), "{}", msg);
    }

    #[tokio::test]
    async fn invalid_environment_is_validation_error() {
        let url = ledger_base_url(MockOk::default()).await;
        let client = LedgerGrpc::new(url);
        let (org, _env, src, dst, amt, cur, ext, idem, corr) = sample_post_args();
        let err = client
            .post_transaction(org, "invalid-env", src, dst, amt, cur, ext, idem, corr)
            .await
            .unwrap_err();
        assert!(format!("{}", err).contains("invalid environment"));
    }

    #[tokio::test]
    async fn connect_refused_surfaces_internal_error() {
        let client = LedgerGrpc::new("http://127.0.0.1:9".to_string());
        let (org, env, src, dst, amt, cur, ext, idem, corr) = sample_post_args();
        let err = client
            .post_transaction(org, &env, src, dst, amt, cur, ext, idem, corr)
            .await
            .unwrap_err();
        let s = format!("{}", err);
        assert!(
            s.contains("ledger gRPC connect failed") || s.contains("ledger gRPC post failed"),
            "{}",
            s
        );
    }

    #[test]
    fn invalid_ledger_url_from_shared_errors_on_connect() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let client = LedgerGrpc::new("not a valid uri \0 scheme".to_string());
            let (org, env, src, dst, amt, cur, ext, idem, corr) = sample_post_args();
            let err = client
                .post_transaction(org, &env, src, dst, amt, cur, ext, idem, corr)
                .await
                .unwrap_err();
            assert!(format!("{}", err).contains("invalid LEDGER_GRPC_URL"));
        });
    }

    #[tokio::test]
    async fn custom_timeout_from_env() {
        let _g = LEDGER_TIMEOUT_ENV_LOCK.lock().unwrap();
        std::env::set_var("LEDGER_GRPC_TIMEOUT_SECS", "42");
        let client = LedgerGrpc::new("http://127.0.0.1:9".to_string());
        assert_eq!(client.timeout().as_secs(), 42);
        std::env::remove_var("LEDGER_GRPC_TIMEOUT_SECS");
    }

    #[tokio::test]
    async fn invalid_timeout_env_uses_default_sixty() {
        let _g = LEDGER_TIMEOUT_ENV_LOCK.lock().unwrap();
        std::env::set_var("LEDGER_GRPC_TIMEOUT_SECS", "0");
        let client = LedgerGrpc::new("http://127.0.0.1:9".to_string());
        assert_eq!(client.timeout().as_secs(), 60);
        std::env::set_var("LEDGER_GRPC_TIMEOUT_SECS", "not-a-number");
        let client2 = LedgerGrpc::new("http://127.0.0.1:9".to_string());
        assert_eq!(client2.timeout().as_secs(), 60);
        std::env::remove_var("LEDGER_GRPC_TIMEOUT_SECS");
    }
}
