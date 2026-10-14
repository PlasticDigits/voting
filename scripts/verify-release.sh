#!/usr/bin/env bash
# Disposable Postgres only: never accepts or inherits an application DB URL.
set -euo pipefail
cd "$(dirname "$0")/.."
unset DATABASE_URL LEDGER_TEST_DATABASE_URL
export LEDGER_TEST_ALLOW_DESTRUCTIVE=1 LEDGER_TEST_REQUIRED=1
test_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')
test_database="voting_test_release_$(date +%s)_${RANDOM}"
test_container=""
cleanup() {
  if [[ -n "$test_container" ]]; then docker rm -f "$test_container" >/dev/null; fi
}
trap cleanup EXIT
test_container=$(docker run -d --network host \
  -e POSTGRES_USER=voting -e POSTGRES_PASSWORD=voting -e POSTGRES_DB="$test_database" \
  postgres:16-alpine postgres -c "port=$test_port" -c listen_addresses=127.0.0.1)
for attempt in $(seq 1 30); do
  if docker exec "$test_container" pg_isready -U voting -d "$test_database" -p "$test_port" >/dev/null; then break; fi
  sleep 1
done
docker exec "$test_container" pg_isready -U voting -d "$test_database" -p "$test_port"
export LEDGER_TEST_DATABASE_URL="postgres://voting:voting@127.0.0.1:${test_port}/${test_database}"
cargo fmt --all -- --check
cargo test --workspace --lib --bins --locked
cargo test --workspace --tests --locked --jobs 1 -- --test-threads=1
(cd frontend && npm ci && npm test && \
  VITE_OPERATOR_VOTING_URL=https://operator.vote.cl8y.com \
  VITE_NETWORK=mainnet VITE_WC_PROJECT_ID=00000000000000000000000000000000 npm run build)
sh deploy/docker/test-frontend-spa-fallback.sh
gitleaks detect --source . --config .gitleaks.toml --redact --no-banner
