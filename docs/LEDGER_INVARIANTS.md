# Ledger invariants

Cross-links: [ARCHITECTURE.md](ARCHITECTURE.md) · [OPERATOR_VOTING.md](OPERATOR_VOTING.md) · [OPS.md](OPS.md) · issues [#1](https://gitlab.com/PlasticDigits/voting/-/issues/1) · [#4](https://gitlab.com/PlasticDigits/voting/-/issues/4) · [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) · [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) · skills [AGENTS_VOTING_BUNDLE.md](../skills/AGENTS_VOTING_BUNDLE.md) · [AGENTS_OPS_STAGING.md](../skills/AGENTS_OPS_STAGING.md)

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

The SQL function is unchanged. **Default** GET `/v1/balances` (no `height` query) in `operator-voting` evaluates at `max(indexer tip, registered_at_height)` so a just-registered wallet is not read as 0 while `last_indexed_*` is still `0` or behind the live snapshot ([#9](https://gitlab.com/PlasticDigits/voting/-/issues/9), **OV-B1** in [OPERATOR_VOTING.md](OPERATOR_VOTING.md)). Explicit `?height=` below register still returns 0.

## L7 — Reorg unwind

Terra: compare stored `last_indexed_block_hash` to the LCD hash; on mismatch delete transfers/checkpoints `height > fork-1` and reset the cursor. BSC: operator rewind via `rewind_bsc` (delete `bsc_block > N`). Do not share tables with DEX Venus.

## L8 — Identity

`terra` and `bsc` registrations are distinct primary keys. Do not merge `terra1…` and `0x…`.

## L9 — Flash-in is counted

A large transfer into a registered wallet immediately before a proposal snapshot **is** vote power. Documented, not a bug, unless product adds lockup later.

## L10 — Least privilege

Balance functions are `SECURITY DEFINER` in schema **`public`**. Always call / `GRANT EXECUTE` `public.voting_cl8y_balance_at` and `public.voting_bsc_cl8y_balance_at`. The writer role is often named `voting` and control-plane tables live in schema `voting`, so Postgres `search_path` `"$user", public` would otherwise create or grant a **shadow copy** in schema `voting`. `operator_voting` gets `EXECUTE` on the public functions plus `SELECT` on `voting_registrations` / `indexer_state` and R/W on **tables** in schema `voting` (`USAGE` only — no `CREATE`). It must not `INSERT` ledger transfer/checkpoint tables. See [../deploy/grants.sql](../deploy/grants.sql) (CONNECT follows `current_database()`). Coolify: ledger writer applies migrations; `operator-voting` must not (`APPLY_MIGRATIONS=false`; prod refuses true). Privilege tests apply that file with `psql -d`. [OPS.md](OPS.md) O1–O2.

## L11 — Registration handoff

`operator-voting` inserts `voting.registration_intents`. The ledger poller queries live balances and writes `voting_registrations` (idempotent: no double initial credit). LCD/`balanceOf` failure retries; it does **not** write `initial_balance = 0` as a fake success. A pending intent is not a registration: GET `/v1/registration/:addr` returns `pending` until the ledger row exists ([#9](https://gitlab.com/PlasticDigits/voting/-/issues/9)).

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
