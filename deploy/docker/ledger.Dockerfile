# voting-ledger (writer role). Build context: repository root.
#   docker build -f deploy/docker/ledger.Dockerfile -t voting-ledger .
#
# Coolify: apply sqlx migrations on first boot, then run deploy/grants.sql
# as the DB owner. See docs/OPS.md.

FROM rust:1.85-bookworm AS builder

WORKDIR /build
RUN apt-get update \
  && apt-get install -y --no-install-recommends pkg-config libssl-dev \
  && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY ledger/ ./ledger/
COPY operator-voting/ ./operator-voting/
RUN cargo build --release -p voting-ledger \
  && strip target/release/voting-ledger

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates libssl3 curl \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --system --uid 10001 --create-home ledger

COPY --from=builder /build/target/release/voting-ledger /usr/local/bin/voting-ledger

USER ledger
WORKDIR /app

ENV API_BIND=0.0.0.0:3001 \
    RUN_MODE=prod \
    RUST_LOG=info

EXPOSE 3001

HEALTHCHECK --interval=30s --timeout=5s --start-period=40s --retries=3 \
  CMD curl -fsS http://127.0.0.1:3001/health >/dev/null || exit 1

CMD ["voting-ledger"]
