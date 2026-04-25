use chrono::Duration;
use sqlx::PgPool;
use std::time::Duration as StdDuration;
use tracing::{info, warn};

use crate::errors::AppError;
use crate::ledger_grpc::LedgerGrpc;
use crate::models::{Transaction, TransactionKind, TransactionStatus};
use crate::repositories::{AccountRepository, TransactionRepository};

pub(crate) async fn handle_retry_claim_outcome(
    outcome: Result<Vec<Transaction>, AppError>,
    claim_error_sleep: StdDuration,
) -> Option<Vec<Transaction>> {
    match outcome {
        Ok(rows) => Some(rows),
        Err(e) => {
            warn!(error = %e, "retry_worker_failed_to_claim_pending");
            tokio::time::sleep(claim_error_sleep).await;
            None
        }
    }
}

pub(crate) fn stale_posting_secs_from_env() -> i64 {
    const TRANSACTION_POSTING_STALE_AFTER_SECS_ENV: &str = "TRANSACTION_POSTING_STALE_AFTER_SECS";
    std::env::var(TRANSACTION_POSTING_STALE_AFTER_SECS_ENV)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(600)
}

fn stale_posting_duration() -> Duration {
    Duration::seconds(stale_posting_secs_from_env())
}

/// One retry-worker iteration: claim a batch, post claimed rows, then idle sleep.
pub(crate) async fn retry_worker_poll_once(
    pool: &PgPool,
    ledger: &LedgerGrpc,
    claim_error_sleep: StdDuration,
    idle_sleep: StdDuration,
) {
    let outcome = TransactionRepository::claim_pending_transactions_for_ledger_post(
        pool,
        Duration::seconds(2),
        stale_posting_duration(),
        Some(200),
        None,
    )
    .await;

    if let Some(pending) = handle_retry_claim_outcome(outcome, claim_error_sleep).await {
        process_claimed_ledger_posts(pool, ledger, pending).await;
    }

    tokio::time::sleep(idle_sleep).await;
}

/// Posts rows already moved to `posting` by [`TransactionRepository::claim_pending_transactions_for_ledger_post`].
pub async fn process_claimed_ledger_posts(pool: &PgPool, ledger_grpc: &LedgerGrpc, pending: Vec<Transaction>) {
    for tx in pending {
        let environment = if let Some(ref env) = tx.environment {
            env.clone()
        } else {
            let account = match AccountRepository::find_by_id(pool, tx.from_account_id, "sandbox").await {
                Ok(a) => a,
                Err(_) => match AccountRepository::find_by_id(pool, tx.from_account_id, "production").await {
                    Ok(a) => a,
                    Err(e) => {
                        warn!(
                            transaction_id = %tx.id,
                            error = %e,
                            "retry_worker_missing_account; releasing posting row"
                        );
                        let _ = TransactionRepository::update_status(
                            pool,
                            tx.id,
                            TransactionStatus::Pending,
                            Some("retry_worker: could not resolve account environment"),
                        )
                        .await;
                        continue;
                    }
                },
            };

            account
                .environment
                .clone()
                .unwrap_or_else(|| "sandbox".to_string())
        };

        let (source_external, dest_external) = match tx.transaction_kind {
            TransactionKind::Transfer => (
                tx.from_account_id.to_string(),
                tx.to_account_id.to_string(),
            ),
            TransactionKind::Deposit => (
                "SYSTEM_CASH_CONTROL".to_string(),
                tx.to_account_id.to_string(),
            ),
            TransactionKind::Withdraw => (
                tx.from_account_id.to_string(),
                "SYSTEM_CASH_CONTROL".to_string(),
            ),
        };

        let post_result = ledger_grpc
            .post_transaction(
                tx.organization_id,
                &environment,
                source_external,
                dest_external,
                tx.amount,
                tx.currency.clone(),
                tx.id,
                tx.idempotency_key.clone(),
                tx.id.to_string(),
            )
            .await;

        match post_result {
            Ok(()) => {
                let _ = TransactionRepository::update_status(pool, tx.id, TransactionStatus::Posted, None).await;
            }
            Err(e) => {
                let reason = format!("{}", e);
                let _ = TransactionRepository::update_status(
                    pool,
                    tx.id,
                    TransactionStatus::Pending,
                    Some(&reason),
                )
                .await;
            }
        }
    }
}

/// Best-effort background retry loop that posts pending transactions to the Ledger via gRPC.
/// Eventual consistency: transactions remain pending until Ledger accepts them.
pub async fn run(pool: PgPool, ledger_grpc: LedgerGrpc) {
    info!("Ledger gRPC retry worker started");

    loop {
        retry_worker_poll_once(
            &pool,
            &ledger_grpc,
            StdDuration::from_secs(5),
            StdDuration::from_secs(3),
        )
        .await;
    }
}

#[cfg(test)]
mod claim_outcome_tests {
    use super::handle_retry_claim_outcome;
    use crate::errors::AppError;
    use crate::models::{Transaction, TransactionKind, TransactionStatus};
    use chrono::Utc;
    use std::time::Duration;
    use uuid::Uuid;

    #[tokio::test]
    async fn claim_outcome_ok_empty_vec() {
        let out = handle_retry_claim_outcome(Ok(vec![]), Duration::ZERO).await;
        assert!(out.as_ref().is_some_and(|v| v.is_empty()));
    }

