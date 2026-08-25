# voting-ledger

Dual-chain CL8Y registration ledger. Invariants: [`../docs/LEDGER_INVARIANTS.md`](../docs/LEDGER_INVARIANTS.md).

```bash
DATABASE_URL=postgres://… cargo run -p voting-ledger
cargo test -p voting-ledger --lib
LEDGER_TEST_DATABASE_URL=postgres://voting:voting@127.0.0.1:5433/voting cargo test -p voting-ledger --tests
```

Does **not** reuse the DEX Venus vFDUSD poller. Empty `BSC_RPC_URLS` skips live BSC.
