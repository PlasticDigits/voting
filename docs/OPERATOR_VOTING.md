# operator-voting

Cross-links: [LEDGER_INVARIANTS.md](LEDGER_INVARIANTS.md) · [FRONTEND.md](FRONTEND.md) · [OPS.md](OPS.md) · issues [#2](https://gitlab.com/PlasticDigits/voting/-/issues/2) · [#4](https://gitlab.com/PlasticDigits/voting/-/issues/4) · [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) · [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) · [#10](https://gitlab.com/PlasticDigits/voting/-/issues/10) · [#11](https://gitlab.com/PlasticDigits/voting/-/issues/11) · skills [AGENTS_OPS_STAGING.md](../skills/AGENTS_OPS_STAGING.md) · [AGENTS_DRAFT_REVIEW.md](../skills/AGENTS_DRAFT_REVIEW.md)

Standalone Axum service. **Not** merged into the DEX indexer API. Uses a **restricted** `DATABASE_URL` in production.

## Endpoints

| Method | Path | Notes |
|--------|------|-------|
| `GET` | `/health` | Liveness |
| `GET` | `/openapi.json` | Stub OpenAPI |
| `POST` | `/v1/register` | ADR-36 or EIP-191; enqueue ledger intent |
| `GET` | `/v1/registration/:addr` | Terra and/or BSC status |
| `POST` | `/v1/proposals` | Create a **draft** (`purpose=draft`). ≥1000 CL8Y on **that** address’s chain. No snapshot freeze. |
| `GET` | `/v1/proposals` | List + tallies. Optional `?status=draft\|open`. Includes `summary`. |
| `GET` | `/v1/proposals/:id` | Detail; sections, comments, analysis; `advisory: true` |
| `PUT` | `/v1/proposals/:id/sections` | Proposer amends while `draft`. `409` on stale `prev_body_hash`. |
| `POST` | `/v1/proposals/:id/comments` | Registered wallet; not 1000 CL8Y. Draft **and** open. |
| `POST` | `/v1/proposals/:id/analysis` | Committee only; not the proposer. Draft only. |
| `POST` | `/v1/proposals/:id/open` | Committee `purpose=open_vote`. Freeze from indexer tips. Client heights ignored. |
| `POST` | `/v1/proposals/:id/votes` | One vote per `(proposal, chain, wallet)`. **Forbidden unless `status=open`.** |
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
- Purpose / chain / proposal id / app name are domain-separated. A register blob cannot vote. A `propose` blob cannot create a draft or open a poll. `draft` / `amend` / `comment` / `analyze` / `open_vote` / `vote` are distinct. ADR-36 posted as EVM (and the reverse) is rejected.
- TTL default 10 minutes (`issued_at` / `expires_at`).

## Snapshot (OV-S, issue #11)

Freeze happens at **vote-open**, not draft create.

| ID | Rule |
|----|------|
| **OV-S1** | `POST /v1/proposals` inserts `status=draft` with `terra_height` / `bsc_block` **null**. `proposal_snapshots` is empty until open. |
| **OV-S2** | `POST /v1/proposals/:id/open` writes freeze heights from current `indexer_state` tips (`last_indexed_height`, `last_indexed_bsc_block`). JSON `terra_height` / `bsc_block` on the request are ignored. |
| **OV-S3** | Vote weight is the frozen `proposal_snapshots` row at those open heights. Selling after open does not change weight. |
| **OV-S4** | A wallet that registers **during draft** and holds CL8Y is in the electorate if `voting_registrations` exists before freeze (ledger L6 still applies at the freeze heights). Register **after** open is not in that snapshot. |
| **OV-S5** | `POST .../votes` on `status≠open` is `403`. Replay of a `vote` payload against a still-draft id fails. Open twice is `409`. |
| **OV-S6** | Draft ≥1000 CL8Y uses the live OV-B1 height (`max(tip, registered_at_height)`). That check is not a freeze. |

Threshold: `MIN_PROPOSAL_CL8Y` (default 1000 human units) → `1000 * 10^18` raw, evaluated on the registering address’s chain only, **to start a draft**. Opening a poll is committee-only and is **not** a 1000 CL8Y check.

Do not freeze twice. Do not take freeze heights from the client.

## Template (issue #10)

New drafts persist `body_sections` JSONB. Required keys: `problem`, `solution`, `pros_cons`, `summary`, `success_criteria` (≥40 visible characters after strip-tags + whitespace collapse). `context` is optional. `summary` ≤ 500 visible characters. Combined sanitized HTML ≤ 64 KiB.

`body_hash` is SHA-256 of compact UTF-8 JSON with **sorted keys** (`context`, `problem`, `pros_cons`, `solution`, `success_criteria`, `summary`) and ammonia-sanitized values. Code: [`../operator-voting/src/sections.rs`](../operator-voting/src/sections.rs) · TS [`../frontend/src/utils/proposalSections.ts`](../frontend/src/utils/proposalSections.ts).

Legacy rows without sections still render `body_html`.

## Draft collaboration (issue #11)

Status machine: `draft` → `open` (committee `open_vote`). v1 has no `withdrawn` / `rejected`.

| Action | Who | Notes |
|--------|------|-------|
| Draft | registered, ≥1000 CL8Y | `purpose=draft` |
| Amend sections | proposer only, while draft | `purpose=amend`; `prev_body_hash` must match; last signed with matching prev wins; stale prev → `409` |
| Comment | any registered wallet (not 1000) | Draft **and** open. Cap **20 comments per address per proposal** (`MAX_COMMENTS_PER_WALLET_PER_PROPOSAL`). Empty after sanitize → `400`. Max 8 KiB. |
| Analysis | committee allowlist, **not** the proposer | `purpose=analyze`. Independent record; cannot be a proposer section. Immutable after open. |
| Open | `VOTING_COMMITTEE_ADDRESSES` | `purpose=open_vote`; must sign current section hash. Analysis is optional (UI shows “no independent analysis attached”). |
| Vote | registered at freeze with weight | `status=open` only |

Identity v1: comments, amends, and committee actions are per address. Do not merge `terra1…` and `0x…`.

AI review is **not** implemented. Operators skip it. Committee signature is the only open-vote authority. `source=ai` is reserved in the table CHECK for a later env-gated job.

## Committee

`VOTING_COMMITTEE_ADDRESSES` — comma-separated Terra bech32 and/or `0x`, same parser as the blacklist. Invalid entries fail startup. Empty allowlist → nobody can open or attach analysis (fail closed). Opening does not require 1000 CL8Y on the committee wallet.

## Blacklist

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
| **OV-B5** | Draft/open/vote ignore client-supplied balances and freeze heights. Weight is `public.voting_*_balance_at` / frozen `proposal_snapshots` only. |

`GET /v1/registration/:addr` returns `pending: { terra, bsc }` while an intent is in flight. It 404s only when there is neither a ledger row nor a pending intent.

Code: [`../operator-voting/src/balance_query.rs`](../operator-voting/src/balance_query.rs). Frontend: [`FRONTEND.md`](FRONTEND.md).

## Blacklist

`VOTING_BLACKLIST_ADDRESSES` — comma-separated Terra bech32 and/or `0x`. Invalid entries fail startup. Compare after lowercase / EIP-55 normalize. Blacklisted addresses cannot register, draft, comment, amend, open, or vote.

## HTML

Proposal bodies **and comments / analysis** are stored after [ammonia](https://docs.rs/ammonia) allowlist (`p`, headings, lists, `a[href]`, …). Scripts and event handlers are stripped. Body cap 64 KiB; comments 8 KiB.

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
