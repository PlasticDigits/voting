# Ops runbook (issue #7)

Cross-links: [HANDOFF.md](HANDOFF.md) · [ARCHITECTURE.md](ARCHITECTURE.md) · [OPERATOR_VOTING.md](OPERATOR_VOTING.md) · [FRONTEND.md](FRONTEND.md) · [LEDGER_INVARIANTS.md](LEDGER_INVARIANTS.md) · skill [AGENTS_OPS_STAGING.md](../skills/AGENTS_OPS_STAGING.md) · Legal [AGENTS_LEGAL_CLICKWRAP.md](../skills/AGENTS_LEGAL_CLICKWRAP.md) · wallets [AGENTS_WALLET_CONNECTORS.md](../skills/AGENTS_WALLET_CONNECTORS.md) · GitLab [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7)

In-tree ledger / `operator-voting` / `/vote` landed in !1. This document is the remaining **ops + public-expose** checklist. Do not mark production voting done until every required item below is true on staging.

## Invariants (O1–O7)

| ID | Rule |
|----|------|
| **O1** | Three Coolify services: `voting-ledger`, `operator-voting`, dApp. Ledger writer migrates; `operator-voting` uses the restricted role from [`../deploy/grants.sql`](../deploy/grants.sql) (L10). |
| **O2** | `APPLY_MIGRATIONS` is **false** for `operator-voting` in prod (`RUN_MODE=prod` default). Never give that process `INSERT` on ledger ingest tables. |
| **O3** | Production / Coolify frontend builds **unset** `VITE_PLAYWRIGHT_E2E` and `VITE_DEV_MNEMONIC`. `vite.config.ts` and [`../deploy/docker/frontend.Dockerfile`](../deploy/docker/frontend.Dockerfile) fail the image if the hatch is on. |
| **O4** | No `VITE_*` BSC JSON-RPC URL. Indexer / ledger owns `eth_call` / `eth_getLogs`. |
| **O5** | Connected `/vote` UI stays behind [cl8y-ecosystem-legal](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal). Property is dedicated (`vote.cl8y.com` proposed — confirm before admin write). |
| **O6** | `operator-voting` POST endpoints are IP/QPS limited (**O-RL1–O-RL5** in [OPERATOR_VOTING.md](OPERATOR_VOTING.md)) before public expose. Body cap (64 KiB) is not a substitute. |
| **O7** | Identity v1 = one address, one voter. Live QA must exercise **Keplr Terra** and **MetaMask BSC 56** separately. |

## 1. Coolify / staging

Images (build context = repo root):

| Service | Dockerfile | Port | `DATABASE_URL` |
|---------|------------|------|----------------|
| `voting-ledger` | [`../deploy/docker/ledger.Dockerfile`](../deploy/docker/ledger.Dockerfile) | 3001 | ledger **writer** |
| `operator-voting` | [`../deploy/docker/operator-voting.Dockerfile`](../deploy/docker/operator-voting.Dockerfile) | 3002 | `operator_voting` **restricted** |
| dApp | [`../deploy/docker/frontend.Dockerfile`](../deploy/docker/frontend.Dockerfile) | 80 | none |

Env sketch: [`../deploy/coolify.env.example`](../deploy/coolify.env.example).

Boot order:

1. Create the `voting` database.
2. Start `voting-ledger` (`RUN_MODE=prod`, writer `DATABASE_URL`, `TERRA_LCD_URL`, `BSC_RPC_URLS`). It applies sqlx migrations.
3. As the DB owner, apply [`../deploy/grants.sql`](../deploy/grants.sql) and set a real `operator_voting` password (not the file default).
4. Start `operator-voting` with the restricted URL. Confirm `APPLY_MIGRATIONS` is unset/false. Set `CORS_ORIGINS=https://vote.cl8y.com` (or the staging origin). Set `RATE_LIMIT_TRUST_FORWARDED=true` (Coolify proxy).
5. Build the dApp with `VITE_OPERATOR_VOTING_URL=https://…` only. Do not pass `VITE_PLAYWRIGHT_E2E`.

`GET /health` on ledger and API must be 200 before opening DNS.

## 2. Legal ops (sibling repo)

Confirm hostname with ops **before** any admin write. Proposed: `vote.cl8y.com`.

From a [`cl8y-ecosystem-legal`](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal) checkout, using an **interactive** admin token (never `ADMIN_TOKEN` in this repo):

1. Register property `vote.cl8y.com` (Terra Classic **and** EVM networks).
2. Add `https://vote.cl8y.com` (and the staging origin if different) to Legal API `CORS_ORIGINS`.
3. Rebuild the Legal portal with `VITE_REDIRECT_URI_ALLOWLIST` including that origin.

In-tree wiring: [`../frontend/src/utils/legalClickwrap.ts`](../frontend/src/utils/legalClickwrap.ts). Playbook: [`../skills/AGENTS_LEGAL_CLICKWRAP.md`](../skills/AGENTS_LEGAL_CLICKWRAP.md).

## 3. Live wallet QA (staging, real wallets)

Playwright with `VITE_PLAYWRIGHT_E2E=true` is **not** a substitute. After Legal accept:

| Wallet | Chain | Path |
|--------|-------|------|
| Keplr | Terra Classic | Legal → register → propose if ≥1000 CW20 → vote |
| MetaMask | BSC 56 | Legal → register → vote (propose if ≥1000 BEP-20) |
| Cosmos WalletConnect | Terra mobile | Pairing still works with the ported DEX helpers (WC-M1–M12) |

Wrong-chain MetaMask (not 56) and missing `signArbitrary` must show the existing readable errors. Do not ask for seeds.

## 4. POST rate limits (in-tree, required before public expose)

`operator-voting` uses `governor` keyed by client IP (equivalent to `tower-governor`).

| Env | Prod default | Meaning |
|-----|--------------|---------|
| `RATE_LIMIT_POST_PER_MINUTE` | `60` | POST quota per IP |
| `RATE_LIMIT_POST_BURST` | `20` | Burst tokens |
| `RATE_LIMIT_TRUST_FORWARDED` | `true` when `RUN_MODE=prod` | Trust Coolify `X-Forwarded-For` |

`RUN_MODE=prod` refuses to start if the quota is `0`. GET `/health` is unlimited. Quota is **per replica** (O-RL5).

## 5. LocalTerra / LCD equality (optional)

LocalTerra on this workstation is typically `http://127.0.0.1:1317`. Hardening loop:

1. Mint TCL8Y to a test wallet (DEX LocalTerra fixtures — do not invent a new CW20).
2. Register via `operator-voting` (ledger takes a **live** LCD `Balance`, L1).
3. Transfer TCL8Y; wait for the ledger poller.
4. Assert `voting_cl8y_balance_at(wallet, tip) ==` live LCD `Balance` for that registered wallet.

Integration coverage without LocalTerra: [`../ledger/tests/ledger_integration.rs`](../ledger/tests/ledger_integration.rs) (`register_transfer_balance_at_and_no_backfill`). Live LCD equality remains an ops check.

Postgres integration tests take `pg_advisory_lock(739001)` ([`../ledger/src/test_lock.rs`](../ledger/src/test_lock.rs)) so parallel `cargo test` binaries cannot `TRUNCATE` each other.

## Still blocked without human ops

These cannot be completed from a repo-only agent:

- Coolify project create / DNS / TLS / secret install
- Legal admin property + CORS + portal allowlist write
- Keplr / MetaMask / mobile WC on a real staging origin
