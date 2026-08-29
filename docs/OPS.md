# Ops runbook (issue #7)

Cross-links: [HANDOFF.md](HANDOFF.md) · [ARCHITECTURE.md](ARCHITECTURE.md) · [OPERATOR_VOTING.md](OPERATOR_VOTING.md) · [FRONTEND.md](FRONTEND.md) · [LEDGER_INVARIANTS.md](LEDGER_INVARIANTS.md) · skill [AGENTS_OPS_STAGING.md](../skills/AGENTS_OPS_STAGING.md) · Legal [AGENTS_LEGAL_CLICKWRAP.md](../skills/AGENTS_LEGAL_CLICKWRAP.md) · wallets [AGENTS_WALLET_CONNECTORS.md](../skills/AGENTS_WALLET_CONNECTORS.md) · GitLab [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) · [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) · [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9)

In-tree ledger / `operator-voting` / `/vote` landed in !1. This document is the remaining **ops + public-expose** checklist. Do not mark production voting done until every required item below is true on staging.

## Invariants (O1–O8)

| ID | Rule |
|----|------|
| **O1** | Three Coolify services: `voting-ledger`, `operator-voting`, dApp. Ledger writer migrates; `operator-voting` uses the restricted role from [`../deploy/grants.sql`](../deploy/grants.sql) (L10). |
| **O2** | `APPLY_MIGRATIONS` is **false** for `operator-voting` in prod. The image pins it; `RUN_MODE=prod` **refuses to start** if it is true. Never give that process `INSERT` on ledger ingest tables. Apply [`../deploy/grants.sql`](../deploy/grants.sql) as the DB owner — privilege tests run that file, not a lookalike. |
| **O3** | Production / Coolify frontend builds **unset** `VITE_PLAYWRIGHT_E2E` and `VITE_DEV_MNEMONIC`. `vite.config.ts` (`prodEnvGuards`) and [`../deploy/docker/frontend.Dockerfile`](../deploy/docker/frontend.Dockerfile) fail the image if the hatch is on. The image stamps an explicit CSP (`connect-src`, no blanket `https:`) from [`../deploy/docker/frontend.security-headers.conf`](../deploy/docker/frontend.security-headers.conf). |
| **O4** | No `VITE_*` BSC JSON-RPC URL. Indexer / ledger owns `eth_call` / `eth_getLogs`. |
| **O5** | Connected `/vote` UI stays behind [cl8y-ecosystem-legal](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal). Property is dedicated (`vote.cl8y.com` proposed — confirm before admin write). |
| **O6** | `operator-voting` POST endpoints are IP/QPS limited (**O-RL1–O-RL5** in [OPERATOR_VOTING.md](OPERATOR_VOTING.md)) before public expose. Body cap (64 KiB) is not a substitute. |
| **O7** | Identity v1 = one address, one voter. Live QA must exercise **Keplr Terra** and **MetaMask BSC 56** separately. |
| **O8** | SPA documents: `GET /`, `/vote`, `/new`, `/vote/new`, `/vote/:id`, `/:id` return **200** `text/html`. Missing `/assets/*` stay **404**. Coolify must use [`../deploy/docker/frontend.nginx.conf`](../deploy/docker/frontend.nginx.conf) (`try_files $uri /index.html`) or an equivalent edge snippet. HEALTHCHECK probes `/vote` and `/new`, not only `/`. Do not close [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) while live `/vote` 404s. |

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
3. As the DB owner, apply [`../deploy/grants.sql`](../deploy/grants.sql) with `psql -v ON_ERROR_STOP=1 -d "$WRITER_URL"`. Set a real `operator_voting` password (not the file default). Privilege tests apply that same file via `psql -d`. `GRANT EXECUTE` must stay on `public.voting_*_balance_at` (L10 search_path).
4. Start `operator-voting` with the restricted URL. Confirm `APPLY_MIGRATIONS` is unset/false (image default; prod config refuses true). Set `CORS_ORIGINS=https://vote.cl8y.com` (or the staging origin). Set `RATE_LIMIT_TRUST_FORWARDED=true` (Coolify proxy).
5. Build the dApp with `VITE_OPERATOR_VOTING_URL=https://…` only. Do not pass `VITE_PLAYWRIGHT_E2E`, `VITE_DEV_MNEMONIC`, or any `VITE_*` BSC RPC. The image writes Legal + API origins into nginx CSP. Confirm the service uses [`../deploy/docker/frontend.Dockerfile`](../deploy/docker/frontend.Dockerfile) (nginx 1.27 + `frontend.nginx.conf`). If Coolify’s default static nginx is in front instead, paste the same `try_files $uri /index.html` (and keep `/assets/` as `=404`). Prove:

   ```bash
   curl -sI https://vote.cl8y.com/vote
   curl -sI https://vote.cl8y.com/new
   curl -sI https://vote.cl8y.com/assets/missing.js
   ```

   `/` and `/vote` and `/new` must be 200 HTML. `/assets/missing.js` must be 404. Live `nginx/1.31.x` without this fallback was the [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) Legal-return 404.

