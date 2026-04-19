//! Postgres-backed tests for posting / claim paths (Docker via testcontainers).

mod common;

use accounts_api::errors::AppError;
use accounts_api::grpc::ledger_proto::ledger_service_server::{LedgerService, LedgerServiceServer};
use accounts_api::grpc::ledger_proto::{
    GetAccountBalanceRequest, GetAccountBalanceResponse, PostTransactionRequest,
    PostTransactionResponse,
};
use accounts_api::ledger_grpc::LedgerGrpc;
use accounts_api::models::{AccountType, TransactionStatus};
use accounts_api::repositories::{AccountRepository, TransactionRepository};
use accounts_api::services::transaction_retry::process_claimed_ledger_posts;
use chrono::Duration;
use common::migrated_pool;
use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{transport::Server, Request, Response, Status};
use uuid::Uuid;

async fn serve_ledger_ok() -> String {
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
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let incoming = TcpListenerStream::new(listener);
    tokio::spawn(async move {
        Server::builder()
            .add_service(LedgerServiceServer::new(MockOk))
            .serve_with_incoming(incoming)
            .await
            .ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    format!("http://{}", addr)
}

async fn serve_ledger_capture() -> (String, Arc<Mutex<Option<(String, String)>>>) {
    let captured = Arc::new(Mutex::new(None));

    #[derive(Clone)]
    struct MockCapture {
        last: Arc<Mutex<Option<(String, String)>>>,
    }

    #[tonic::async_trait]
    impl LedgerService for MockCapture {
        async fn post_transaction(
            &self,
            req: Request<PostTransactionRequest>,
        ) -> Result<Response<PostTransactionResponse>, Status> {
            let r = req.into_inner();
            *self.last.lock().unwrap() = Some((
                r.source_external_account_id,
                r.destination_external_account_id,
            ));
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
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let incoming = TcpListenerStream::new(listener);
    let mock = MockCapture {
        last: captured.clone(),
    };
    tokio::spawn(async move {
        Server::builder()
            .add_service(LedgerServiceServer::new(mock))
            .serve_with_incoming(incoming)
            .await
            .ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    (format!("http://{}", addr), captured)
}

async fn insert_pending_deposit(
    pool: &PgPool,
    org: Uuid,
    idem: &str,
    env: &str,
    created_age: Duration,
) -> Uuid {
    let id = Uuid::new_v4();
    let acc = Uuid::new_v4();
    let age_secs: i64 = created_age.num_seconds().max(1);
    sqlx::query(
        r#"
        INSERT INTO transactions (
            id, organization_id, from_account_id, to_account_id, amount, currency,
            transaction_kind, status, failure_reason, idempotency_key, environment,
            description, external_recipient_id, reference_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, 100, 'USD', 'deposit', 'pending', NULL, $5, $6,
                NULL, NULL, NULL,
                NOW() - ($7 * INTERVAL '1 second'),
                NOW() - ($7 * INTERVAL '1 second'))
        "#,
    )
    .bind(id)
    .bind(org)
    .bind(acc)
    .bind(acc)
    .bind(idem)
    .bind(env)
    .bind(age_secs)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn insert_pending_tx_kind(
    pool: &PgPool,
    org: Uuid,
    transaction_kind: &str,
    from_account_id: Uuid,
    to_account_id: Uuid,
    idem: &str,
    env: &str,
    created_age: Duration,
) -> Uuid {
    let id = Uuid::new_v4();
    let age_secs: i64 = created_age.num_seconds().max(1);
    sqlx::query(
        r#"
        INSERT INTO transactions (
            id, organization_id, from_account_id, to_account_id, amount, currency,
            transaction_kind, status, failure_reason, idempotency_key, environment,
            description, external_recipient_id, reference_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, 100, 'USD', $8, 'pending', NULL, $5, $6,
                NULL, NULL, NULL,
                NOW() - ($7 * INTERVAL '1 second'),
                NOW() - ($7 * INTERVAL '1 second'))
        "#,
    )
    .bind(id)
    .bind(org)
    .bind(from_account_id)
    .bind(to_account_id)
    .bind(idem)
    .bind(env)
    .bind(age_secs)
    .bind(transaction_kind)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn try_claim_pending_then_none_when_not_pending() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let id = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-{}", Uuid::new_v4()),
        "sandbox",
        Duration::seconds(10),
    )
    .await;

    let claimed = TransactionRepository::try_claim_pending_for_post(&pool, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.status, TransactionStatus::Posting);

    let second = TransactionRepository::try_claim_pending_for_post(&pool, id)
        .await
        .unwrap();
    assert!(second.is_none());
}

#[tokio::test]
async fn try_claim_pending_returns_none_for_unknown_id() {
    let (_c, pool) = migrated_pool().await;
    let r = TransactionRepository::try_claim_pending_for_post(&pool, Uuid::new_v4())
        .await
        .unwrap();
    assert!(r.is_none());
}

#[tokio::test]
async fn claim_batch_reclaims_stale_posting_then_claims_that_row() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let id = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-stale-{}", Uuid::new_v4()),
        "sandbox",
        Duration::hours(3),
    )
    .await;

    // Stale posting: updated_at far enough before DB `NOW()` that reclaim threshold (now - stale_after) still matches.
    sqlx::query(
        "UPDATE transactions SET status = 'posting', updated_at = NOW() - interval '30 seconds' WHERE id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();

    let claimed = TransactionRepository::claim_pending_transactions_for_ledger_post(
        &pool,
        Duration::seconds(1),
        Duration::seconds(10),
        Some(10),
        None,
    )
    .await
    .unwrap();

    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, id);
    assert_eq!(claimed[0].status, TransactionStatus::Posting);
}

#[tokio::test]
async fn claim_batch_reclaims_stale_posting_with_sandbox_env_filter() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let id = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-stale-sb-{}", Uuid::new_v4()),
        "sandbox",
        Duration::hours(3),
    )
    .await;

    sqlx::query(
        "UPDATE transactions SET status = 'posting', updated_at = NOW() - interval '30 seconds' WHERE id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();

    let claimed = TransactionRepository::claim_pending_transactions_for_ledger_post(
        &pool,
        Duration::seconds(1),
        Duration::seconds(10),
        Some(10),
        Some("sandbox"),
    )
    .await
    .unwrap();

    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, id);
    assert_eq!(claimed[0].status, TransactionStatus::Posting);
}

#[tokio::test]
async fn claim_batch_respects_environment_filter() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let id_sb = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-sb-{}", Uuid::new_v4()),
        "sandbox",
        Duration::hours(2),
    )
    .await;
    let _id_prod = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-pr-{}", Uuid::new_v4()),
        "production",
        Duration::hours(2),
    )
    .await;

    let claimed = TransactionRepository::claim_pending_transactions_for_ledger_post(
        &pool,
        Duration::seconds(1),
        Duration::seconds(300),
        Some(10),
        Some("sandbox"),
    )
    .await
    .unwrap();

    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, id_sb);
    assert_eq!(claimed[0].environment.as_deref(), Some("sandbox"));
}

