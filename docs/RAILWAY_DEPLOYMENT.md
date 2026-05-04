# Railway Deployment Guide (gRPC-only)

This guide covers deploying the MVP services to Railway using **gRPC** for inter-service communication.

## Architecture Overview

- **users-service (Rust)**: Public HTTP API for user/auth flows. Calls accounts via gRPC.
- **accounts-service (Rust)**: HTTP API + gRPC server for account operations.
- **ledger (Rails)**: gRPC server for ledger posting (optional, depending on which flows you run).
- **PostgreSQL (Neon)**: Shared database.

## Prerequisites

- Railway CLI installed + authenticated
- Neon database connection strings ready

## Deploy

### 1) Deploy Accounts Service

From `services/accounts-service`:

- Deploy service
- Set variables:
  - `DATABASE_URL`
  - `PORT`
  - `GRPC_PORT`
  - `LEDGER_GRPC_URL` (ledger **gRPC** URL, e.g. `http://<ledger-grpc host>.railway.internal:9090` on private networking—not the Rails HTTP URL)
  - `HOST`
  - `RUST_LOG`

### 2) Deploy Users Service

From `services/users-service`:

- Deploy service
- Set variables:
  - `DATABASE_URL`
  - `SERVER_ADDR` (or `HOST` + `PORT` depending on your setup)
  - `RUST_LOG`
  - `ACCOUNTS_GRPC_URL` (point to accounts-service internal host + gRPC port)
  - `API_KEY_HASH_SECRET` (required for production)
  - `INTERNAL_SERVICE_TOKEN_ALLOWLIST` (recommended hardening)

### 3) (Optional) Deploy Ledger Service

From `services/ledger-service`:

- Deploy service
- Set variables:
  - `DATABASE_URL`
  - `GRPC_PORT` — defaults to **9090** in-app, matching **`ledger-grpc` `internalPort`** in `railway.toml`. Set only if you change that port.
  - `RAILS_ENV`
  - `LOG_LEVEL`

## Verification

- Users service boots and can reach Accounts gRPC (`ACCOUNTS_GRPC_URL`).
- Accounts service boots and logs gRPC startup on `GRPC_PORT`.

