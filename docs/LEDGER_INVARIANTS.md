# Ledger invariants

Cross-links: [ARCHITECTURE.md](ARCHITECTURE.md) · [OPERATOR_VOTING.md](OPERATOR_VOTING.md) · [OPS.md](OPS.md) · issues [#1](https://gitlab.com/PlasticDigits/voting/-/issues/1) · [#4](https://gitlab.com/PlasticDigits/voting/-/issues/4) · [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) · [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) · [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14) · skills [AGENTS_VOTING_BUNDLE.md](../skills/AGENTS_VOTING_BUNDLE.md) · [AGENTS_OPS_STAGING.md](../skills/AGENTS_OPS_STAGING.md)

This crate is `voting-ledger` (`ledger/`). It is **not** the DEX indexer. Do not implement these tables in `cl8y-dex-terraclassic`.

## L1 — No archive node

Registration observes a live end-of-block balance on one provider. Configure `TERRA_RPC_URLS` for native Comet RPC observations: validate `/status` network/sync state and fresh `/block` time, capture current height/hash, query CW20 through `/abci_query` at that current recent height, require the body's actual response height and recheck the preknown canonical hash. This uses ordinary retained current state and needs no archive node. Gateways that strip smart-query height headers cannot supply a proven LCD-only observation. LCD-only registration remains supported when the live response supplies the actual height and matches the prequery height/hash; a moving observation retries. BSC likewise binds `eth_call` to the observed current head and verifies its hash before/after. No empty/ambiguous observation invents a baseline.

SDK JSON block hashes encode 32-byte values as base64; Comet encodes the same bytes as hex. Normalize both to uppercase 64-hex identities. Both legacy `block` and current `sdk_block` headers are supported; conflicting headers fail closed.

Ingest is bounded to 32 blocks per independent chain poll. Only first boot **without an electorate** may start at tip. A nonzero cursor always advances from `last + 1`, regardless of gap length. Unavailable forward history leaves a durable degraded checkpoint; it never skips blocks while retaining old power. Explicit recovery creates a new live baseline epoch (L16).

## L2 — Registered-set only

CW20 wasm events and BEP-20 Transfer logs are stored only when `from` or `to` is already registered. Unregistered counterparties appear as the other leg of a registered transfer; they are **not** voters. Later registration is a new live snapshot (**no backfill**).

## L3 — Emitter trust (DEX #285)

Only wasmd-stamped `_contract_address` scopes a CW20 event. A forgeable `contract_address` attribute is ignored. Only the configured `CL8Y_TOKEN_ADDRESS` is indexed.

## L4 — Pinned BEP-20

Only `BSC_CL8Y_TOKEN_ADDRESS` (default `0x8F452a1fdd388A45e1080992eFF051b4dd9048d2`). Lookalike Transfer logs are dropped. Empty `BSC_RPC_URLS` skips live BSC (CI). Production (`RUN_MODE=prod`) **requires** `BSC_RPC_URLS`. Never log RPC URLs.

## L5 — 18-decimal integers

Amounts and resulting balances are unsigned integers in the uint256 range, stored as strings / `NUMERIC(78,0)`. No floats; underflow or overflow rolls back and degrades accounting.

## L6 — Inclusive height / block

A transfer at height/block `H` is included in `voting_cl8y_balance_at(wallet, H)` / `voting_bsc_cl8y_balance_at(wallet, H)`. Query at `H < registered_at_height` returns 0.

The SQL function is unchanged. **Default** GET `/v1/balances` (no `height` query) in `operator-voting` evaluates at `max(indexer tip, registered_at_height)` so a just-registered wallet is not read as 0 while `last_indexed_*` is still `0` or behind the live snapshot ([#9](https://gitlab.com/PlasticDigits/voting/-/issues/9), **OV-B1** in [OPERATOR_VOTING.md](OPERATOR_VOTING.md)). Explicit `?height=` below register still returns 0.

## L7 — Reorg unwind

Both chains retain a 128-block canonical horizon (committed head plus 128 predecessors). Each poll verifies the committed head; mismatch searches backwards for a common ancestor, then atomically unwinds unfinalized accounting after it. Hash-fetch failure prevents fresh readiness. A beyond-horizon fork invalidates frozen current-epoch/legacy polls for review without altering their immutable values. A fork beyond retained history or through a registration baseline durably requires explicit reconciliation. BSC indexing is 15 blocks behind the observed head. Frozen proposals referencing orphaned heights are marked invalid for review; their vote records and frozen power are not rewritten. Do not share tables with DEX Venus.

## L8 — Identity

`terra` and `bsc` registrations are distinct primary keys. Do not merge `terra1…` and `0x…`.

## L9 — Flash-in is counted

A large transfer into a registered wallet immediately before a proposal snapshot **is** vote power. Documented, not a bug, unless product adds lockup later.

## L10 — Least privilege

Balance functions are `SECURITY DEFINER` in schema **`public`**. Always call / `GRANT EXECUTE` `public.voting_cl8y_balance_at` and `public.voting_bsc_cl8y_balance_at`. The writer role is often named `voting` and control-plane tables live in schema `voting`, so Postgres `search_path` `"$user", public` would otherwise create or grant a **shadow copy** in schema `voting`. `operator_voting` gets `EXECUTE` on the public functions plus `SELECT` on `voting_registrations` / `indexer_state` and R/W on **tables** in schema `voting` (`USAGE` only — no `CREATE`). It must not `INSERT` ledger transfer/checkpoint tables. See [../deploy/grants.sql](../deploy/grants.sql) (CONNECT follows `current_database()`). Coolify: ledger writer applies migrations; `operator-voting` must not (`APPLY_MIGRATIONS=false`; prod refuses true). Privilege tests apply that file with `psql -d`. [OPS.md](OPS.md) O1–O2.

Every physical ledger pool connection sets `search_path=pg_catalog,public,pg_temp`, overriding role/database defaults and connection startup options. Worker lookups must resolve ledger data in `public`; schema `voting` holds control-plane tables and must never shadow ledger storage. Migration DDL temporarily uses `public,pg_temp` so unqualified objects are created in `public`, then restores the trusted writer path even after migration failure. The regression `writer_pool_pins_every_connection_against_startup_search_path_shadow_tables` exercises all ten pool connections and a fresh replacement against a legacy shadow table, verifies canonical reads/writes, and preserves both histories and frozen snapshot values. This safeguard does not promote or reconcile shadow-table data: inventory actual schemas, identities, rows and frozen references before any separate recovery decision. See [agent hardening workflow](../skills/AGENTS_LEDGER_HARDENING.md).

For a controlled diagnostic release, `LEDGER_INSPECT_STORAGE_ONLY=true` runs before migrations, recovery or workers in a PostgreSQL repeatable-read, read-only transaction. It logs one `VOTING_STORAGE_INVENTORY` JSON marker for the six allowlisted ledger relations and `_sqlx_migrations` in `public`/`voting`: existence, row counts, baseline-column presence, numeric cursors and migration versions. It compares only common allowlisted columns with `EXCEPT ALL` in both directions and logs mismatch counts, never rows, wallets, signatures or state strings. Each query has a one-second statement deadline and 500ms lock deadline; failed counts/comparisons are `null`, which proves neither zero nor equality. The whole inspection is bounded at 45 seconds and rolls back. This mode exposes `/live` 200 and `/ready`/`health` 503 with `storage_inspection`, performs no accounting/DDL/data promotion, and handles SIGTERM. Remove the flag and redeploy the same reviewed revision only after interpreting the inventory. Regression: `process_storage_inspection_precedes_migrations_and_never_starts_accounting`.

## L11 — Registration handoff

`operator-voting` inserts `voting.registration_intents`. A **separate** ledger task (not either ingest loop) queries live balances and writes `voting_registrations` (idempotent: no double initial credit). LCD/`balanceOf` failure retries; it does **not** write `initial_balance = 0` as a fake success. A pending intent is not a registration: GET `/v1/registration/:addr` returns `pending` until the ledger row exists ([#9](https://gitlab.com/PlasticDigits/voting/-/issues/9), [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14)). `ON CONFLICT (chain, wallet_address) DO NOTHING` — UI retry re-polls the same row; it does not insert a second intent. The writer role must `SELECT`/`UPDATE` `voting.registration_intents` (table owner after migrate).

## L12 — Intent query failures are visible

`pending_intents` / `mark_intent_processed` must **not** treat permission, missing-table, or other SQL errors as an empty queue. Fail the intent poll, log, and set `/health.intents_ok` to `false`. LCD live-balance retries do not flip `intents_ok` (those leave `processed_at` NULL and try again). Code: [`../ledger/src/db.rs`](../ledger/src/db.rs), [`../ledger/src/main.rs`](../ledger/src/main.rs).

## L13 — Execution completeness ([#36](https://git.cl8y.com/code/voting/issues/36))

Terra consumes `tx_responses`, checks execution height/hash/status and supported pagination totals, detects repeated keys and identities, and excludes failed executions from deltas. Ingest also hashes every base64 transaction in bound `block.data.txs` with SHA256 and matches the entire execution set/order, including failed executions; a disabled/partial tx-search index cannot certify an empty block. One pinned provider supplies the whole block and every page before failover retries it. A missing execution schema is never an empty block. Base64 legacy attributes decode together. All pages validate before a transaction may publish progress. Retained execution fields and pagination keys share an aggregate 64 MiB block budget, with at most 1000 pages and 100000 transactions; exceeding it cannot advance a cursor. WASM ordinals exclude tx/fee/message metadata and precede emitter/registration filtering, so logs/raw representations retain one identity. Additional SDK metadata is ignored for semantic stream comparison; conflicting accounting attributes fail. BSC verifies `eth_chainId=0x38`, exact pinned emitters, complete log/block/transaction identities, nonremoved logs, three 32-byte topics and a 32-byte uint256. Provider caps split ranges within bounded work. HTTP responses are capped at 4 MiB, requests at eight seconds; logs and errors contain classes rather than provider URLs or response bodies.

## L14 — Exactly once accounting ([#37](https://git.cl8y.com/code/voting/issues/37))

Only a newly inserted source event changes balances. Repeated identities must match their original payload; conflicting payloads fail. Terra event indices come from original WASM ordinals, independent of non-WASM metadata, emitter or registration filtering. Registrations persist the observed baseline block hash/epoch. Readiness and opening refuse unverified eligible baselines at/below the selected cursor; future baselines above it carry zero frozen weight and cannot halt the electorate. A new unfinalized BSC baseline remains unverified until the confirmed ingest cursor reaches its block and verifies the same canonical hash; a changed baseline durably degrades rather than retaining orphaned initial power. Registration and ingest use the same transaction advisory locks `(8136,1)` Terra / `(8136,2)` BSC. A baseline includes every transfer at its block; only **strictly later** transfers affect it. Registration observations happen outside the lock, then insertion refuses an observation overtaken by ingest. Event rows, running checkpoints, height/hash and readiness metadata commit together. Underflow and out-of-order new events roll back instead of silently repairing a balance. Workers record permanent `accounting_corruption` / `accounting_invalid_amount` classes requiring explicit recovery, separately from transient provider failure.

BSC range observations validate network identity around one pinned provider and fetch historical headers in batches of at most eight tasks. Every returned header must have the requested number and a complete hash; results are sorted by height independently of response completion order. All hashes must validate before accounting/cursor publication, and existing pre/post end-hash checks remain. One failed interior header rejects the entire observation; the bounded batch drains before failover restarts the complete observation on another provider and cannot combine forks. Regression coverage includes delayed HTTP timings, out-of-order completion, wrong interior numbers/hashes, unchanged database state on failure, and successful pinned failover/replay. See [production cadence/capability checks](OPS.md) and [agent workflow](../skills/AGENTS_LEDGER_HARDENING.md).

## L15 — Snapshot admission ([#38](https://git.cl8y.com/code/voting/issues/38))

`public.ledger_checkpoints` records height, hash, epoch, observed time, completeness, canonicality, network head and degraded reason. Opening acquires both accounting locks in Terra/BSC order. Both positive checkpoints must be complete, canonical, nondegraded, at most 120 seconds old, and within two blocks of their policy tip (Terra head; BSC head minus 15). A refreshed timestamp during catch-up cannot certify a distant cursor. The observed Terra latest header time/BSC head timestamp must also be at most 120 seconds old and no more than five seconds ahead of wall clock; a stale provider returning a consistent old head cannot renew readiness. Missing or malformed state fails closed. Legacy active electorates are marked `legacy_electorate_requires_reconciliation` on migration; old cursors alone cannot prove accounting correctness.

Head selection checks the committed cursor after canonical rewind. A fresh-looking provider below that cursor (below cursor plus 15 for the BSC network head) fails over to another configured provider; if all providers are behind, no accounting or checkpoint freshness is published. A genuine verified rewind therefore uses its new ancestor rather than the old cursor as the minimum. Mutable LCD latest-block and transaction-search requests send `Cache-Control: no-cache, max-age=0` and `Pragma: no-cache`; immutable block reads retain their ordinary request policy. These directives do not establish correctness: physical execution manifests, height/time, network and canonical hash checks still reject an inconsistent origin. See the cached-head and cached-gateway HTTP-to-Postgres regressions in [hardening.rs](../ledger/tests/hardening.rs) and [provider operations](OPS.md).

## L16 — Explicit live baseline epoch

Run `voting-ledger --reconcile-live-baseline` with the production writer/provider environment injected by `bao-exec`, while ordinary ledger workers are stopped. For a controlled Coolify release, set `LEDGER_RECONCILE_LIVE_BASELINE=true` explicitly: startup recovers only existing electorates whose checkpoint is absent or degraded, rechecks under the accounting lock, skips healthy committed epochs after restart, then starts workers. Remove the flag after verifying both epochs/readiness. Stop the pre-fix ledger **and API** before migration, because their writes/admission do not obey the new locks. The command holds one chain lock, obtains all active holder balances at one pinned current block/hash on one provider with eight concurrent recent-state reads per batch (Terra native RPC requires the ABCI body response height (or LCD requires an echoed `x-cosmos-block-height`); BSC uses an explicit current block tag), verifies BSC confirmation depth, inserts a new balance checkpoint and moves `baseline_height`, then increments the epoch and commits checkpoint metadata atomically. A missing returned height binding, divergent provider hash or fork aborts the transaction; it makes no partial chain changes. A successful Terra pass and failed BSC pass still cannot admit a dual-chain snapshot until both are ready. Past checkpoints and registration identity/initial balance stay intact, so historical frozen power remains readable. Replaying transfers before/at the new baseline cannot double count recovery balances. This is an explicit new eligibility epoch, **not** a claim that unavailable history was indexed. Restart resumes at the committed new baseline. Legacy conflicting Terra source identities are preserved verbatim in `cl8y_cw20_legacy_identity_conflicts` before the unique index is added. The active electorate remains degraded until reconciliation; archived rows are audit evidence and are never re-applied as newly discovered transfers.

## L17 — Worker and health behavior ([#39](https://git.cl8y.com/code/voting/issues/39))

Terra, BSC and intents have separate supervised tasks. A chain outage does not short circuit its sibling. Chain polls have a 60-second deadline and capped exponential backoff; fatal task exit shuts down for supervisor restart. SIGTERM/SIGINT stop HTTP, cancel bounded work and roll back uncommitted database transactions. `/live` is process liveness; `/ready` and `/health` return 503 for unavailable DB, absent/stale/degraded/lagging core chains or failed intent SQL. Health exposes safe chain age/lag and queue count/oldest age. Intents select 16 fair indexed items, claim retry leases, run four at a time with ten-second item deadlines, and back off failures; a failed early intent moves behind later arrivals. The entire intent pass, including SQL claims/bookkeeping, has a 60-second deadline; progress state publication has a three-second deadline. `indexer_state.intents_ok` must be true with `updated_at` age <=120 seconds. Individual poisoned or above-cutoff intents do not globally veto healthy progress; a hung pass, stale progress or SQL failure does. Queue claims survive restart, then retry when the bounded lease expires.

Regression scenarios live in [`ledger/tests/hardening.rs`](../ledger/tests/hardening.rs). Agent maintenance guidance: [`AGENTS_LEDGER_HARDENING.md`](../skills/AGENTS_LEDGER_HARDENING.md). Operator opening and incident review: [OPERATOR_VOTING.md](OPERATOR_VOTING.md). Deploy/reconciliation: [OPS.md](OPS.md).

## Env

Historical schema `voting` may contain ledger storage created under the old
`$user` path. Migration `20261014000105` archives full originals and metadata,
imports registrations and balance/event history after collision checks, preserves
frozen power, and requires explicit current canonical recovery. The legacy tables
and tracker stay untouched. Identical repeated promotion is a no-op; conflicting
rows or changed legacy fingerprints fail atomically. Tests:
[`legacy_namespace.rs`](../ledger/tests/legacy_namespace.rs). Live provenance:
[inventory](verification/2026-10-14-live-storage-inventory.json).

| Variable | Role |
|----------|------|
| `DATABASE_URL` | Writer role |
| `CL8Y_TOKEN_ADDRESS` | Terra CW20 |
| `BSC_CL8Y_TOKEN_ADDRESS` | BSC BEP-20 |
| `TERRA_LCD_URL` / `LCD_URLS` | Live LCD (comma-separated) |
| `TERRA_RPC_URLS` / `TERRA_RPC_URL` | Native response-height bound balances/recovery; required when the LCD strips observation headers |
| `TERRA_CHAIN_ID` | Expected network, default `columbus-5`; prod pins it, dev can explicitly choose `localterra` |
| `BSC_RPC_URLS` | Live BSC (comma-separated; never log) |
| `RUN_MODE` | `prod` fail-closed |
| `POLL_INTERVAL_MS` | Intent + ingest loop sleep (default `4000`) |
| `API_BIND` | Health listener (default `0.0.0.0:3001`) |
| `LEDGER_RECONCILE_LIVE_BASELINE` | Explicit one-time deploy recovery, default `false`; healthy epochs skip on restart |
| `LEDGER_INSPECT_STORAGE_ONLY` | Bounded read-only inventory, no migration/workers; `/live`200 and `/ready`503; default `false` |
