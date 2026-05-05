# Audit Microservice

High-performance ingest service for an append-only audit trail: other platform services call a unary gRPC API to record one structured event per request; rows are validated, persisted to a dedicated Postgres database, and protected from updates and deletes at the database layer.

## Features

1. **Append-only ingest**
   - Unary `AppendAuditEvent` RPC (exactly one event per request in v1)
   - Catalog of allowed actions and metadata keys; rejects invalid payloads before write

2. **Operational endpoints**
   - HTTP health check for orchestration and load balancers
   - gRPC server on a configurable port (default `50054`)

3. **Durable storage**
   - PostgreSQL with SQLx migrations
   - Rules on `audit_events` block `UPDATE` and `DELETE` (append-only table semantics)

## Technology Stack

- **Rust** — Async services and strong typing for ingest paths
- **Axum** — HTTP server (health)
- **Tonic** — gRPC server and protobuf codegen
- **SQLx** — Async Postgres access and migrations
- **PostgreSQL** (e.g. Neon) — Dedicated audit database, separate from domain DBs

## Project Structure

```
audit-service/
├── .env.example            # Copy to .env for local config
├── src/
│   ├── main.rs              # Binary entry point
│   ├── lib.rs               # Library surface (tests, embedding)
│   ├── bootstrap.rs         # HTTP + gRPC startup
│   ├── config.rs            # Environment configuration
│   ├── db.rs                # Insert path for audit rows
│   ├── grpc_server.rs       # AuditService gRPC implementation
│   ├── proto.rs             # Generated protobuf types
│   ├── validate.rs          # Event validation (catalog, PII-safe metadata)
│   └── routes/              # HTTP routes (health)
├── migrations/              # SQL migrations (audit_events)
├── tests/                   # Integration tests (e.g. Postgres + gRPC E2E)
├── Dockerfile               # Docker / Railway (repo root context; see CI docker-builds)
├── railway.toml             # Railway config-as-code (optional; path from repo root in monorepo UI)
├── build.rs                 # Protobuf build (shared repo proto)
├── Cargo.toml
└── README.md                # This file
```

Proto definitions live at repo root: `proto/audit/v1/audit.proto`.

## Setup

### Prerequisites

- Rust (latest stable)
- PostgreSQL instance for the audit database (e.g. Neon **`audit-db`** created by repo `make bootstrap` when `NEON_API_KEY` is set, or any other Postgres you point `AUDIT_DATABASE_URL` at)
- `protoc` (for local builds from clean checkout; see `build.rs`)

### Environment Variables

From this directory, copy the example file and adjust values (or set variables in your environment without a `.env` file):

```bash
cp .env.example .env
```

`dotenv` loads `.env` from the current working directory when you run `cargo run` from `audit-service/`. For variable semantics, see comments in [`.env.example`](./.env.example) and `../../docs/audit-trail-architecture.md` for the audit database.

### Database Setup

1. Install SQLx CLI (optional): `cargo install sqlx-cli`
2. Ensure `AUDIT_DATABASE_URL` points at an empty or migrated database
3. From this directory, run migrations: `sqlx migrate run`  
   (Migrations live in `migrations/`; the service also applies them on startup with drift tolerance for shared environments.)

### Running the Service

```bash
cargo run
```

The process listens on `SERVER_ADDR` for HTTP and on `0.0.0.0:GRPC_PORT` for gRPC.

## API Documentation

- **Architecture and data model:** `../../docs/audit-trail-architecture.md` (repository root)
- **gRPC:** `rails.core.audit.v1.AuditService` / `AppendAuditEvent` — see `proto/audit/v1/audit.proto`

## Performance Considerations

- Connection pooling to the audit database
- Unary RPC per event keeps ingest simple and back-pressure friendly
- Async I/O for HTTP, gRPC, and database writes

## Development

### Running Tests

```bash
cargo test
```

End-to-end tests that start Postgres via **Testcontainers** are marked `#[ignore]` so local runs do not require Docker. To run them (Docker must be available):

```bash
cargo test --locked -- --include-ignored
```

### Code Formatting

```bash
cargo fmt
```

### Linting

```bash
cargo clippy --all-targets
```

## Database Migrations

- Migrations are in the `migrations/` directory
- Run manually with: `sqlx migrate run`
- Add a new migration with: `sqlx migrate add <migration_name>`
