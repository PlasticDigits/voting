# voting-ledger

Dual-chain CL8Y registration ledger. Invariants: [`../docs/LEDGER_INVARIANTS.md`](../docs/LEDGER_INVARIANTS.md). Coolify writer image: [`../deploy/docker/ledger.Dockerfile`](../deploy/docker/ledger.Dockerfile). Restricted-role grants after migrate: [`../deploy/grants.sql`](../deploy/grants.sql). Ops: [`../docs/OPS.md`](../docs/OPS.md).

```bash
DATABASE_URL=postgres://… cargo run -p voting-ledger
cargo test -p voting-ledger --lib
LEDGER_TEST_DATABASE_URL=postgres://voting:voting@127.0.0.1:5433/voting_test_local LEDGER_TEST_ALLOW_DESTRUCTIVE=1 cargo test -p voting-ledger --tests -- --test-threads=1
```

Use an explicitly disposable test database marked `COMMENT ON DATABASE voting_test_local IS 'cl8y-voting-ephemeral-test'`. Never use a production `DATABASE_URL` for destructive tests.

For production gateways without smart-query observation headers, configure native `TERRA_RPC_URLS`. Balance/recovery queries bind the ABCI body height to a current recent block/hash on the same verified Comet endpoint. A read-only probe is available with `cargo run -p voting-ledger --example probe_terra_rpc` under a URL environment injected by `bao-exec`.

Terra and BSC run independently. `/live` reports process liveness; `/ready` and `/health` require fresh canonical complete dual-chain accounting and available intent SQL. Production requires both provider sets. Historical gaps never jump a nonzero cursor to tip.

A controlled recovery can run `voting-ledger --reconcile-live-baseline` with `bao-exec` injecting the writer/provider environment. For a one-time Coolify upgrade, explicitly set `LEDGER_RECONCILE_LIVE_BASELINE=true`, stop the old ledger/API, then deploy. Startup recovers missing/degraded existing electorates and skips already committed healthy epochs after restart. Remove the flag after verifying readiness. The new baseline preserves historical checkpoints and frozen advisory votes. See [L13–L17](../docs/LEDGER_INVARIANTS.md) and [agent maintenance guidance](../skills/AGENTS_LEDGER_HARDENING.md).

Does **not** reuse the DEX Venus vFDUSD poller. Empty `BSC_RPC_URLS` leaves dual-chain readiness unavailable.
