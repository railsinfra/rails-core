use chrono::Duration;
use sqlx::PgPool;
use std::time::Duration as StdDuration;
use tracing::info;

use crate::repositories::TransactionRepository;

fn reconcile_interval_secs_from_env() -> u64 {
    const OUTBOUND_RECONCILE_INTERVAL_SECS_ENV: &str = "OUTBOUND_RECONCILE_INTERVAL_SECS";
    std::env::var(OUTBOUND_RECONCILE_INTERVAL_SECS_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(60)
}

fn stale_posting_secs_from_env() -> i64 {
    const TRANSACTION_POSTING_STALE_AFTER_SECS_ENV: &str = "TRANSACTION_POSTING_STALE_AFTER_SECS";
    std::env::var(TRANSACTION_POSTING_STALE_AFTER_SECS_ENV)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(600)
}

fn max_pending_age_secs_from_env() -> i64 {
    const OUTBOUND_MAX_PENDING_AGE_SECS_ENV: &str = "OUTBOUND_MAX_PENDING_AGE_SECS";
    std::env::var(OUTBOUND_MAX_PENDING_AGE_SECS_ENV)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(3600)
}

pub async fn run(pool: PgPool) {
    let tick = StdDuration::from_secs(reconcile_interval_secs_from_env());
    let stale = Duration::seconds(stale_posting_secs_from_env());
    let max_pending = Duration::seconds(max_pending_age_secs_from_env());

    info!("Transaction reconciliation worker started");
    loop {
        match TransactionRepository::reconcile_stale_transactions(&pool, stale, max_pending).await {
            Ok((requeued, failed)) => {
                if requeued > 0 || failed > 0 {
                    info!(requeued, failed, "transaction_reconcile_sweep");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "transaction_reconcile_sweep_failed");
            }
        }
        tokio::time::sleep(tick).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        max_pending_age_secs_from_env, reconcile_interval_secs_from_env, stale_posting_secs_from_env,
    };
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn defaults_are_applied_when_env_is_absent() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("OUTBOUND_RECONCILE_INTERVAL_SECS");
        std::env::remove_var("TRANSACTION_POSTING_STALE_AFTER_SECS");
        std::env::remove_var("OUTBOUND_MAX_PENDING_AGE_SECS");
        assert_eq!(reconcile_interval_secs_from_env(), 60);
        assert_eq!(stale_posting_secs_from_env(), 600);
        assert_eq!(max_pending_age_secs_from_env(), 3600);
    }
}
