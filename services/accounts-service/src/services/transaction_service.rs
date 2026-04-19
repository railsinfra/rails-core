use crate::errors::AppError;
use crate::models::Transaction;
use crate::repositories::{AccountRepository, TransactionRepository};
use sqlx::PgPool;
use uuid::Uuid;

pub struct TransactionService;

impl TransactionService {
    pub async fn get_transaction(pool: &PgPool, id: Uuid, environment: &str) -> Result<Transaction, AppError> {
        // Transactions don't have environment column, but we verify the account is in the correct environment
        // by checking the account exists in that environment first
        let transaction = TransactionRepository::find_by_id(pool, id).await?;
        
        // Verify the from_account is in the correct environment
        let _account = AccountRepository::find_by_id(pool, transaction.from_account_id, environment).await?;
        
        Ok(transaction)
    }

    pub async fn get_account_transactions(
        pool: &PgPool,
        account_id: Uuid,
        environment: &str,
        limit: Option<i64>,
    ) -> Result<Vec<Transaction>, AppError> {
        // Verify account exists in the correct environment before fetching transactions
        let _account = AccountRepository::find_by_id(pool, account_id, environment).await?;
        
        // Filter by environment, but include legacy transactions (NULL environment)
        TransactionRepository::find_by_account_id(pool, account_id, limit, Some(environment)).await
    }

    pub async fn get_transactions_by_organization_paginated(
        pool: &PgPool,
        organization_id: Uuid,
        environment: &str,
        page: u32,
        per_page: u32,
    ) -> Result<(Vec<Transaction>, crate::models::PaginationMeta), AppError> {
        TransactionRepository::find_by_organization_id_paginated(
            pool,
            organization_id,
            environment,
            page,
            per_page,
        )
        .await
    }
}
