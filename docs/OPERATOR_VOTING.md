# operator-voting

Cross-links: [LEDGER_INVARIANTS.md](LEDGER_INVARIANTS.md) · [FRONTEND.md](FRONTEND.md) · [OPS.md](OPS.md) · issues [#2](https://gitlab.com/PlasticDigits/voting/-/issues/2) · [#4](https://gitlab.com/PlasticDigits/voting/-/issues/4) · [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) · [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) · skill [AGENTS_OPS_STAGING.md](../skills/AGENTS_OPS_STAGING.md)

Standalone Axum service. **Not** merged into the DEX indexer API. Uses a **restricted** `DATABASE_URL` in production.

## Endpoints

| Method | Path | Notes |
|--------|------|-------|
| `GET` | `/health` | Liveness |
| `GET` | `/openapi.json` | Stub OpenAPI |
| `POST` | `/v1/register` | ADR-36 or EIP-191; enqueue ledger intent |
| `GET` | `/v1/registration/:addr` | Terra and/or BSC status |
| `POST` | `/v1/proposals` | ≥1000 CL8Y on **that** address’s chain; dual snapshot |
| `GET` | `/v1/proposals` | List + tallies |
| `GET` | `/v1/proposals/:id` | Detail; `advisory: true` |
| `POST` | `/v1/proposals/:id/votes` | One vote per `(proposal, chain, wallet)` |
| `GET` | `/v1/proposals/:id/votes/:addr` | Own vote |
| `GET` | `/v1/balances/:addr` | Thin restricted-view read. Default height is **OV-B1** (below). |

## Signing domain

Canonical JSON (`app` = `cl8y-voting`):

```json
{
  "app": "cl8y-voting",
  "chain": "terra",
  "chain_id": "columbus-5",
  "purpose": "register",
  "address": "terra1…",
  "issued_at": 0,
  "expires_at": 0
}
```

- Terra: Keplr `signArbitrary` (ADR-36 amino wrap). Simulated Wallet may sign raw secp256k1; the verifier accepts ADR-36 first, then raw fallback.
- EVM: EIP-191 `personal_sign`. Recovered `0x` must match (case-insensitive).
- Purpose / chain / proposal id / app name are domain-separated. A register blob cannot vote. ADR-36 posted as EVM (and the reverse) is rejected.
- TTL default 10 minutes (`issued_at` / `expires_at`).

## Snapshot

Proposal create records `{ terra_height, bsc_block }` from `indexer_state` (wall-clock aligned tips). Vote weight is the **frozen snapshot row**, not live tip. Selling after create does not change weight.

Threshold: `MIN_PROPOSAL_CL8Y` (default 1000 human units) → `1000 * 10^18` raw, evaluated on the registering address’s chain only.

The proposer’s freeze height on **that** chain is `max(indexer tip, registered_at_height)`. A Terra registered-at height is never applied to the BSC freeze clock (and the reverse).

## Balance read (OV-B1, issue #9)

`GET /v1/balances/:addr` is the dApp’s eligibility figure. It is **not** a live LCD/`balanceOf` proxy.

| Field | Meaning |
|-------|---------|
| `registered` | `voting_registrations` row exists for the inferred chain |
| `pending` | Unprocessed `voting.registration_intents` row; snapshot not written yet |
| `balance` | `public.voting_*_cl8y_balance_at` at `as_of_height` |
| `as_of_height` / `height` | Height or BSC block used for that call |
| `initial_balance` | Live snapshot stored at register; null if unregistered |

Rules:

| ID | Rule |
|----|------|
| **OV-B1** | Omitted `height` for a registered wallet is `max(indexer tip, registered_at_height)`. If the tip is `0` or still behind the live snapshot, the default read uses `registered_at_height` so L6 does not return 0. |
| **OV-B2** | Explicit `?height=` is used as-is. Below `registered_at_height` the SQL function still returns 0 (L6). Future heights cannot mint checkpoints the indexer never wrote. |
| **OV-B3** | Unregistered / pending wallets still return HTTP 200 with `registered: false` and `balance: "0"`. The UI must not present that 0 as a live chain holding. |
| **OV-B4** | `?chain=` that contradicts the address prefix (`terra1…` vs `0x…`) is `400`. No cross-chain sum. |
| **OV-B5** | Propose/vote ignore client-supplied balances. Weight is `public.voting_*_balance_at` / frozen `proposal_snapshots` only. |

`GET /v1/registration/:addr` returns `pending: { terra, bsc }` while an intent is in flight. It 404s only when there is neither a ledger row nor a pending intent.

Code: [`../operator-voting/src/balance_query.rs`](../operator-voting/src/balance_query.rs). Frontend: [`FRONTEND.md`](FRONTEND.md).

## Blacklist

`VOTING_BLACKLIST_ADDRESSES` — comma-separated Terra bech32 and/or `0x`. Invalid entries fail startup. Compare after lowercase / EIP-55 normalize. Blacklisted addresses cannot register-propose-vote (propose and vote denied; register also denied).

## HTML

Proposal bodies are stored after [ammonia](https://docs.rs/ammonia) allowlist (`p`, headings, lists, `a[href]`, …). Scripts and event handlers are stripped. Body cap 64 KiB.

## POST rate limits (O-RL)

`governor` keyed by IP (equivalent to `tower-governor`). Required before public expose ([#7](https://gitlab.com/PlasticDigits/voting/-/issues/7)). Code: [`../operator-voting/src/rate_limit.rs`](../operator-voting/src/rate_limit.rs).

| ID | Rule |
|----|------|
| **O-RL1** | Every POST shares one per-IP quota. |
| **O-RL2** | GET `/health` and other reads are not QPS-limited. |
| **O-RL3** | Body cap (`MAX_BODY_BYTES`) is independent. |
| **O-RL4** | `X-Forwarded-For` / `X-Real-IP` trusted only when `RATE_LIMIT_TRUST_FORWARDED` is on (Coolify / prod default). |
| **O-RL5** | Counters are in-process per replica. |

`RUN_MODE=prod` refuses `RATE_LIMIT_POST_PER_MINUTE=0`. Defaults: 60 POST/min, burst 20. A 429 response includes `Retry-After`.

## Migrations

sqlx migrations live in the ledger crate. `operator-voting` applies them only when `APPLY_MIGRATIONS` is true (dev default). Prod (`RUN_MODE=prod`) defaults to **false**, the Coolify image pins `APPLY_MIGRATIONS=false`, and config **refuses to start** if prod tries to migrate — the restricted role never owns schema. Coolify applies [`../deploy/grants.sql`](../deploy/grants.sql) after the ledger writer migrates (`USAGE` on `voting`, not `CREATE`). See [OPS.md](OPS.md) O1–O2.

## Env

See [../.env.example](../.env.example) and [../deploy/coolify.env.example](../deploy/coolify.env.example). Production compose **must not** reuse the ledger writer `DATABASE_URL`.