#[tokio::test]
async fn process_claimed_posts_and_marks_posted() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let id = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-post-{}", Uuid::new_v4()),
        "sandbox",
        Duration::hours(2),
    )
    .await;

    let mut claimed = TransactionRepository::claim_pending_transactions_for_ledger_post(
        &pool,
        Duration::seconds(1),
        Duration::seconds(300),
        Some(10),
        None,
    )
    .await
    .unwrap();
    assert_eq!(claimed.len(), 1);
    let tx = claimed.pop().unwrap();

    let url = serve_ledger_ok().await;
    let ledger = LedgerGrpc::new(url);
    process_claimed_ledger_posts(&pool, &ledger, vec![tx]).await;

    let done = TransactionRepository::find_by_id(&pool, id).await.unwrap();
    assert_eq!(done.status, TransactionStatus::Posted);
}

#[tokio::test]
async fn process_claimed_on_ledger_error_returns_pending() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let id = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-fail-{}", Uuid::new_v4()),
        "sandbox",
        Duration::hours(2),
    )
    .await;

    let mut claimed = TransactionRepository::claim_pending_transactions_for_ledger_post(
        &pool,
        Duration::seconds(1),
        Duration::seconds(300),
        Some(10),
        None,
    )
    .await
    .unwrap();
    let tx = claimed.pop().unwrap();

    let ledger = LedgerGrpc::new("http://127.0.0.1:9".to_string());
    process_claimed_ledger_posts(&pool, &ledger, vec![tx]).await;

    let back = TransactionRepository::find_by_id(&pool, id).await.unwrap();
    assert_eq!(back.status, TransactionStatus::Pending);
    assert!(back.failure_reason.unwrap_or_default().len() > 0);
}

