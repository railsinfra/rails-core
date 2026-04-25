use accounts_api::grpc::ledger_proto::ledger_service_server::{LedgerService, LedgerServiceServer};
use accounts_api::grpc::ledger_proto::{
    GetAccountBalanceRequest, GetAccountBalanceResponse, GetAccountBalancesRequest,
    GetAccountBalancesResponse, PostTransactionRequest, PostTransactionResponse,
};
use std::sync::{Arc, Mutex};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{transport::Server, Request, Response, Status};

#[derive(Clone)]
pub struct MockLedger {
    capture: Option<Arc<Mutex<Option<(String, String)>>>>,
}

impl MockLedger {
    pub fn ok() -> Self {
        Self { capture: None }
    }

    pub fn capture(last: Arc<Mutex<Option<(String, String)>>>) -> Self {
        Self {
            capture: Some(last),
        }
    }
}

#[tonic::async_trait]
impl LedgerService for MockLedger {
    async fn post_transaction(
        &self,
        req: Request<PostTransactionRequest>,
    ) -> Result<Response<PostTransactionResponse>, Status> {
        if let Some(last) = &self.capture {
            let r = req.into_inner();
            *last.lock().unwrap() = Some((
                r.source_external_account_id,
                r.destination_external_account_id,
            ));
        }
        Ok(Response::new(PostTransactionResponse {
            status: "posted".into(),
            ledger_transaction_id: String::new(),
            failure_reason: String::new(),
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

async fn spawn_ledger_server(mock: MockLedger) -> String {
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

pub async fn serve_ledger_ok() -> String {
    spawn_ledger_server(MockLedger::ok()).await
}

pub async fn serve_ledger_capture() -> (String, Arc<Mutex<Option<(String, String)>>>) {
    let captured = Arc::new(Mutex::new(None));
    let url = spawn_ledger_server(MockLedger::capture(captured.clone())).await;
    (url, captured)
}
