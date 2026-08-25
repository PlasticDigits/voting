# Ledger invariants

Cross-links: [ARCHITECTURE.md](ARCHITECTURE.md) · [OPERATOR_VOTING.md](OPERATOR_VOTING.md) · issues [#1](https://gitlab.com/PlasticDigits/voting/-/issues/1) · [#4](https://gitlab.com/PlasticDigits/voting/-/issues/4) · skill [AGENTS_VOTING_BUNDLE.md](../skills/AGENTS_VOTING_BUNDLE.md)

This crate is `voting-ledger` (`ledger/`). It is **not** the DEX indexer. Do not implement these tables in `cl8y-dex-terraclassic`.

## L1 — No archive node

Registration stores the **live** CW20 `Balance` / BEP-20 `balanceOf` at the current LCD height / `eth_blockNumber`. There is no `x-cosmos-block-height` header and no historical `eth_getLogs` backfill.

## L2 — Registered-set only

CW20 wasm events and BEP-20 Transfer logs are stored only when `from` or `to` is already registered. Unregistered counterparties appear as the other leg of a registered transfer; they are **not** voters. Later registration is a new live snapshot (**no backfill**).

## L3 — Emitter trust (DEX #285)

Only wasmd-stamped `_contract_address` scopes a CW20 event. A forgeable `contract_address` attribute is ignored. Only the configured `CL8Y_TOKEN_ADDRESS` is indexed.

## L4 — Pinned BEP-20

Only `BSC_CL8Y_TOKEN_ADDRESS` (default `0x8F452a1fdd388A45e1080992eFF051b4dd9048d2`). Lookalike Transfer logs are dropped. Empty `BSC_RPC_URLS` skips live BSC (CI). Production (`RUN_MODE=prod`) **requires** `BSC_RPC_URLS`. Never log RPC URLs.

## L5 — 18-decimal integers

Amounts are unsigned integer strings / `NUMERIC(78,0)`. No floats.

## L6 — Inclusive height / block

A transfer at height/block `H` is included in `voting_cl8y_balance_at(wallet, H)` / `voting_bsc_cl8y_balance_at(wallet, H)`. Query at `H < registered_at_height` returns 0.

## L7 — Reorg unwind

Terra: compare stored `last_indexed_block_hash` to the LCD hash; on mismatch delete transfers/checkpoints `height > fork-1` and reset the cursor. BSC: operator rewind via `rewind_bsc` (delete `bsc_block > N`). Do not share tables with DEX Venus.

## L8 — Identity

`terra` and `bsc` registrations are distinct primary keys. Do not merge `terra1…` and `0x…`.

## L9 — Flash-in is counted

A large transfer into a registered wallet immediately before a proposal snapshot **is** vote power. Documented, not a bug, unless product adds lockup later.

## L10 — Least privilege

Balance functions are `SECURITY DEFINER`. `operator_voting` gets `EXECUTE` on the functions plus `SELECT` on `voting_registrations` / `indexer_state` and R/W on schema `voting`. It must not `INSERT` ledger transfer/checkpoint tables. See [../deploy/grants.sql](../deploy/grants.sql).

## L11 — Registration handoff

`operator-voting` inserts `voting.registration_intents`. The ledger poller queries live balances and writes `voting_registrations` (idempotent: no double initial credit).

## Env

| Variable | Role |
|----------|------|
| `DATABASE_URL` | Writer role |
| `CL8Y_TOKEN_ADDRESS` | Terra CW20 |
| `BSC_CL8Y_TOKEN_ADDRESS` | BSC BEP-20 |
| `TERRA_LCD_URL` / `LCD_URLS` | Live LCD (comma-separated) |
| `BSC_RPC_URLS` | Live BSC (comma-separated; never log) |
| `RUN_MODE` | `prod` fail-closed |
| `API_BIND` | Health listener (default `0.0.0.0:3001`) |
