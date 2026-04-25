use tracing::info;
use std::time::Instant;
use crate::errors::AppError;
use crate::ledger_grpc::LedgerGrpc;
use crate::models::{Account, AccountStatus, CreateAccountRequest, Transaction, TransactionKind, TransactionStatus, PaginatedAccountsResponse};
use crate::repositories::{AccountHolderRepository, AccountRepository, TransactionRepository};
use crate::utils::generate_account_number;
use sqlx::PgPool;
use uuid::Uuid;

pub struct AccountService;

/// Whether this HTTP request should invoke the ledger or return the current row as-is.
enum ImmediateLedgerOutcome {
    ReturnWithoutPost(Transaction),
    Post(Transaction),
}

impl AccountService {
    /// Single-writer path: `posted` / `posting` / `failed` are returned unchanged; `pending` is claimed
    /// (`pending`→`posting`) so the retry worker cannot start a concurrent ledger post for the same row.
    async fn resolve_immediate_ledger_post(
        pool: &PgPool,
        transaction: Transaction,
    ) -> Result<ImmediateLedgerOutcome, AppError> {
        match transaction.status {
            TransactionStatus::Posted | TransactionStatus::Posting | TransactionStatus::Failed => {
                Ok(ImmediateLedgerOutcome::ReturnWithoutPost(transaction))
            }
            TransactionStatus::Pending => {
                if let Some(claimed) =
                    TransactionRepository::try_claim_pending_for_post(pool, transaction.id).await?
                {
                    Ok(ImmediateLedgerOutcome::Post(claimed))
                } else {
                    let current = TransactionRepository::find_by_id(pool, transaction.id).await?;
                    Ok(ImmediateLedgerOutcome::ReturnWithoutPost(current))
                }
            }
        }
    }

    /// Create account for a holder (SDK flow: email + names, API key in header).
    /// Resolves organization and admin from users service; enforces max 1 checking + 1 saving per holder.
    pub async fn create_account_with_holder(
        pool: &PgPool,
        request: CreateAccountRequest,
        organization_id: Uuid,
        admin_user_id: Option<Uuid>,
    ) -> Result<Account, AppError> {
        let email = request
            .email
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| AppError::Validation("email is required for holder-based account creation".to_string()))?;
        let environment = request.environment.as_deref().unwrap_or("sandbox");

        let holder = AccountHolderRepository::get_or_create(
            pool,
            organization_id,
            environment,
            email,
            request.first_name.as_deref().unwrap_or(""),
            request.last_name.as_deref().unwrap_or(""),
        )
        .await?;

        let count = AccountRepository::count_by_holder_and_type(
            pool,
            holder.id,
            request.account_type,
            environment,
        )
        .await?;
        if count >= 1 {
            let type_str = match request.account_type {
                crate::models::AccountType::Checking => "checking",
                crate::models::AccountType::Saving => "saving",
            };
            return Err(AppError::BusinessLogic(format!(
                "Holder already has a {} account (max 1 checking, 1 saving per holder)",
                type_str
            )));
        }

        let account_number = generate_account_number(pool, 12).await?;
        let account = AccountRepository::create_with_holder(
            pool,
            &account_number,
            request.account_type,
            organization_id,
            environment,
            holder.id,
            admin_user_id,
            &request.currency,
        )
        .await?;