#[tokio::test]
async fn process_claimed_legacy_null_environment_releases_when_accounts_missing() {
    let (_c, pool) = migrated_pool().await;
    sqlx::query("ALTER TABLE transactions ALTER COLUMN environment DROP NOT NULL")
        .execute(&pool)
        .await
        .unwrap();

    let org = Uuid::new_v4();
    let id = Uuid::new_v4();
    let acc = Uuid::new_v4();
    let idem = format!("idem-null-{}", Uuid::new_v4());
    let created = chrono::Utc::now() - Duration::hours(2);
    sqlx::query(
        r#"
        INSERT INTO transactions (
            id, organization_id, from_account_id, to_account_id, amount, currency,
            transaction_kind, status, failure_reason, idempotency_key, environment,
            description, external_recipient_id, reference_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, 100, 'USD', 'deposit', 'posting', NULL, $5, NULL,
                NULL, NULL, NULL, $6, $6)
        "#,
    )
    .bind(id)
    .bind(org)
    .bind(acc)
    .bind(acc)
    .bind(&idem)
    .bind(created)
    .execute(&pool)
    .await
    .unwrap();

    let tx = TransactionRepository::find_by_id(&pool, id).await.unwrap();
    let url = serve_ledger_ok().await;
    let ledger = LedgerGrpc::new(url);
    process_claimed_ledger_posts(&pool, &ledger, vec![tx]).await;

    let released = TransactionRepository::find_by_id(&pool, id).await.unwrap();
    assert_eq!(released.status, TransactionStatus::Pending);
    let reason = released.failure_reason.unwrap_or_default();
    assert!(
        reason.contains("retry_worker"),
        "unexpected failure_reason: {reason}"
    );
}

#[tokio::test]
async fn claim_pending_default_limit_branch() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    for i in 0..3 {
        insert_pending_deposit(
            &pool,
            org,
            &format!("idem-many-{i}-{}", Uuid::new_v4()),
            "sandbox",
            Duration::hours(2),
        )
        .await;
    }
    let claimed = TransactionRepository::claim_pending_transactions_for_ledger_post(
        &pool,
        Duration::seconds(1),
        Duration::seconds(300),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(claimed.len(), 3);
}

#[tokio::test]
async fn process_claimed_transfer_posts_ledger_source_and_destination_accounts() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let from_a = Uuid::new_v4();
    let to_b = Uuid::new_v4();
    let id = insert_pending_tx_kind(
        &pool,
        org,
        "transfer",
        from_a,
        to_b,
        &format!("idem-xfer-{}", Uuid::new_v4()),
        "sandbox",
        Duration::hours(2),
    )
    .await;

    let mut claimed = TransactionRepository::claim_pending_transactions_for_ledger_post(
        &pool,
        Duration::seconds(1),
        Duration::seconds(300),
        Some(10),
        None,
    )
    .await
    .unwrap();
    let tx = claimed.pop().unwrap();

    let (url, cap) = serve_ledger_capture().await;
    let ledger = LedgerGrpc::new(url);
    process_claimed_ledger_posts(&pool, &ledger, vec![tx]).await;

    let posted = TransactionRepository::find_by_id(&pool, id).await.unwrap();
    assert_eq!(posted.status, TransactionStatus::Posted);

    let pair = cap.lock().unwrap().clone().expect("ledger post captured");
    assert_eq!(pair.0, from_a.to_string());
    assert_eq!(pair.1, to_b.to_string());
}

#[tokio::test]
async fn process_claimed_withdraw_posts_from_account_to_system_cash_control() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let from_a = Uuid::new_v4();
    let to_b = Uuid::new_v4();
    let id = insert_pending_tx_kind(
        &pool,
        org,
        "withdraw",
        from_a,
        to_b,
        &format!("idem-wd-{}", Uuid::new_v4()),
        "sandbox",
        Duration::hours(2),
    )
    .await;

    let mut claimed = TransactionRepository::claim_pending_transactions_for_ledger_post(
        &pool,
        Duration::seconds(1),
        Duration::seconds(300),
        Some(10),
        None,
    )
    .await
    .unwrap();
    let tx = claimed.pop().unwrap();

    let (url, cap) = serve_ledger_capture().await;
    let ledger = LedgerGrpc::new(url);
    process_claimed_ledger_posts(&pool, &ledger, vec![tx]).await;

    let posted = TransactionRepository::find_by_id(&pool, id).await.unwrap();
    assert_eq!(posted.status, TransactionStatus::Posted);

    let pair = cap.lock().unwrap().clone().expect("ledger post captured");
    assert_eq!(pair.0, from_a.to_string());
    assert_eq!(pair.1, "SYSTEM_CASH_CONTROL");
}