`GET /health` on ledger and API must be 200 before opening DNS. Ledger `/health` keeps `ok: true` as **liveness** (do not bounce the poller on boot). After a holder registers, inspect `caught_up`, `terra_height`, and `terra_behind_registration` / `bsc_behind_registration`. A cursor of `0` while `voting_registrations` exists is **not** caught up ([#9](https://gitlab.com/PlasticDigits/voting/-/issues/9)). Default GET `/v1/balances` still clamps to `registered_at_height` (OV-B1); ingest catching up is still required for post-register transfers.

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

`RUN_MODE=prod` refuses to start if the quota is `0` **or** `APPLY_MIGRATIONS` is true. GET `/health` is unlimited. Quota is **per replica** (O-RL5). A 429 includes `Retry-After`.

## 5. LocalTerra / LCD equality (optional)

LocalTerra on this workstation is typically `http://127.0.0.1:1317`. Hardening loop:

1. Mint TCL8Y to a test wallet (DEX LocalTerra fixtures — do not invent a new CW20).
2. Register via `operator-voting` (ledger takes a **live** LCD `Balance`, L1). Confirm `CL8Y_TOKEN_ADDRESS` is the pinned hpax3 contract (wallet ticker **CL8Y-cb** is that contract unless another is proven).
3. Transfer TCL8Y; wait for the ledger poller.
4. Assert `voting_registrations.initial_balance` ≈ live LCD `Balance` ≈ `GET /v1/balances` default `as_of_height` ≥ `registered_at_height` for that registered wallet.
5. Confirm ledger `GET /health` `terra_height` advances after register (`caught_up: true`).

Do not close [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) / [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) on a screenshot of “Registered” if `GET /v1/balances` is still `"0"` while LCD is not.

Integration coverage without LocalTerra: [`../ledger/tests/ledger_integration.rs`](../ledger/tests/ledger_integration.rs) (`register_transfer_balance_at_and_no_backfill`) and [`../operator-voting/tests/api_flow.rs`](../operator-voting/tests/api_flow.rs) (`default_balance_clamps_when_tip_lags_register`). Live LCD equality remains an ops check.

Postgres integration tests take `pg_advisory_lock(739001)` ([`../ledger/src/test_lock.rs`](../ledger/src/test_lock.rs)) so parallel `cargo test` binaries cannot `TRUNCATE` each other.

## Still blocked without human ops

These cannot be completed from a repo-only agent:

- Coolify project create / DNS / TLS / secret install
- Legal admin property + CORS + portal allowlist write
- Keplr / MetaMask / mobile WC on a real staging origin
- Switching the live `vote.cl8y.com` edge to `frontend.nginx.conf` (or equivalent `try_files`) so `/vote` is 200 — in-tree O8 is not a DNS change
