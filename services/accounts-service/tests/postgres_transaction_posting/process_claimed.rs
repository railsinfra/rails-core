use accounts_api::ledger_grpc::LedgerGrpc;
use accounts_api::models::{AccountType, TransactionStatus};
use accounts_api::repositories::{AccountRepository, TransactionRepository};
use accounts_api::services::transaction_retry::process_claimed_ledger_posts;
use chrono::Duration;
use uuid::Uuid;

use crate::support::{
    allow_null_transaction_environment, claim_one_for_processing, hours_ago,
    insert_pending_deposit, insert_pending_tx_kind, insert_posting_deposit_null_environment,
    migrated_pool, serve_ledger_capture, serve_ledger_ok,
};

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

    let tx = claim_one_for_processing(&pool, None).await;

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

    let tx = claim_one_for_processing(&pool, None).await;

    let ledger = LedgerGrpc::new("http://127.0.0.1:9".to_string());
    process_claimed_ledger_posts(&pool, &ledger, vec![tx]).await;

    let back = TransactionRepository::find_by_id(&pool, id).await.unwrap();
    assert_eq!(back.status, TransactionStatus::Pending);
    assert!(back.failure_reason.unwrap_or_default().len() > 0);
}

#[tokio::test]
async fn process_claimed_legacy_null_environment_releases_when_accounts_missing() {
    let (_c, pool) = migrated_pool().await;
    allow_null_transaction_environment(&pool).await;

    let org = Uuid::new_v4();
    let id = Uuid::new_v4();
    let acc = Uuid::new_v4();
    let idem = format!("idem-null-{}", Uuid::new_v4());
    insert_posting_deposit_null_environment(&pool, id, org, acc, &idem, hours_ago(2)).await;

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

    let tx = claim_one_for_processing(&pool, None).await;

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

    let tx = claim_one_for_processing(&pool, None).await;

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
async fn process_claimed_null_environment_falls_back_to_production_account() {
    let (_c, pool) = migrated_pool().await;
    allow_null_transaction_environment(&pool).await;

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
    insert_posting_deposit_null_environment(&pool, id, org, acc.id, &idem, hours_ago(2)).await;

    let tx = TransactionRepository::find_by_id(&pool, id).await.unwrap();
    let url = serve_ledger_ok().await;
    let ledger = LedgerGrpc::new(url);
    process_claimed_ledger_posts(&pool, &ledger, vec![tx]).await;

    let done = TransactionRepository::find_by_id(&pool, id).await.unwrap();
    assert_eq!(done.status, TransactionStatus::Posted);
}
