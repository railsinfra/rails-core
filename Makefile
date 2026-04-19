# Rails-core standalone repository Makefile.
.PHONY: help dev bootstrap verify health reset seed

RAILS_CORE := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))
REPO_ROOT := $(abspath $(RAILS_CORE))

help:
	@echo "rails-core (repo root: $(REPO_ROOT))"
	@echo "  make dev        — Docker Compose: nginx :8080 + all services"
	@echo "  make bootstrap  — verify vendored service directories exist"
	@echo "  make verify     — assert service directories exist"
	@echo "  make health     — HTTP checks via gateway :8080 (compose must be up)"
	@echo "  make reset      — docker compose down"
	@echo "  make seed       — placeholder (see script)"

bootstrap:
	@bash "$(RAILS_CORE)scripts/bootstrap.sh"

verify:
	@bash "$(RAILS_CORE)scripts/verify-layout.sh"

health:
	@bash "$(RAILS_CORE)scripts/health-check.sh"

seed:
	@bash "$(RAILS_CORE)scripts/seed.sh"

reset:
	@bash "$(RAILS_CORE)scripts/reset.sh"

dev:
	@test -f "$(RAILS_CORE).env" || (echo "Missing $(RAILS_CORE).env — run: cp .env.example .env" && exit 1)
	cd "$(RAILS_CORE)" && docker compose --env-file .env up --build
