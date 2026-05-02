# Rails-core standalone repository Makefile.
.PHONY: help dev bootstrap verify health test reset stop logs reset-env reset-neon seed k6-smoke k6-smore

RAILS_CORE := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))
REPO_ROOT := $(abspath $(RAILS_CORE))

help:
	@echo "rails-core (repo root: $(REPO_ROOT))"
	@echo "  make dev        — bootstrap + compose up -d --build + wait for /health (then prints URLs)"
	@echo "  make bootstrap  — layout check + Neon/.env (see .env.example for NEON_API_KEY)"
	@echo "  make verify     — assert service directories exist"
	@echo "  make health     — HTTP checks via gateway :8080 (compose must be up)"
	@echo "  make test       — health + cross-service contract tests (stack must be up)"
	@echo "  make logs       — docker compose logs -f (stack must be up; requires .env)"
	@echo "  make reset      — docker compose down"
	@echo "  make stop       — same as make reset"
	@echo "  make reset-env  — clear DB URLs in .env (local only; keeps other keys)"
	@echo "  make reset-neon — delete Neon project from .env (requires CONFIRM_PURGE_NEON=yes)"
	@echo "  make seed       — placeholder (see script)"
	@echo "  make k6-smoke   — k6 smoke (native k6 or Docker grafana/k6); see scripts/k6/README.md"
	@echo "  make k6-smore   — alias for k6-smoke"

bootstrap:
	@bash "$(RAILS_CORE)scripts/bootstrap.sh"

verify:
	@bash "$(RAILS_CORE)scripts/verify-layout.sh"

health:
	@bash "$(RAILS_CORE)scripts/health-check.sh"

test: verify
	@bash "$(RAILS_CORE)scripts/system-test.sh"

seed:
	@bash "$(RAILS_CORE)scripts/seed.sh"

reset:
	@bash "$(RAILS_CORE)scripts/reset.sh"

stop: reset

logs:
	@test -f "$(RAILS_CORE).env" || (echo "Missing $(RAILS_CORE).env — run make bootstrap first." && exit 1)
	@cd "$(REPO_ROOT)" && docker compose --env-file .env logs -f

reset-env:
	@bash "$(RAILS_CORE)scripts/reset.sh" --clear-env

reset-neon:
	@bash "$(RAILS_CORE)scripts/reset.sh" --purge-neon

k6-smoke: verify
	@bash "$(RAILS_CORE)scripts/k6/run-smoke.sh"

k6-smore: k6-smoke

dev: bootstrap
	@test -f "$(RAILS_CORE).env" || (echo "Missing $(RAILS_CORE).env after bootstrap — see scripts/bootstrap.sh output." && exit 1)
	@echo ""
	@echo "Starting Docker Compose in the background (first run may compile Rust/Ruby for many minutes)."
	@echo "Containers will restart on failure; gateway starts after app healthchecks pass."
	@cd "$(REPO_ROOT)" && docker compose --env-file .env up -d --build
	@RAILS_CORE_ROOT="$(REPO_ROOT)" python3 "$(RAILS_CORE)scripts/lib/wait_for_stack.py"