#[tokio::test]
async fn find_by_id_rejects_invalid_transaction_kind_after_constraint_drop() {
    let (_c, pool) = migrated_pool().await;
    sqlx::query(
        "ALTER TABLE transactions DROP CONSTRAINT IF EXISTS transactions_transaction_kind_check",
    )
    .execute(&pool)
    .await
    .unwrap();

    let org = Uuid::new_v4();
    let id = Uuid::new_v4();
    let acc = Uuid::new_v4();
    let idem = format!("idem-bad-kind-{}", Uuid::new_v4());
    let created = chrono::Utc::now() - Duration::hours(1);
    sqlx::query(
        r#"
        INSERT INTO transactions (
            id, organization_id, from_account_id, to_account_id, amount, currency,
            transaction_kind, status, failure_reason, idempotency_key, environment,
            description, external_recipient_id, reference_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, 100, 'USD', 'not_a_kind', 'pending', NULL, $5, 'sandbox',
                NULL, NULL, NULL, $6, $6)
        "#,
    )
    .bind(id)
    .bind(org)
    .bind(acc)
    .bind(acc)
    .bind(&idem)
    .bind(created)
    .execute(&pool)
    .await
    .unwrap();

    let err = TransactionRepository::find_by_id(&pool, id).await.unwrap_err();
    match err {
        AppError::Internal(msg) => assert!(msg.contains("Invalid transaction kind"), "{msg}"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn find_by_id_rejects_invalid_transaction_status_after_constraint_drop() {
    let (_c, pool) = migrated_pool().await;
    sqlx::query("ALTER TABLE transactions DROP CONSTRAINT IF EXISTS transactions_status_check")
        .execute(&pool)
        .await
        .unwrap();

    let org = Uuid::new_v4();
    let id = Uuid::new_v4();
    let acc = Uuid::new_v4();
    let idem = format!("idem-bad-status-{}", Uuid::new_v4());
    let created = chrono::Utc::now() - Duration::hours(1);
    sqlx::query(
        r#"
        INSERT INTO transactions (
            id, organization_id, from_account_id, to_account_id, amount, currency,
            transaction_kind, status, failure_reason, idempotency_key, environment,
            description, external_recipient_id, reference_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, 100, 'USD', 'deposit', 'bogus_status', NULL, $5, 'sandbox',
                NULL, NULL, NULL, $6, $6)
        "#,
    )
    .bind(id)
    .bind(org)
    .bind(acc)
    .bind(acc)
    .bind(&idem)
    .bind(created)
    .execute(&pool)
    .await
    .unwrap();

    let err = TransactionRepository::find_by_id(&pool, id).await.unwrap_err();
    match err {
        AppError::Internal(msg) => assert!(msg.contains("Invalid transaction status"), "{msg}"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn update_status_can_set_posting() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let id = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-posting-{}", Uuid::new_v4()),
        "sandbox",
        Duration::hours(1),
    )
    .await;

    let updated = TransactionRepository::update_status(
        &pool,
        id,
        TransactionStatus::Posting,
        None,
    )
    .await
    .unwrap();
    assert_eq!(updated.status, TransactionStatus::Posting);
}

#[tokio::test]
async fn process_claimed_null_environment_falls_back_to_production_account() {
    let (_c, pool) = migrated_pool().await;
    sqlx::query("ALTER TABLE transactions ALTER COLUMN environment DROP NOT NULL")
        .execute(&pool)
        .await
        .unwrap();

    let org = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let acc = AccountRepository::create(
        &pool,
        &format!("acct{}", Uuid::new_v4()),
        AccountType::Checking,
        Some(org),
        "production",
        user_id,
        "USD",
    )
    .await
    .unwrap();

    let id = Uuid::new_v4();
    let idem = format!("idem-null-prod-{}", Uuid::new_v4());
    let created = chrono::Utc::now() - Duration::hours(2);
    sqlx::query(
        r#"
        INSERT INTO transactions (
            id, organization_id, from_account_id, to_account_id, amount, currency,
            transaction_kind, status, failure_reason, idempotency_key, environment,
            description, external_recipient_id, reference_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, 100, 'USD', 'deposit', 'posting', NULL, $5, NULL,
                NULL, NULL, NULL, $6, $6)
        "#,
    )
    .bind(id)
    .bind(org)
    .bind(acc.id)
    .bind(acc.id)
    .bind(&idem)
    .bind(created)
    .execute(&pool)
    .await
    .unwrap();

    let tx = TransactionRepository::find_by_id(&pool, id).await.unwrap();
    let url = serve_ledger_ok().await;
    let ledger = LedgerGrpc::new(url);
    process_claimed_ledger_posts(&pool, &ledger, vec![tx]).await;

    let done = TransactionRepository::find_by_id(&pool, id).await.unwrap();
    assert_eq!(done.status, TransactionStatus::Posted);
}
