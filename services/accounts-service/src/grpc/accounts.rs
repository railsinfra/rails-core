use tonic::{Request, Response, Status};

use super::proto::{
    accounts_service_server::AccountsService,
    GetAccountBalanceRequest,
    GetAccountBalanceResponse,
};

#[derive(Clone)]
pub struct AccountsGrpcService;

impl AccountsGrpcService {
    pub fn new() -> Self {
        Self
    }
}

#[tonic::async_trait]
impl AccountsService for AccountsGrpcService {
    async fn get_account_balance(
        &self,
        request: Request<GetAccountBalanceRequest>,
    ) -> Result<Response<GetAccountBalanceResponse>, Status> {
        let _req = request.into_inner();
        Err(Status::unimplemented(
            "GetAccountBalance is not supported by Accounts service",
        ))
    }
}
