# Rails core architecture

Lean, opinionated **financial infrastructure**: three domain services, one **nginx** gateway, explicit **proto contracts**, and **Docker Compose** for local-first onboarding.

## Single diagram

```
                    ┌─────────────────────────────────────────┐
                    │  Developer / browser / API client       │
                    └───────────────────┬─────────────────────┘
                                        │ HTTP :8080
                                        ▼
                    ┌─────────────────────────────────────────┐
                    │  gateway (nginx)                        │
                    │  /users/*  /accounts/*  /ledger/*  /docs │
                    └───┬─────────────┬─────────────┬─────────┘
                        │             │             │
           ┌────────────▼──┐   ┌──────▼──────┐   ┌─▼──────────────┐
           │ users-service   │   │ accounts-   │   │ ledger-service │
           │ (Rust)          │   │ service     │   │ (Rails)        │
           │ identity, auth  │   │ (Rust)      │   │ double-entry   │
           │ tenants         │   │ balances    │   │ finality       │
           └───┬─────────────┘   └───┬─────────┘   └───┬────────────┘
               │                     │                  │
               ▼                     ▼                  ▼
         ┌───────────┐         ┌───────────┐    ┌───────────┐
         │ Postgres  │         │ Postgres  │    │ Postgres  │
         │ (users DB)│         │(accounts) │    │ (ledger) │
         └───────────┘         └───────────┘    └───────────┘
         external / Neon      external       external
```

**Rule:** no shared database, no cross-import of internal code. Services talk over **HTTP** (through the gateway) and **gRPC/proto contracts** (`proto/*.proto`, v1) for machine-to-machine shapes.

## Service roles (strict)

| Service | Owns | Must not own |
|---------|------|----------------|
| **users-service** | Identity, user lifecycle, tenant context, auth edges | Account balances, ledger postings |
| **accounts-service** | Account records, balances, transfer orchestration toward ledger | Double-entry truth, user password tables |
| **ledger-service** | Immutable ledger, postings, financial finality | User profile, primary account ownership outside ledger domain |

## Request flow example

**User action → gateway → accounts → ledger → response**

1. Client calls `POST /accounts/...` on the gateway (`http://localhost:8080/accounts/...`).
2. **accounts-service** validates the business command, persists its bounded context, and (when a financial fact is final) invokes **ledger-service** via **contracted** HTTP or gRPC (shape from `proto/`), not ad-hoc JSON.
3. **ledger-service** applies double-entry rules, returns a definitive result.
4. Response is returned through the gateway path prefix.

(Exact routes depend on each service’s HTTP router; the gateway preserves prefixes consistently.)

## Why separate services?

- **Blast radius:** a bug in dashboard JSON parsing must not corrupt the ledger.
- **Data isolation:** each service has its own database URL; no cross-DB joins.
- **Team velocity:** contracts in `proto/` stabilize integration; internals stay private.
- **Onboarding:** one compose file, one `make dev`, one public port **8080**.

## Contract layer

| File | Package | Purpose |
|------|---------|---------|
| `proto/users.proto` | `rails.core.users.v1` | Users / identity RPC surface (v1) |
| `proto/accounts.proto` | `rails.core.accounts.v1` | Accounts RPC surface (v1) |
| `proto/ledger.proto` | `rails.core.ledger.v1` | Ledger RPC surface (v1) |

Add RPCs here **before** implementing cross-service calls. Version bump = v2 package or new `*.proto` file.

## Gateway

`gateway/nginx.conf` terminates developer traffic and forwards:

| Path prefix | Upstream |
|-------------|----------|
| `/users/` | `users-service:8080` |
| `/accounts/` | `accounts-service:8080` |
| `/ledger/` | `ledger-service:3000` |
| `/docs/` | Static files from `docs/` |

## Local development model

- **Single env file:** `.env` at the repository root (copy from `.env.example`).
- **No local Postgres in compose:** services read `*_DATABASE_URL` pointing at hosted Postgres.
- **One command:** `make dev` runs Compose: builds/runs Rust & Rails in dev images with bind mounts, starts nginx on **8080**.

See [quickstart.md](quickstart.md) for exact commands.
