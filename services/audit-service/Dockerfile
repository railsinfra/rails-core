# Build context must be the **repository root** (see `.github/workflows/ci.yml` docker-builds).
# `build.rs` resolves protos from `../../proto` relative to `services/audit-service`.

FROM rust:1.92-slim-bookworm AS builder

WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config=1.8.1-1 \
    libssl-dev=3.0.19-1~deb12u2 \
    protobuf-compiler=3.21.12-3 \
    && rm -rf /var/lib/apt/lists/*

COPY proto /build/proto
COPY services/audit-service /build/services/audit-service

WORKDIR /build/services/audit-service

RUN cargo build --locked --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates=20230311+deb12u1 \
    libssl3=3.0.19-1~deb12u2 \
    curl=7.88.1-10+deb12u14 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /build/services/audit-service/target/release/audit_service /app/audit_service
COPY --from=builder /build/services/audit-service/migrations /app/migrations

EXPOSE 8080 50054

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

CMD ["/app/audit_service"]
