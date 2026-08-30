# operator-voting

Cross-links: [LEDGER_INVARIANTS.md](LEDGER_INVARIANTS.md) · [FRONTEND.md](FRONTEND.md) · [OPS.md](OPS.md) · issues [#2](https://gitlab.com/PlasticDigits/voting/-/issues/2) · [#4](https://gitlab.com/PlasticDigits/voting/-/issues/4) · [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) · [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) · [#10](https://gitlab.com/PlasticDigits/voting/-/issues/10) · [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14) · skill [AGENTS_OPS_STAGING.md](../skills/AGENTS_OPS_STAGING.md) · [AGENTS_VOTING_BUNDLE.md](../skills/AGENTS_VOTING_BUNDLE.md)

Standalone Axum service. **Not** merged into the DEX indexer API. Uses a **restricted** `DATABASE_URL` in production.

## Endpoints

| Method | Path | Notes |
|--------|------|-------|
| `GET` | `/health` | Liveness |
| `GET` | `/openapi.json` | Stub OpenAPI |
| `POST` | `/v1/register` | ADR-36 or EIP-191; enqueue ledger intent |
| `GET` | `/v1/registration/:addr` | Terra and/or BSC status |
| `POST` | `/v1/proposals` | ≥1000 CL8Y on **that** address’s chain; dual snapshot; **`body_sections` required** ([#10](https://gitlab.com/PlasticDigits/voting/-/issues/10), OV-S) |
| `GET` | `/v1/proposals` | List + tallies + `summary` TL;DR when sections exist |
| `GET` | `/v1/proposals/:id` | Detail; `advisory: true`; `body_sections` or legacy `body_html` |
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
| **OV-B6** | Every GET `/v1/balances` JSON includes `registered`, `pending`, `as_of_height`, and `initial_balance` (null if unregistered). Clients treat **missing** keys as unknown — never as a live 0 ([#14](https://gitlab.com/PlasticDigits/voting/-/issues/14)). Redeploy `operator-voting` from `main` if live JSON omits them. |

`GET /v1/registration/:addr` returns `pending: { terra, bsc }` while an intent is in flight. It 404s only when there is neither a ledger row nor a pending intent.

Code: [`../operator-voting/src/balance_query.rs`](../operator-voting/src/balance_query.rs). Frontend: [`FRONTEND.md`](FRONTEND.md).

## Blacklist

`VOTING_BLACKLIST_ADDRESSES` — comma-separated Terra bech32 and/or `0x`. Invalid entries fail startup. Compare after lowercase / EIP-55 normalize. Blacklisted addresses cannot register-propose-vote (propose and vote denied; register also denied).

## HTML

Proposal **sections** are stored after [ammonia](https://docs.rs/ammonia) allowlist (`p`, headings, lists, `a[href]`, …). Scripts and event handlers are stripped. Combined sanitized payload cap 64 KiB (`MAX_BODY_BYTES`). New creates must send `body_sections`; freeform `body_html` on POST is `400`.

## Structured sections (OV-S, issue #10)

New proposals are a fixed section object, not a single WYSIWYG blob. Votes stay offchain / advisory. Draft/review/committee is **not** this issue ([#13](https://gitlab.com/PlasticDigits/voting/-/issues/13)).

| Key | UI label | Required? |
|-----|----------|-----------|
| `problem` | The idea or problem to be solved | Yes |
| `context` | Supporting context, real-world issues, and alternatives | No (empty allowed) |
| `solution` | Proposed solution and supporting evidence | Yes |
| `pros_cons` | Pros and cons / trade-offs | Yes |
| `summary` | Summary (TL;DR) | Yes |
| `success_criteria` | Success criteria / goalposts | Yes |

Do **not** add a proposer-owned “independent analysis” field.

| ID | Rule |
|----|------|
| **OV-S1** | Server is source of truth. Missing, whitespace-only, or HTML-empty required sections → `400`. Count after strip-tags, entity decode, zero-width drop, Unicode whitespace collapse. Minima: 40 visible characters; `summary` also ≤ 500. |
| **OV-S2** | Persist `body_sections JSONB` (nullable for legacy). New creates always write the six string keys. `body_html` is a server-rendered concatenation in display order (TL;DR first). `body_canonical` is the hashed JSON string. |
| **OV-S3** | `body_hash` is SHA-256 (hex) of **canonical JSON**: UTF-8, compact (no extra whitespace), **sorted keys** `context`, `problem`, `pros_cons`, `solution`, `success_criteria`, `summary`. Each value is ammonia-cleaned HTML; empty-visible values become `""`. Hashing raw unsanitized HTML or freeform `body_html` → `401`. |
| **OV-S4** | Display order is `summary`, `problem`, `context`, `solution`, `pros_cons`, `success_criteria`. Hash order is sorted keys (OV-S3). Do not confuse them. |
| **OV-S5** | `GET /v1/proposals` includes `summary` (sanitized HTML or `null` for legacy). List cards must render it as **text**, not `innerHTML`. |
| **OV-S6** | Legacy rows with `body_sections IS NULL` still `GET` via `body_html`. Never mark that HTML as trusted; re-sanitize on read in the dApp. |
| **OV-S7** | Terra and BSC use the same template. Do not merge identities. 1000 CL8Y, blacklist, ammonia allowlist, POST QPS unchanged. No second snapshot freeze. |
| **OV-S8** | Success-criteria copy is goalposts for spend/outcome justification, not slashing or punishment. |

Serializer (TypeScript and Rust must match):

```json
{"context":"","problem":"<p>Holders often vote without enough information about impact and risks.</p>","pros_cons":"<p>Pros: proposals are comparable. Cons: writing takes more care.</p>","solution":"<p>Require labeled sections with server-enforced minimums.</p>","success_criteria":"<p>Voters can scan TL;DR, trade-offs, and goalposts before they sign.</p>","summary":"<p>Standard sections so every proposal is scannable before a vote.</p>"}
```

SHA-256 hex: `85b0e3985805be8d192178665cf0b745ddcb14cb5ccfc37d2b49c137fe0f5ed1`

Code: [`../operator-voting/src/proposal_sections.rs`](../operator-voting/src/proposal_sections.rs) · [`../frontend/src/utils/proposalSections.ts`](../frontend/src/utils/proposalSections.ts). Migration: [`../ledger/migrations/20260830000001_proposal_sections.sql`](../ledger/migrations/20260830000001_proposal_sections.sql). No new tables — `deploy/grants.sql` unchanged. Skill: [`../skills/AGENTS_PROPOSAL_TEMPLATE.md`](../skills/AGENTS_PROPOSAL_TEMPLATE.md).

## POST rate limits (O-RL)

## POST rate limits (O-RL)

`governor` keyed by IP (equivalent to `tower-governor`). Required before public expose ([#7](https://gitlab.com/PlasticDigits/voting/-/issues/7)). Code: [`../operator-voting/src/rate_limit.rs`](../operator-voting/src/rate_limit.rs). Staging smoke must be a **parallel** burst — sequential curls refill GCRA (see [OPS.md](OPS.md) §4). Live API: `https://operator.vote.cl8y.com`.

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
