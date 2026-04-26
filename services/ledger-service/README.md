# README

This README would normally document whatever steps are necessary to get the
application up and running.

Things you may want to cover:

* Ruby version

* System dependencies

* Configuration

* Database creation

* Database initialization

* How to run the test suite

* Services (job queues, cache servers, search engines, etc.)

* Deployment instructions

* ...

## gRPC and protocol buffers

Ledger’s own API types are generated from `proto/ledger.proto` into `lib/grpc/` (see `config/initializers/grpc.rb`).

**Audit trail (shared `rails.core.audit.v1`)** — Ruby stubs for append-only audit ingest live under `lib/grpc/audit/v1/`. Regenerate after changing `proto/audit/v1/audit.proto` (from the **ledger-service** directory, with `grpc-tools` installed, e.g. `gem install grpc-tools`):

```bash
grpc_tools_ruby_protoc \
  -I ../../proto \
  --ruby_out=lib/grpc \
  --grpc_out=lib/grpc \
  ../../proto/audit/v1/audit.proto
```

The generated `audit_services_pb.rb` uses `require 'audit/v1/audit_pb'` by default; this repo keeps **`require_relative 'audit_pb'`** in that file so loads work with `lib/grpc` on the load path. Re-apply that change if you regenerate and the require breaks.