        info!(
            "Account created (holder): id={}, account_number={}, holder_id={}",
            account.id, account.account_number, holder.id
        );
        Ok(account)
    }

    pub async fn create_account(
        pool: &PgPool,
        request: CreateAccountRequest,
    ) -> Result<Account, AppError> {
        // Holder-based path: email + names (no user_id). Caller must have called create_account_with_holder.
        // Legacy path: user_id and/or admin_user_id.
        let account_number = generate_account_number(pool, 12)
            .await?;

        // Use create_with_hierarchy if admin_user_id is provided (for customer accounts)
        let account = if let Some(admin_user_id) = request.admin_user_id {
            let user_id = request.user_id.ok_or_else(|| {
                AppError::Validation("user_id is required when admin_user_id is set (legacy path)".to_string())
            })?;
            AccountRepository::create_with_hierarchy(
                pool,
                &account_number,
                request.account_type,
                request.organization_id,
                &request.environment.unwrap_or_else(|| "sandbox".to_string()),
                user_id,
                Some(admin_user_id),
                Some("CUSTOMER".to_string()),  // Customer accounts require admin
                &request.currency,
            )
            .await?
        } else {
            let user_id = request.user_id.ok_or_else(|| {
                AppError::Validation("user_id is required for legacy account creation".to_string())
            })?;
            AccountRepository::create(
                pool,
                &account_number,
                request.account_type,
                request.organization_id,
                &request.environment.unwrap_or_else(|| "sandbox".to_string()),
                user_id,
                &request.currency,
            )
            .await?
        };

        info!(
            "Account created: id={}, account_number={}, user_id={:?}",
            account.id, account.account_number, request.user_id
        );

        Ok(account)
    }

    pub async fn get_account(pool: &PgPool, id: Uuid, environment: &str) -> Result<Account, AppError> {
        AccountRepository::find_by_id(pool, id, environment).await
    }

    pub async fn get_accounts_by_user(
        pool: &PgPool,
        user_id: Uuid,
        environment: &str,
    ) -> Result<Vec<Account>, AppError> {
        AccountRepository::find_by_user_id(pool, user_id, environment).await
    }

    pub async fn get_accounts_by_organization(
        pool: &PgPool,
        organization_id: Uuid,
        environment: &str,
    ) -> Result<Vec<Account>, AppError> {
        AccountRepository::find_by_organization_id(pool, organization_id, environment).await
    }

    pub async fn get_accounts_by_admin(
        pool: &PgPool,
        admin_user_id: Uuid,
        environment: &str,
    ) -> Result<Vec<Account>, AppError> {
        AccountRepository::find_by_admin_user_id(pool, admin_user_id, environment).await
    }

    pub async fn get_accounts_by_user_paginated(
        pool: &PgPool,
        user_id: Uuid,
        environment: &str,
        page: u32,
        per_page: u32,
    ) -> Result<PaginatedAccountsResponse, AppError> {
        AccountRepository::find_by_user_id_paginated(pool, user_id, environment, page, per_page).await
    }

    pub async fn get_accounts_by_organization_paginated(
        pool: &PgPool,
        organization_id: Uuid,
        environment: &str,
        page: u32,
        per_page: u32,
    ) -> Result<PaginatedAccountsResponse, AppError> {
        AccountRepository::find_by_organization_id_paginated(pool, organization_id, environment, page, per_page).await
    }

    pub async fn get_accounts_by_admin_paginated(
        pool: &PgPool,
        admin_user_id: Uuid,
        environment: &str,
        page: u32,
        per_page: u32,
    ) -> Result<PaginatedAccountsResponse, AppError> {
        AccountRepository::find_by_admin_user_id_paginated(pool, admin_user_id, environment, page, per_page).await
    }

    pub async fn update_account_status(
        pool: &PgPool,
        id: Uuid,
        environment: &str,
        status: AccountStatus,
    ) -> Result<Account, AppError> {
        let account = AccountRepository::find_by_id(pool, id, environment).await?;

        if account.status == Some(AccountStatus::Closed) && status != AccountStatus::Closed {
            return Err(AppError::BusinessLogic(
                "Cannot reactivate a closed account".to_string(),
            ));
        }

        info!(
            "Updating account {} status to {:?}",
            id, status
        );

        AccountRepository::update_status(pool, id, environment, status).await
    }

    pub async fn close_account(pool: &PgPool, id: Uuid, environment: &str) -> Result<Account, AppError> {
        Self::update_account_status(pool, id, environment, AccountStatus::Closed).await
    }

    pub async fn deposit_with_idempotency(
        pool: &PgPool,
        account_id: Uuid,
        environment: &str,
        amount: i64,
        idempotency_key: &str,
        ledger_grpc: &LedgerGrpc,
        correlation_id: Option<String>,
        description: Option<&str>,
        external_recipient_id: Option<&str>,
        reference_id: Option<Uuid>,
    ) -> Result<(Account, crate::models::Transaction), AppError> {
        if idempotency_key.trim().is_empty() {
            return Err(AppError::Validation("Idempotency-Key header is required".to_string()));
        }

        let account = AccountRepository::find_by_id(pool, account_id, environment).await?;

        if account.status != Some(AccountStatus::Active) {
            return Err(AppError::AccountNotActive);
        }

        let organization_id = account
            .organization_id
            .ok_or_else(|| AppError::Validation("organization_id is required".to_string()))?;

        let currency = account
            .currency
            .clone()
            .unwrap_or_else(|| "USD".to_string());

        // Use environment from header (already validated), not from account record
        // This ensures we're operating in the correct environment context

        let mut tx = pool.begin().await?;

        let transaction = TransactionRepository::create_or_get_by_idempotency(
            &mut *tx,
            organization_id,
            account_id,
            account_id,
            amount,
            &currency,
            TransactionKind::Deposit,
            idempotency_key,
            environment,
            description,
            external_recipient_id,
            reference_id,
        )
        .await?;

        tx.commit().await?;

        info!(
            organization_id = %organization_id,
            transaction_id = %transaction.id,
            from_account_id = %transaction.from_account_id,
            to_account_id = %transaction.to_account_id,
            status = ?transaction.status,
            "transaction_intent_created"
        );

        let transaction = match Self::resolve_immediate_ledger_post(pool, transaction).await? {
            ImmediateLedgerOutcome::ReturnWithoutPost(tx) => tx,
            ImmediateLedgerOutcome::Post(tx) => {
                let correlation_id = correlation_id.unwrap_or_else(|| tx.id.to_string());
                let started_at = Instant::now();
                let post_result = ledger_grpc
                    .post_transaction(
                        organization_id,
                        &environment,
                        "SYSTEM_CASH_CONTROL".to_string(),
                        account_id.to_string(),
                        amount,
                        currency.clone(),
                        tx.id,
                        tx.idempotency_key.clone(),
                        correlation_id,
                    )
                    .await;
                tracing::info!(
                    transaction_id = %tx.id,
                    transaction_kind = "deposit",
                    elapsed_ms = started_at.elapsed().as_millis(),
                    success = post_result.is_ok(),
                    "ledger_post_transaction_timing"
                );

                match post_result {
                    Ok(()) => {
                        TransactionRepository::update_status(pool, tx.id, TransactionStatus::Posted, None).await?
                    }
                    Err(e) => {
                        let reason = format!("{}", e);
                        tracing::warn!(
                            transaction_id = %tx.id,
                            error = %reason,
                            "Ledger gRPC post failed; leaving transaction pending"
                        );
                        TransactionRepository::update_status(
                            pool,
                            tx.id,
                            TransactionStatus::Pending,
                            Some(&reason),
                        )
                        .await?
                    }
                }
            }
        };

        Ok((account, transaction))
    }

    pub async fn withdraw_with_idempotency(
        pool: &PgPool,
        account_id: Uuid,
        environment: &str,
        amount: i64,
        idempotency_key: &str,
        ledger_grpc: &LedgerGrpc,
        correlation_id: Option<String>,
        description: Option<&str>,
        external_recipient_id: Option<&str>,
        reference_id: Option<Uuid>,
    ) -> Result<(Account, crate::models::Transaction), AppError> {
        // Note: Withdrawals are negative amounts, but we store as positive
        // The ledger will handle the debit/credit logic
        if idempotency_key.trim().is_empty() {
            return Err(AppError::Validation("Idempotency-Key header is required".to_string()));
        }

        let account = AccountRepository::find_by_id(pool, account_id, environment).await?;

        if account.status != Some(AccountStatus::Active) {
            return Err(AppError::AccountNotActive);
        }

        let organization_id = account
            .organization_id
            .ok_or_else(|| AppError::Validation("organization_id is required".to_string()))?;

        let currency = account
            .currency
            .clone()
            .unwrap_or_else(|| "USD".to_string());

        // Idempotency: if we already have a transaction for this key, use it (skip overdraft check on retry)
        if let Some(existing) = TransactionRepository::find_by_idempotency(
            pool,
            organization_id,
            environment,
            idempotency_key,
        )
        .await?
        {
            if matches!(
                existing.status,
                TransactionStatus::Posted | TransactionStatus::Posting
            ) {
                return Ok((account, existing));
            }
            // Pending: fall through to attempt Ledger post
        } else {
            // New transaction: block overdrafts before creating
            let ledger_balance_str = ledger_grpc
                .get_account_balance(organization_id, environment, account_id, &currency)
                .await?;
            let ledger_balance: i64 = ledger_balance_str
                .trim()
                .parse()
                .map_err(|_| AppError::Internal("Failed to parse Ledger balance".to_string()))?;
            let display_balance = -ledger_balance; // Ledger stores liability as negative; negate for customer view
            if display_balance < amount {
                return Err(AppError::InsufficientFunds);
            }
        }

        // Use environment from header (already validated), not from account record
        // This ensures we're operating in the correct environment context

        let mut tx = pool.begin().await?;

        let transaction = TransactionRepository::create_or_get_by_idempotency(
            &mut *tx,
            organization_id,
            account_id,
            account_id,
            amount,
            &currency,
            TransactionKind::Withdraw,
            idempotency_key,
            environment,
            description,
            external_recipient_id,
            reference_id,
        )
        .await?;

        tx.commit().await?;

        info!(
            organization_id = %organization_id,
            transaction_id = %transaction.id,
            from_account_id = %transaction.from_account_id,
            to_account_id = %transaction.to_account_id,
            status = ?transaction.status,
            "transaction_intent_created"
        );

        let transaction = match Self::resolve_immediate_ledger_post(pool, transaction).await? {
            ImmediateLedgerOutcome::ReturnWithoutPost(tx) => tx,
            ImmediateLedgerOutcome::Post(tx) => {
                let correlation_id = correlation_id.unwrap_or_else(|| tx.id.to_string());
                let started_at = Instant::now();
                let post_result = ledger_grpc
                    .post_transaction(
                        organization_id,
                        &environment,
                        account_id.to_string(),
                        "SYSTEM_CASH_CONTROL".to_string(),
                        amount,
                        currency.clone(),
                        tx.id,
                        tx.idempotency_key.clone(),
                        correlation_id,
                    )
                    .await;
                tracing::info!(
                    transaction_id = %tx.id,
                    transaction_kind = "withdraw",
                    elapsed_ms = started_at.elapsed().as_millis(),
                    success = post_result.is_ok(),
                    "ledger_post_transaction_timing"
                );

                match post_result {
                    Ok(()) => {
                        TransactionRepository::update_status(pool, tx.id, TransactionStatus::Posted, None).await?
                    }
                    Err(e) => {
                        let reason = format!("{}", e);
                        tracing::warn!(
                            transaction_id = %tx.id,
                            error = %reason,
                            "Ledger gRPC post failed; leaving transaction pending"
                        );
                        TransactionRepository::update_status(
                            pool,
                            tx.id,
                            TransactionStatus::Pending,
                            Some(&reason),
                        )
                        .await?
                    }
                }
            }
        };

        Ok((account, transaction))
    }

    pub async fn transfer_with_idempotency(
        pool: &PgPool,
        from_account_id: Uuid,
        environment: &str,
        to_account_id: Uuid,
        amount: i64,
        idempotency_key: &str,
        ledger_grpc: &LedgerGrpc,
        correlation_id: Option<String>,
        description: Option<&str>,
        external_recipient_id: Option<&str>,
        reference_id: Option<Uuid>,
    ) -> Result<(Account, Account, crate::models::Transaction), AppError> {
        if idempotency_key.trim().is_empty() {
            return Err(AppError::Validation("Idempotency-Key header is required".to_string()));
        }

        let from_account = AccountRepository::find_by_id(pool, from_account_id, environment).await?;

        if from_account.status != Some(AccountStatus::Active) {
            return Err(AppError::AccountNotActive);
        }


        let to_account = AccountRepository::find_by_id(pool, to_account_id, environment).await?;

        if to_account.status != Some(AccountStatus::Active) {
            return Err(AppError::AccountNotActive);
        }

        let from_org = from_account
            .organization_id
            .ok_or_else(|| AppError::Validation("organization_id is required".to_string()))?;
        let to_org = to_account
            .organization_id
            .ok_or_else(|| AppError::Validation("organization_id is required".to_string()))?;

        if from_org != to_org {
            return Err(AppError::Validation(
                "accounts must belong to the same organization".to_string(),
            ));
        }

        let from_currency = from_account
            .currency
            .clone()
            .unwrap_or_else(|| "USD".to_string());
        let to_currency = to_account
            .currency
            .clone()
            .unwrap_or_else(|| "USD".to_string());

        if from_currency != to_currency {
            return Err(AppError::Validation(
                "currency must match both accounts".to_string(),
            ));
        }

        // Idempotency: if we already have a transaction for this key, use it (skip overdraft check on retry)
        if let Some(existing) = TransactionRepository::find_by_idempotency(
            pool,
            from_org,
            environment,
            idempotency_key,
        )
        .await?
        {
            if matches!(
                existing.status,
                TransactionStatus::Posted | TransactionStatus::Posting
            ) {
                return Ok((from_account, to_account, existing));
            }
            // Pending: fall through to attempt Ledger post
        } else {
            // New transaction: block overdrafts before creating
            let ledger_balance_str = ledger_grpc
                .get_account_balance(from_org, environment, from_account_id, &from_currency)
                .await?;
            let ledger_balance: i64 = ledger_balance_str
                .trim()
                .parse()
                .map_err(|_| AppError::Internal("Failed to parse Ledger balance".to_string()))?;
            let display_balance = -ledger_balance; // Ledger stores liability as negative; negate for customer view
            if display_balance < amount {
                return Err(AppError::InsufficientFunds);
            }
        }

        // Use environment from header (already validated), not from account record
        // This ensures we're operating in the correct environment context

        let mut tx = pool.begin().await?;

        let transaction = TransactionRepository::create_or_get_by_idempotency(
            &mut *tx,
            from_org,
            from_account_id,
            to_account_id,
            amount,
            &from_currency,
            TransactionKind::Transfer,
            idempotency_key,
            environment,
            description,
            external_recipient_id,
            reference_id,
        )
        .await?;

        tx.commit().await?;

        info!(
            organization_id = %from_org,
            transaction_id = %transaction.id,
            from_account_id = %transaction.from_account_id,
            to_account_id = %transaction.to_account_id,
            status = ?transaction.status,
            "transaction_intent_created"
        );

        let transaction = match Self::resolve_immediate_ledger_post(pool, transaction).await? {
            ImmediateLedgerOutcome::ReturnWithoutPost(tx) => tx,
            ImmediateLedgerOutcome::Post(tx) => {
                let correlation_id = correlation_id.unwrap_or_else(|| tx.id.to_string());
                let started_at = Instant::now();
                let post_result = ledger_grpc
                    .post_transaction(
                        from_org,
                        &environment,
                        from_account_id.to_string(),
                        to_account_id.to_string(),
                        amount,
                        from_currency.clone(),
                        tx.id,
                        tx.idempotency_key.clone(),
                        correlation_id,
                    )
                    .await;
                tracing::info!(
                    transaction_id = %tx.id,
                    transaction_kind = "transfer",
                    elapsed_ms = started_at.elapsed().as_millis(),
                    success = post_result.is_ok(),
                    "ledger_post_transaction_timing"
                );

                match post_result {
                    Ok(()) => {
                        TransactionRepository::update_status(pool, tx.id, TransactionStatus::Posted, None).await?
                    }
                    Err(e) => {
                        let reason = format!("{}", e);
                        tracing::warn!(
                            transaction_id = %tx.id,
                            error = %reason,
                            "Ledger gRPC post failed; leaving transaction pending"
                        );
                        TransactionRepository::update_status(
                            pool,
                            tx.id,
                            TransactionStatus::Pending,
                            Some(&reason),
                        )
                        .await?
                    }
                }
            }
        };

        Ok((from_account, to_account, transaction))
    }
}

