# operator-voting (restricted role). Build context: repository root.
#   docker build -f deploy/docker/operator-voting.Dockerfile -t operator-voting .
#
# DATABASE_URL must be postgres://operator_voting:... (deploy/grants.sql).
# Do not point this image at the ledger writer. APPLY_MIGRATIONS is pinned
# false; RUN_MODE=prod also refuses true. See docs/OPS.md and docs/OPERATOR_VOTING.md.

FROM rust:1.88-bookworm AS builder

WORKDIR /build
RUN apt-get update \
  && apt-get install -y --no-install-recommends pkg-config libssl-dev \
  && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY ledger/ ./ledger/
COPY operator-voting/ ./operator-voting/
RUN cargo build --release -p operator-voting \
  && strip target/release/operator-voting

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates libssl3 curl \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --system --uid 10002 --create-home voting

COPY --from=builder /build/target/release/operator-voting /usr/local/bin/operator-voting

USER voting
WORKDIR /app

# One ENV per line. Coolify's build-secret injector treats a continued
# line starting with RUN_MODE as a Dockerfile RUN instruction.
ENV API_BIND=0.0.0.0:3002
ENV RUN_MODE=prod
ENV APPLY_MIGRATIONS=false
ENV RUST_LOG=info

EXPOSE 3002

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD curl -fsS http://127.0.0.1:3002/health >/dev/null || exit 1

CMD ["operator-voting"]