    #[tokio::test]
    async fn claim_outcome_ok_non_empty() {
        let now = Utc::now();
        let tx = Transaction {
            id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            from_account_id: Uuid::new_v4(),
            to_account_id: Uuid::new_v4(),
            amount: 1,
            currency: "USD".into(),
            transaction_kind: TransactionKind::Deposit,
            status: TransactionStatus::Posting,
            failure_reason: None,
            idempotency_key: "k".into(),
            environment: Some("sandbox".into()),
            description: None,
            external_recipient_id: None,
            reference_id: None,
            created_at: now,
            updated_at: now,
        };
        let out = handle_retry_claim_outcome(Ok(vec![tx]), Duration::ZERO)
            .await
            .expect("some rows");
        assert_eq!(out.len(), 1);
    }

    #[tokio::test]
    async fn claim_outcome_err_returns_none_after_sleep() {
        let out = handle_retry_claim_outcome(
            Err(AppError::Internal("db down".into())),
            Duration::ZERO,
        )
        .await;
        assert!(out.is_none());
    }
}

#[cfg(test)]
mod poll_smoke {
    use super::retry_worker_poll_once;
    use crate::ledger_grpc::LedgerGrpc;
    use crate::models::TransactionStatus;
    use crate::repositories::TransactionRepository;
    use crate::run_accounts_migrations;
    use chrono::Duration;
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration as StdDuration;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres;
    use uuid::Uuid;

    async fn migrated_pool() -> (testcontainers::ContainerAsync<Postgres>, sqlx::PgPool) {
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
            .max_connections(3)
            .connect(&url)
            .await
            .expect("connect to test postgres");
        sqlx::query(r#"CREATE EXTENSION IF NOT EXISTS "pgcrypto""#)
            .execute(&pool)
            .await
            .expect("create pgcrypto extension for gen_random_uuid");
        run_accounts_migrations(&pool)
            .await
            .expect("run accounts migrations");
        (container, pool)
    }

    #[tokio::test]
    async fn poll_once_empty_db_completes() {
        let (_c, pool) = migrated_pool().await;
        let ledger = LedgerGrpc::new("http://127.0.0.1:9".to_string());
        retry_worker_poll_once(&pool, &ledger, StdDuration::ZERO, StdDuration::ZERO).await;
    }

    #[tokio::test]
    async fn poll_once_claims_then_releases_pending_on_ledger_error() {
        let (_c, pool) = migrated_pool().await;
        let org = Uuid::new_v4();
        let acc = Uuid::new_v4();
        let id = Uuid::new_v4();
        let idem = format!("idem-poll-{}", Uuid::new_v4());
        let age_secs: i64 = Duration::hours(2).num_seconds().max(1);
        sqlx::query(
            r#"
            INSERT INTO transactions (
                id, organization_id, from_account_id, to_account_id, amount, currency,
                transaction_kind, status, failure_reason, idempotency_key, environment,
                description, external_recipient_id, reference_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, 100, 'USD', 'deposit', 'pending', NULL, $5, 'sandbox',
                    NULL, NULL, NULL,
                    NOW() - ($6 * INTERVAL '1 second'),
                    NOW() - ($6 * INTERVAL '1 second'))
            "#,
        )
        .bind(id)
        .bind(org)
        .bind(acc)
        .bind(acc)
        .bind(&idem)
        .bind(age_secs)
        .execute(&pool)
        .await
        .expect("insert pending tx");

        let ledger = LedgerGrpc::new("http://127.0.0.1:9".to_string());
        retry_worker_poll_once(&pool, &ledger, StdDuration::ZERO, StdDuration::ZERO).await;

        let row = TransactionRepository::find_by_id(&pool, id).await.expect("row");
        assert_eq!(row.status, TransactionStatus::Pending);
        assert!(row.failure_reason.unwrap_or_default().len() > 0);
    }

    #[tokio::test]
    async fn run_loop_executes_until_task_abort() {
        use super::run;

        let (_c, pool) = migrated_pool().await;
        let ledger = LedgerGrpc::new("http://127.0.0.1:9".to_string());
        let pool2 = pool.clone();
        let handle = tokio::spawn(async move { run(pool2, ledger).await });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        handle.abort();
        let join = handle.await;
        assert!(join.expect_err("aborted").is_cancelled());
    }
}

#[cfg(test)]
mod stale_secs_tests {
    use super::stale_posting_secs_from_env;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn stale_secs_default_when_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(TRANSACTION_POSTING_STALE_AFTER_SECS);
        assert_eq!(stale_posting_secs_from_env(), 600);
    }

    #[test]
    static TRANSACTION_POSTING_STALE_AFTER_SECS: &str = "TRANSACTION_POSTING_STALE_AFTER_SECS";
        fn stale_secs_from_env() {
            let _g = ENV_LOCK.lock().unwrap();
            std::env::set_var(TRANSACTION_POSTING_STALE_AFTER_SECS, "120");
            assert_eq!(stale_posting_secs_from_env(), 120);
            std::env::remove_var(TRANSACTION_POSTING_STALE_AFTER_SECS);
        }
    #[test]
    static TRANSACTION_POSTING_STALE_AFTER_SECS: &str = "TRANSACTION_POSTING_STALE_AFTER_SECS";
    fn stale_secs_invalid_env_falls_back() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var(TRANSACTION_POSTING_STALE_AFTER_SECS, "0");
        assert_eq!(stale_posting_secs_from_env(), 600);
        std::env::set_var(TRANSACTION_POSTING_STALE_AFTER_SECS, "-5");
        assert_eq!(stale_posting_secs_from_env(), 600);
        std::env::remove_var(TRANSACTION_POSTING_STALE_AFTER_SECS);
    }
    #[test]
    fn stale_secs_non_numeric_falls_back() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var(TRANSACTION_POSTING_STALE_AFTER_SECS, NOT_A_NUMBER);
        assert_eq!(stale_posting_secs_from_env(), 600);
        std::env::remove_var(TRANSACTION_POSTING_STALE_AFTER_SECS);
    }
}
