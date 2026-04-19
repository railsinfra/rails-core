# Users Service - Local Development Setup

## Prerequisites

The users service requires the following services to be running:

1. **PostgreSQL** - Database for storing users, businesses, environments, and sessions

## Quick Start

### 1. Install Dependencies

#### PostgreSQL
```bash
# macOS
brew install postgresql@14
brew services start postgresql@14

# Or use Docker
docker run --name postgres-users -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=users -p 5432:5432 -d postgres:14
```

### 2. Set Up Database

Create the database and run migrations:

```bash
# Create database
createdb users

# Or with PostgreSQL running in Docker:
docker exec -i postgres-users psql -U postgres -c "CREATE DATABASE users;"

# Run migrations (using sqlx-cli)
cargo install sqlx-cli
export DATABASE_URL="postgresql://localhost:5432/users"
sqlx migrate run

# Or manually run the migration SQL
psql postgresql://localhost:5432/users < migrations/20260119000100_init_users_service.sql
```

### 3. Configure Environment

Create a `.env` file or set environment variables:

```bash
export DATABASE_URL="postgresql://localhost:5432/users"
export SERVER_ADDR="0.0.0.0:8080"
export RUST_LOG="info"

# API key hashing secret (required in production)
export API_KEY_HASH_SECRET="replace_me"

# Service-to-service protection (recommended)
# Only required for the sensitive endpoints:
# - POST /api/v1/auth/login
# - POST /api/v1/business/register
# Comma-separated list of allowed internal tokens.
export INTERNAL_SERVICE_TOKEN_ALLOWLIST="replace_me"
```

### 4. Build and Run

```bash
# Build the service
cargo build --release

# Run using the helper script
./run-local.sh

# Or run directly
cargo run --release
```

## Service Endpoints

Once running, the service will be available at `http://localhost:8080`:

- `POST /api/v1/business/register` - Register a new business
- `POST /api/v1/users` - Create a new user
- `POST /api/v1/auth/login` - Login and get tokens
- `POST /api/v1/auth/refresh` - Refresh access token
- `POST /api/v1/auth/revoke` - Revoke refresh token
- `POST /api/v1/api-keys` - Create a new API key (admin JWT only)
- `GET /api/v1/api-keys` - List API keys for the business (admin JWT only)
- `POST /api/v1/api-keys/:api_key_id/revoke` - Revoke an API key (admin JWT only)

## Request Requirements

### Correlation ID

All `/api/...` requests must include:

- `x-correlation-id: <string>`

This is used for request tracking and will be included in logs.

### Environment ID

Most authenticated endpoints require:

- `x-environment-id: <uuid>`

This is used to select the active environment (e.g. `sandbox` vs `production`).

### Authentication

Protected endpoints support either:

- `authorization: Bearer <jwt>` (dashboard/admin usage)
- `x-api-key: <api_key>` (SDK/server-to-server usage)

Notes:

- API keys are **business-owned** and treated as **admin-level** for the MVP.
- API keys are verified by hashing the provided key and matching it against the stored hash.
- The hash uses `API_KEY_HASH_SECRET` (env var; default `dev_api_key_hash_secret` if unset). Use the **same** secret when creating and validating keys (e.g. same `.env` or `.env.dev` when running the service).

### API key creation and one-time reveal

API keys are not created by default.

- `POST /api/v1/api-keys` creates a new API key and returns the plaintext key **exactly once**.
- The plaintext key is **not stored** in the database.
- If the key is lost, create a new key and revoke the old one.

Revocation:

- Revoked keys are not deleted.
- Revocation is implemented using `status='revoked'` and `revoked_at`.
- Revoked keys are blocked from future calls.

### Local API key flow test

There is a minimal script at the repo root that exercises the full flow:

- Business register
- Admin login
- API key creation (one-time reveal)
- Create user using `x-api-key`

Run:

```bash
bash test-api-key-flow.sh
```

### Sensitive endpoint protection (recommended)

The following endpoints are considered sensitive and can be restricted to trusted internal callers:

- `POST /api/v1/auth/login`
- `POST /api/v1/business/register`

If `INTERNAL_SERVICE_TOKEN_ALLOWLIST` is set, these endpoints require:

- `x-internal-service-token: <token>`

If the token is missing or not allowlisted, the service responds with `403` and a generic message.

This is designed for the intended production flow:

- Marketing/Client app (Next.js) -> `rails-client-server`
- `rails-client-server` -> `users-service` (adds internal token + correlation id)

## Troubleshooting

### Service hangs on startup
- Check that PostgreSQL is running: `pg_isready` or `psql -l`
- Check the logs with `RUST_LOG=debug`

### Database connection errors
- Verify `DATABASE_URL` is correct
- Ensure the database exists: `psql -l | grep users`
- Check PostgreSQL is listening: `lsof -i :5432`

### Port already in use
- Change `SERVER_ADDR` to a different port (e.g., `0.0.0.0:8081`)
- Or kill the process using port 8080: `lsof -ti:8080 | xargs kill`

## Development

### Running Tests
```bash
cargo test
```

### Running with Debug Logging
```bash
RUST_LOG=debug cargo run
```

### Checking Service Status
```bash
# Check if service is running
ps aux | grep users_service

# Check if port is listening
lsof -i :8080

# Test endpoint
curl http://localhost:8080/api/v1/business/register \
  -H "x-correlation-id: local-test-1" \
  -H "x-internal-service-token: replace_me" \
  -H "Content-Type: application/json" \
  -d '{"name":"Test Business","admin_email":"admin@test.com","admin_password":"password123"}'
```
