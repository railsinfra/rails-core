use crate::config::Config;
use tonic::transport::Channel;

pub mod proto {
    tonic::include_proto!("rails.accounts.v1");
}

use proto::accounts_service_client::AccountsServiceClient;

#[derive(Clone)]
pub struct GrpcClients {
    // Populated when Accounts gRPC is reachable; optional for tests and degraded mode.
    #[allow(dead_code)]
    pub(crate) accounts_client: Option<AccountsServiceClient<Channel>>,
}

impl GrpcClients {
    /// No outbound gRPC (e.g. tests and local runs without Accounts).
    pub fn none() -> Self {
        Self {
            accounts_client: None,
        }
    }
}

pub async fn init(config: &Config) -> Result<GrpcClients, tonic::transport::Error> {
    match AccountsServiceClient::connect(config.accounts_grpc_url.clone()).await {
        Ok(client) => {
            tracing::info!("Connected to Accounts gRPC service at {}", config.accounts_grpc_url);
            Ok(GrpcClients {
                accounts_client: Some(client),
            })
        }
        Err(e) => {
            tracing::warn!(
                "Failed to connect to Accounts gRPC service at {}: {}",
                config.accounts_grpc_url,
                e
            );
            tracing::warn!("Account creation will fail - ensure Accounts service is running");
            Ok(GrpcClients {
                accounts_client: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{init, proto};
    use crate::config::Config;
    use proto::accounts_service_server::{AccountsService, AccountsServiceServer};
    use proto::{GetAccountBalanceRequest, GetAccountBalanceResponse};
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::Server;

    struct MockAccounts;

    #[tonic::async_trait]
    impl AccountsService for MockAccounts {
        async fn get_account_balance(
            &self,
            _request: tonic::Request<GetAccountBalanceRequest>,
        ) -> Result<tonic::Response<GetAccountBalanceResponse>, tonic::Status> {
            Ok(tonic::Response::new(GetAccountBalanceResponse {
                account_id: "acct".into(),
                balance: "0".into(),
                currency: "ZAR".into(),
            }))
        }
    }

    #[tokio::test]
    async fn init_treats_connection_refused_as_optional_accounts_client() {
        let cfg = Config::test_stub_with_accounts_grpc("http://127.0.0.1:1".into());
        let clients = init(&cfg).await.expect("init returns Ok even when connect fails");
        assert!(clients.accounts_client.is_none());
    }

    #[tokio::test]
    async fn init_connects_when_accounts_grpc_is_reachable() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let incoming = TcpListenerStream::new(listener);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server = Server::builder()
            .add_service(AccountsServiceServer::new(MockAccounts))
            .serve_with_incoming_shutdown(incoming, async {
                let _ = shutdown_rx.await;
            });
        let join = tokio::spawn(server);
        tokio::time::sleep(Duration::from_millis(80)).await;
        let cfg = Config::test_stub_with_accounts_grpc(format!("http://{}", addr));
        let clients = init(&cfg).await.expect("init");
        assert!(clients.accounts_client.is_some());
        let _ = shutdown_tx.send(());
        let _ = join.await;
    }
}
