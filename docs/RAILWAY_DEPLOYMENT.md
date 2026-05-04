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

## GitHub Actions (this monorepo)

Railway deploy workflows must live under **`.github/workflows/`** at the **repository root** (GitHub does not run `services/*/.github/workflows/`).

| Workflow file | Trigger | GitHub environment | Behaviour |
|---------------|---------|--------------------|-----------|
| `deploy-railway-dev.yml` | push to `develop` | `development` | Per service: **Run Tests** → **Deploy to Railway Dev** (`railway up --service … --detach` from repo root), same shape as the old per-repo `deploy-dev.yml` files under `services/`. |
| `deploy-railway-staging.yml` | push to `staging` | `staging` | Same pattern for staging (`deploy-staging.yml` parity). |
| `deploy-railway-production.yml` | push to `main` | `production` | Same pattern for production (`deploy-production.yml` parity). |

YAML under `services/<name>/.github/workflows/` is a **reference** only; change behaviour by editing the **root** workflows above.

## Monorepo builds (avoid Railpack “could not determine how to build”)

`rails-core` is a **monorepo**: Dockerfiles live under `services/<name>/`, but Railway often clones the **whole repo** with an **empty “Root Directory”**. In that mode Railpack runs at the repo root, skips nested Dockerfiles, and fails.

Pick **one** approach per service:

### A) Service variable `RAILWAY_DOCKERFILE_PATH` (full repo checkout)

Set on each Railway service (Variables), paths relative to repo root:

| Service | `RAILWAY_DOCKERFILE_PATH` |
|---------|---------------------------|
| accounts-service | `services/accounts-service/Dockerfile` |
| users-service | `services/users-service/Dockerfile` |
| ledger-service | `services/ledger-service/Dockerfile` |
| audit-service | `services/audit-service/Dockerfile` |

Then set **Builder** to **Dockerfile** (disable Railpack auto-detect if the UI offers it). Build context remains the **repository root**, which matches these Dockerfiles.

**audit-service** uses the same **repo-root** Docker build as the other Rust services (`docker build -f services/audit-service/Dockerfile .` from the monorepo root). Set **Root Directory** empty (repo root) and point **Config as code** at `/services/audit-service/railway.toml` if needed.

### B) Root directory per service (isolated monorepo)

In Railway → service → **Settings → Root Directory**, set e.g. `services/<service>`. Railway then uses that folder as the build context; only use this if the service’s Dockerfile is written for that **narrow** context. The **ledger** Dockerfile in this repo expects the **monorepo root**—use **A** for ledger unless you maintain a separate crate-local Dockerfile.

Official reference: [Deploying an isolated monorepo](https://docs.railway.app/deployments/monorepo), [Dockerfiles](https://docs.railway.app/builds/dockerfiles).

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

