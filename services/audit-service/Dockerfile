# syntax=docker/dockerfile:1
#
# Build context: **repository root** (`rails-core`), same contract as
# `services/accounts-service/Dockerfile` and CI `docker-builds`.
#
#   docker build -f services/audit-service/Dockerfile .

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

COPY --from=builder /build/services/audit-service/target/release/audit_service /usr/local/bin/audit_service

ENV SERVER_ADDR=0.0.0.0:8080

EXPOSE 8080 50054

CMD ["/usr/local/bin/audit_service"]
