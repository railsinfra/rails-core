//! Postgres-backed tests for posting / claim paths (Docker via testcontainers).

mod support;

mod try_claim;
mod claim_batch;
mod process_claimed;
mod find_by_id;
mod update_status;
