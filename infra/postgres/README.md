# Postgres

Rails-core assumes **externally provisioned** PostgreSQL (development: Neon or any hosted instance). Compose does **not** start a local Postgres container.

Each service receives its own connection string via `USERS_DATABASE_URL`, `ACCOUNTS_DATABASE_URL`, and `LEDGER_DATABASE_URL` in `.env` at the repository root (see `.env.example`).
