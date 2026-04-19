use crate::errors::AppError;
use sqlx::PgPool;
use tonic::{Request, Response, Status};

use super::proto::{
    accounts_service_server::AccountsService,
    GetAccountBalanceRequest,
    GetAccountBalanceResponse,
};

#[derive(Clone)]
pub struct AccountsGrpcService {
    pool: PgPool,
}

impl AccountsGrpcService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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

fn map_app_error(err: AppError) -> Status {
    match err {
        AppError::NotFound(msg) => Status::not_found(msg),
        AppError::Validation(msg) => Status::invalid_argument(msg),
        AppError::BusinessLogic(msg) => Status::failed_precondition(msg),
        other => Status::internal(other.to_string()),
    }
}