#[cfg(test)]
mod immediate_ledger_resolve_tests {
    use super::{AccountService, ImmediateLedgerOutcome};
    use crate::models::{Transaction, TransactionStatus};
    use crate::repositories::TransactionRepository;
    use chrono::Duration;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres;
    use uuid::Uuid;

    async fn migrated_pool() -> (testcontainers::ContainerAsync<Postgres>, PgPool) {
        let container = Postgres::default()
            .start()
            .await
            .expect("start postgres testcontainer");
        let host = container.get_host().await.expect("container host");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("container port");
        let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .expect("connect to test postgres");
        sqlx::query(r#"CREATE EXTENSION IF NOT EXISTS "pgcrypto""#)
            .execute(&pool)
            .await
            .expect("create pgcrypto extension for gen_random_uuid");
        sqlx::migrate!("./migrations_accounts")
            .run(&pool)
            .await
            .expect("run migrations_accounts");
        (container, pool)
    }

    async fn insert_pending(pool: &PgPool, org: Uuid, idem: &str) -> Transaction {
        let id = Uuid::new_v4();
        let acc = Uuid::new_v4();
        let age_secs: i64 = Duration::minutes(5).num_seconds().max(1);
        sqlx::query(
            r#"
            INSERT INTO transactions (
                id, organization_id, from_account_id, to_account_id, amount, currency,
                transaction_kind, status, failure_reason, idempotency_key, environment,
                description, external_recipient_id, reference_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, 50, 'USD', 'deposit', 'pending', NULL, $5, 'sandbox',
                    NULL, NULL, NULL,
                    NOW() - ($6 * INTERVAL '1 second'),
                    NOW() - ($6 * INTERVAL '1 second'))
            "#,
        )
        .bind(id)
        .bind(org)
        .bind(acc)
        .bind(acc)
        .bind(idem)
        .bind(age_secs)
        .execute(pool)
        .await
        .unwrap();
        TransactionRepository::find_by_id(pool, id).await.unwrap()
    }

    #[tokio::test]
    async fn resolve_posted_returns_without_claim() {
        let (_c, pool) = migrated_pool().await;
        let org = Uuid::new_v4();
        let mut tx = insert_pending(&pool, org, &format!("r1-{}", Uuid::new_v4())).await;
        tx = TransactionRepository::update_status(&pool, tx.id, TransactionStatus::Posted, None)
            .await
            .unwrap();

        let out = AccountService::resolve_immediate_ledger_post(&pool, tx.clone())
            .await
            .unwrap();
        match out {
            ImmediateLedgerOutcome::ReturnWithoutPost(t) => assert_eq!(t.status, TransactionStatus::Posted),
            ImmediateLedgerOutcome::Post(_) => panic!("expected short-circuit"),
        }
    }

    #[tokio::test]
    async fn resolve_posting_in_memory_returns_without_claim() {
        let (_c, pool) = migrated_pool().await;
        let org = Uuid::new_v4();
        let mut tx = insert_pending(&pool, org, &format!("r2-{}", Uuid::new_v4())).await;
        tx.status = TransactionStatus::Posting;

        let out = AccountService::resolve_immediate_ledger_post(&pool, tx)
            .await
            .unwrap();
        match out {
            ImmediateLedgerOutcome::ReturnWithoutPost(t) => assert_eq!(t.status, TransactionStatus::Posting),
            ImmediateLedgerOutcome::Post(_) => panic!("expected short-circuit"),
        }
    }

    #[tokio::test]
    async fn resolve_failed_returns_without_claim() {
        let (_c, pool) = migrated_pool().await;
        let org = Uuid::new_v4();
        let mut tx = insert_pending(&pool, org, &format!("r3-{}", Uuid::new_v4())).await;
        tx = TransactionRepository::update_status(&pool, tx.id, TransactionStatus::Failed, Some("x"))
            .await
            .unwrap();

        let out = AccountService::resolve_immediate_ledger_post(&pool, tx.clone())
            .await
            .unwrap();
        match out {
            ImmediateLedgerOutcome::ReturnWithoutPost(t) => assert_eq!(t.status, TransactionStatus::Failed),
            ImmediateLedgerOutcome::Post(_) => panic!("expected short-circuit"),
        }
    }

    #[tokio::test]
    async fn resolve_pending_wins_claim() {
        let (_c, pool) = migrated_pool().await;
        let org = Uuid::new_v4();
        let tx = insert_pending(&pool, org, &format!("r4-{}", Uuid::new_v4())).await;

        let out = AccountService::resolve_immediate_ledger_post(&pool, tx)
            .await
            .unwrap();
        match out {
            ImmediateLedgerOutcome::Post(t) => assert_eq!(t.status, TransactionStatus::Posting),
            ImmediateLedgerOutcome::ReturnWithoutPost(_) => panic!("expected claim"),
        }
    }

    #[tokio::test]
    async fn resolve_pending_loses_race_returns_current_row() {
        let (_c, pool) = migrated_pool().await;
        let org = Uuid::new_v4();
        let tx = insert_pending(&pool, org, &format!("r5-{}", Uuid::new_v4())).await;
        let _claimed_elsewhere =
            TransactionRepository::try_claim_pending_for_post(&pool, tx.id)
                .await
                .unwrap()
                .unwrap();

        let out = AccountService::resolve_immediate_ledger_post(&pool, tx)
            .await
            .unwrap();
        match out {
            ImmediateLedgerOutcome::ReturnWithoutPost(t) => {
                assert_eq!(t.status, TransactionStatus::Posting)
            }
            ImmediateLedgerOutcome::Post(_) => panic!("expected reload after lost claim"),
        }
    }
}
