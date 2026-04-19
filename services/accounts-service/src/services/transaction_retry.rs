use chrono::Duration;
use sqlx::PgPool;
use tracing::{info, warn};

use crate::ledger_grpc::LedgerGrpc;
use crate::models::{TransactionKind, TransactionStatus};
use crate::repositories::{AccountRepository, TransactionRepository};

/// Best-effort background retry loop that posts pending transactions to the Ledger via gRPC.
/// Eventual consistency: transactions remain pending until Ledger accepts them.
pub async fn run(pool: PgPool, ledger_grpc: LedgerGrpc) {
    info!("Ledger gRPC retry worker started");

    loop {
        // Retry pending transactions older than a short delay to avoid racing immediate posts.
        // Don't filter by environment - process both new (with environment) and legacy (NULL) transactions
        let pending = match TransactionRepository::find_pending_older_than_any_org(
            &pool,
            Duration::seconds(2),
            Some(200),
            None, // Don't filter by environment - process all pending transactions
        )
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                warn!(error = %e, "retry_worker_failed_to_load_pending");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        for tx in pending {
            // Use environment from transaction if available, otherwise fallback to account lookup
            let environment = if let Some(ref env) = tx.environment {
                env.clone()
            } else {
                // Legacy transaction: determine environment from account record
                // Try sandbox first, then production if not found
                let account = match AccountRepository::find_by_id(&pool, tx.from_account_id, "sandbox").await {
                    Ok(a) => a,
                    Err(_) => {
                        // Try production if sandbox fails
                        match AccountRepository::find_by_id(&pool, tx.from_account_id, "production").await {
                            Ok(a) => a,
                            Err(e) => {
                                warn!(
                                    transaction_id = %tx.id,
                                    error = %e,
                                    "retry_worker_missing_account; leaving pending"
                                );
                                continue;
                            }
                        }
                    }
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
                    let _ = TransactionRepository::update_status(
                        &pool,
                        tx.id,
                        TransactionStatus::Posted,
                        None,
                    )
                    .await;
                }
                Err(e) => {
                    let reason = format!("{}", e);
                    // Ledger returns success on idempotent retry (existing row completed).
                    // Only treat as Posted when Ledger explicitly returns success (Ok(()) above).
                    // All errors: leave Pending so next retry can attempt again.
                    let _ = TransactionRepository::update_status(
                        &pool,
                        tx.id,
                        TransactionStatus::Pending,
                        Some(&reason),
                    )
                    .await;
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

