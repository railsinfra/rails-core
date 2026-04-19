# Quickstart (< 10 minutes)

## 1. Clone

```bash
git clone git@github.com:railsinfra/rails-core.git
cd rails-core
```

`make dev` runs the same layout check as `make bootstrap` (paths in `config/services.json` — services are **vendored** in this repo, not git submodules), brings Compose up in the background (`docker compose up -d --build`), then blocks until health checks pass. Use `make logs` to stream container logs.

## 2. Environment

```bash
cp .env.example .env
```

Set `NEON_API_KEY` in `.env` ([Neon API keys](https://neon.tech/docs/manage/api-keys)) so bootstrap can fill the database URLs, **or** set `USERS_DATABASE_URL`, `ACCOUNTS_DATABASE_URL`, and `LEDGER_DATABASE_URL` manually. Optional Neon-related keys are listed in `.env.example`. Console deep links in the printed table are shortened with Bitly if configured, otherwise **is.gd** (stdlib HTTP); use `NEON_CONSOLE_NO_PUBLIC_SHORTENER=yes` to skip is.gd.

## 3. Run everything

```bash
make dev
```

Wait for first-time `cargo`/`bundle` (and the post-start health wait). Then open:

- **Gateway:** [http://localhost:8080/](http://localhost:8080/) (redirects to `/docs/`)
- **Static docs:** [http://localhost:8080/docs/](http://localhost:8080/docs/)

## 4. Mental model

| Path on :8080 | Service |
|----------------|---------|
| `/users/*` | users-service (Rust) |
| `/accounts/*` | accounts-service (Rust) |
| `/ledger/*` | ledger-service (Rails) |

Read [architecture.md](architecture.md) for the one-page diagram and boundaries.

## 5. Health and contract tests (optional)

With the stack still running:

```bash
make health   # gateway: /health, /users/health, /accounts/health, /ledger/health + /docs/
make test     # health JSON checks + full users → accounts → ledger HTTP flow
```

`make test` expects the gateway at `http://127.0.0.1:8080` unless you set `GATEWAY_URL`.

## 6. Logs, stop / reset containers

```bash
make logs     # follow all service logs (requires .env)
make reset    # or: make stop
./scripts/reset.sh
```

This stops Docker Compose; it does **not** drop external databases.

## 7. Optional consumers

Admin UI and other gateways can call this stack through **http://localhost:8080** during local development; they live in separate repositories (for example under the `railsinfra` org).
