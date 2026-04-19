# Quickstart (< 10 minutes)

## 1. Clone

```bash
git clone git@github.com:railsinfra/rails-core.git
cd rails-core
make bootstrap
```

(`bootstrap` checks that every path in `config/services.json` exists — services are **vendored** in this repo, not submodules.)

## 2. Environment

```bash
cp .env.example .env
# Edit .env — set USERS_DATABASE_URL, ACCOUNTS_DATABASE_URL, LEDGER_DATABASE_URL
# (Hosted Postgres / Neon URLs are fine; no local Postgres container in compose.)
```

## 3. Run everything

```bash
make dev
```

Wait for first-time `cargo`/`bundle` downloads. Then open:

- **Gateway:** [http://localhost:8080/](http://localhost:8080/) (redirects to `/docs/`)
- **Static docs:** [http://localhost:8080/docs/](http://localhost:8080/docs/)

## 4. Mental model

| Path on :8080 | Service |
|----------------|---------|
| `/users/*` | users-service (Rust) |
| `/accounts/*` | accounts-service (Rust) |
| `/ledger/*` | ledger-service (Rails) |

Read [architecture.md](architecture.md) for the one-page diagram and boundaries.

## 5. Stop / reset containers

```bash
make reset
# or
./scripts/reset.sh
```

This stops Docker Compose; it does **not** drop external databases.

## 6. Optional consumers

Admin UI and other gateways can call this stack through **http://localhost:8080** during local development; they live in separate repositories (for example under the `railsinfra` org).
