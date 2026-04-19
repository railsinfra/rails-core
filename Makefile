# Rails-core standalone repository Makefile.
.PHONY: help dev bootstrap verify health test reset reset-env reset-neon seed

RAILS_CORE := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))
REPO_ROOT := $(abspath $(RAILS_CORE))

help:
	@echo "rails-core (repo root: $(REPO_ROOT))"
	@echo "  make dev        — bootstrap + Docker Compose: nginx :8080 + all services"
	@echo "  make bootstrap  — layout check + Neon/.env (see .env.example for NEON_API_KEY)"
	@echo "  make verify     — assert service directories exist"
	@echo "  make health     — HTTP checks via gateway :8080 (compose must be up)"
	@echo "  make test       — health + cross-service contract tests (stack must be up)"
	@echo "  make reset      — docker compose down"
	@echo "  make reset-env  — clear DB URLs in .env (local only; keeps other keys)"
	@echo "  make reset-neon — delete Neon project from .env (requires CONFIRM_PURGE_NEON=yes)"
	@echo "  make seed       — placeholder (see script)"

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

reset-env:
	@bash "$(RAILS_CORE)scripts/reset.sh" --clear-env

reset-neon:
	@bash "$(RAILS_CORE)scripts/reset.sh" --purge-neon

dev: bootstrap
	@test -f "$(RAILS_CORE).env" || (echo "Missing $(RAILS_CORE).env after bootstrap — see scripts/bootstrap.sh output." && exit 1)
	@echo ""
	@echo "Starting Docker Compose (first run may compile Rust/Ruby for several minutes)."
	@echo "When containers are healthy, use the gateway on port 8080:"
	@python3 "$(RAILS_CORE)scripts/lib/print_gateway_hints.py"
	@echo ""
	cd "$(RAILS_CORE)" && docker compose --env-file .env up --build
