# Contributing to rails-core

Thanks for helping improve the project. This document is intentionally short: get oriented, run checks locally, open a PR.

## Run locally

1. Clone the repository and `cd rails-core`.
2. `cp .env.example .env` and set the three `*_DATABASE_URL` values to real Postgres databases (one schema per service is fine if you know what you are doing; the default template assumes separate DB names).
3. `make dev` starts nginx on [http://localhost:8080](http://localhost:8080) and all three application containers.

See [README.md](README.md) for URLs, a copy-paste **curl** example, and what belongs in scope for this repo.

## How services are structured

- **users-service** — Rust (Axum). HTTP under `/users/` on the gateway; gRPC for internal callers.
- **accounts-service** — Rust (Axum). HTTP under `/accounts/`; gRPC to **users** (API key validation) and **ledger** (postings).
- **ledger-service** — Rails. HTTP under `/ledger/`; gRPC server for ledger operations.

Shared API contracts live under `proto/`. The gateway strips the path prefix (`/users/`, `/accounts/`, `/ledger/`) before forwarding.

## Add or change an HTTP endpoint

1. Implement the handler in the correct service (`services/<name>-service/`).
2. Register the route in that service’s router module (search for `Router::new` or `route(` in `src/` or Rails `config/routes.rb` for ledger).
3. If the route should be public via the monorepo gateway, confirm `gateway/nginx.conf` already proxies the right prefix (it usually does).
4. Add or update tests next to the service (`cargo test`, `rails test`, or request specs as established in that tree).

## Run tests

From each service directory, use the same commands CI uses:

- **users-service** and **accounts-service**: `cargo clippy`, `cargo test` (install `protobuf-compiler` on Linux if `tonic` build fails).
- **ledger-service**: `bundle install` then `bin/rails test` (or `bundle exec rails test`) with `RAILS_ENV=test` and a `DATABASE_URL` pointing at a test database.

From the repository root, `make verify` checks that vendored service folders exist.

## Branching (Gitflow)

We follow **Gitflow**-style branching:

- **`main`** — production-ready history; only merges from `release/*` or `hotfix/*`.
- **`develop`** — day-to-day integration; default base for new work.
- **`feature/*`** — branch from `develop` for new work (for example `feature/rai-6-open-source-readiness`). Open a PR back into `develop`.
- **`release/*`** — branch from `develop` when cutting a release; only release prep and fixes, then merge to `main` and back into `develop`.
- **`hotfix/*`** — branch from `main` for urgent production fixes, then merge to `main` and `develop`.

If your change is small and the team agrees on an exception, ask before opening a feature PR against `main`.

## Commit messages (Conventional Commits)

Use **[Conventional Commits](https://www.conventionalcommits.org/)** so history and changelogs stay readable.

Format:

```
<type>(<scope>): <subject>

- optional bullet (keep the body short; two bullets max)
```

- **Types** we use: `feat`, `fix`, `refactor`, `test`, `style`, `docs`, `chore`, `perf`, `ci`, `build`, `revert`.
- **Subject**: imperative mood, lowercase, no trailing period, about **72 characters** max.
- **Scope**: optional; name the area (for example `gateway`, `ledger-service`, `ci`).
- **Breaking changes**: add `!` after the type or scope, for example `feat(api)!: remove legacy transfer endpoint`.

Squash merges should preserve a conventional **subject line** on the main integration branch.

## Pull requests

- Branch from **`develop`** using the Gitflow rules above, unless you are on a release or hotfix line.
- Keep the change focused on one concern when possible.
- Describe **what** changed and **why** in the PR body (plain language is enough).
- Ensure CI is green before requesting review.

No RFC process or architecture committee: if something is unclear, open a draft PR or issue and we iterate in the thread.
