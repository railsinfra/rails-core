//! Postgres-backed tests for posting / claim paths (Docker via testcontainers).

mod support;

mod claim_batch;
mod find_by_id;
mod process_claimed;
mod try_claim;
mod update_status;
