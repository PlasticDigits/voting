# Ops runbook (issue #7)

Cross-links: [HANDOFF.md](HANDOFF.md) · [ARCHITECTURE.md](ARCHITECTURE.md) · [OPERATOR_VOTING.md](OPERATOR_VOTING.md) · [FRONTEND.md](FRONTEND.md) · [LEDGER_INVARIANTS.md](LEDGER_INVARIANTS.md) · skill [AGENTS_OPS_STAGING.md](../skills/AGENTS_OPS_STAGING.md) · Legal [AGENTS_LEGAL_CLICKWRAP.md](../skills/AGENTS_LEGAL_CLICKWRAP.md) · wallets [AGENTS_WALLET_CONNECTORS.md](../skills/AGENTS_WALLET_CONNECTORS.md) · GitLab [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) · [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) · [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) · [#12](https://gitlab.com/PlasticDigits/voting/-/issues/12) · [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14)

In-tree ledger / `operator-voting` / `/vote` landed in !1. This document is the remaining **ops + public-expose** checklist. Do not mark production voting done until every required item below is true on staging.

## Invariants (O1–O8)

| ID | Rule |
|----|------|
| **O1** | Three Coolify services: `voting-ledger`, `operator-voting`, dApp. Ledger writer migrates; `operator-voting` uses the restricted role from [`../deploy/grants.sql`](../deploy/grants.sql) (L10). |
| **O2** | `APPLY_MIGRATIONS` is **false** for `operator-voting` in prod. The image pins it; `RUN_MODE=prod` **refuses to start** if it is true. Never give that process `INSERT` on ledger ingest tables. Apply [`../deploy/grants.sql`](../deploy/grants.sql) as the DB owner — privilege tests run that file, not a lookalike. |
| **O3** | Production / Coolify frontend builds **unset** `VITE_PLAYWRIGHT_E2E` and `VITE_DEV_MNEMONIC`, and **require** `VITE_WC_PROJECT_ID` ([#12](https://gitlab.com/PlasticDigits/voting/-/issues/12)). `vite.config.ts` (`prodEnvGuards`) and [`../deploy/docker/frontend.Dockerfile`](../deploy/docker/frontend.Dockerfile) fail the image if the hatch is on or the WC id is missing. CSP (`connect-src` / `frame-src`, no blanket `https:` or `frame-src *`) is stamped by the image from [`../deploy/docker/frontend.security-headers.conf`](../deploy/docker/frontend.security-headers.conf) **and** by the Coolify static paste [`../deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf). Live `vote.cl8y.com` on Coolify’s stock nginx (1.31.x) does **not** inherit the Dockerfile snippet — re-paste that file after pulling (include WC `frame-src`), then `curl -sI https://vote.cl8y.com/vote` must show `X-Frame-Options: DENY` and a `Content-Security-Policy` without `connect-src https:`. |
| **O4** | No `VITE_*` BSC JSON-RPC URL. Indexer / ledger owns `eth_call` / `eth_getLogs`. |
| **O5** | Connected `/vote` UI stays behind [cl8y-ecosystem-legal](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal). Property is `vote.cl8y.com` (registered; Legal #12). |
| **O6** | `operator-voting` POST endpoints are IP/QPS limited (**O-RL1–O-RL5** in [OPERATOR_VOTING.md](OPERATOR_VOTING.md)) before public expose. Body cap (64 KiB) is not a substitute. |
| **O7** | Identity v1 = one address, one voter. Live QA must exercise **Keplr Terra** and **MetaMask BSC 56** separately. |
| **O8** | SPA documents: `GET /`, `/vote`, `/new`, `/vote/new`, `/vote/:id`, `/:id` return **200** `text/html`. Missing `/assets/*` stay **404**. Coolify must use [`../deploy/docker/frontend.nginx.conf`](../deploy/docker/frontend.nginx.conf) (`try_files $uri /index.html`) or [`../deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf). HEALTHCHECK probes `/vote` and `/new`, not only `/`. [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) is closed on live 200; reopen it if `/vote` 404s again. |

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
5. Build the dApp with `VITE_OPERATOR_VOTING_URL=https://…` and `VITE_WC_PROJECT_ID` (WalletConnect Cloud project that lists `https://vote.cl8y.com`). Do not pass `VITE_PLAYWRIGHT_E2E`, `VITE_DEV_MNEMONIC`, or any `VITE_*` BSC RPC. The image writes Legal + API origins into nginx CSP. Confirm the service uses [`../deploy/docker/frontend.Dockerfile`](../deploy/docker/frontend.Dockerfile) (nginx 1.27 + `frontend.nginx.conf`). If Coolify’s default static nginx is in front instead, paste [`../deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf) (`try_files $uri /index.html`, `/assets/` `=404`). Prove:

   ```bash
   curl -sI https://vote.cl8y.com/vote
   curl -sI https://vote.cl8y.com/new
   curl -sI https://vote.cl8y.com/assets/missing.js
   ```

   `/` and `/vote` and `/new` must be 200 HTML. `/assets/missing.js` must be 404. [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) closed after live `/vote` became 200; keep proving it after nginx edits. Live edge is `https://vote.cl8y.com` (dApp) and `https://operator.vote.cl8y.com` (API). Ledger stays private. After pasting the Coolify snippet, SPA HTML must also send O3 headers (`X-Frame-Options: DENY`, explicit CSP).

`GET /health` on ledger and API must be 200 before opening DNS. A **proxy 503** (`no available server`) is not a healthy poller — nothing is calling `process_pending_intents`, so Register stays pending ([#14](https://gitlab.com/PlasticDigits/voting/-/issues/14)). Confirm the Coolify service name if `ledger.vote.cl8y.com` is not the writer. Ledger `/health` keeps `ok: true` as **liveness** (do not bounce the poller on boot). After a holder registers, inspect `caught_up`, `terra_height`, `terra_behind_registration` / `bsc_behind_registration`, and `intents_ok` (false means `voting.registration_intents` SELECT/UPDATE failed — L12 — not an LCD retry). A cursor of `0` while `voting_registrations` exists is **not** caught up ([#9](https://gitlab.com/PlasticDigits/voting/-/issues/9)). Default GET `/v1/balances` still clamps to `registered_at_height` (OV-B1); ingest catching up is still required for post-register transfers. Live GET `/v1/balances` must include `registered` / `pending` / `as_of_height` (OV-B6); redeploy `operator-voting` from `main` if those keys are missing.

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
| Cosmos WalletConnect | Terra mobile | Galaxy Station / Keplr / Station / Cosmostation / LuncDash: Open/Copy sheet or desktop QR — not hung Connecting... ([#12](https://gitlab.com/PlasticDigits/voting/-/issues/12), WC-M1–M12) |
| BSC WalletConnect | BSC 56 | wagmi QR/modal → `0x…` on chain 56. Cancel must clear EVM connecting. |

Wrong-chain MetaMask (not 56) and missing `signArbitrary` must show the existing readable errors. Do not ask for seeds.

## 4. POST rate limits (in-tree, required before public expose)

`operator-voting` uses `governor` keyed by client IP (equivalent to `tower-governor`).

| Env | Prod default | Meaning |
|-----|--------------|---------|
| `RATE_LIMIT_POST_PER_MINUTE` | `60` | POST quota per IP |
| `RATE_LIMIT_POST_BURST` | `20` | Burst tokens |
| `RATE_LIMIT_TRUST_FORWARDED` | `true` when `RUN_MODE=prod` | Trust Coolify `X-Forwarded-For` |

`RUN_MODE=prod` refuses to start if the quota is `0` **or** `APPLY_MIGRATIONS` is true. GET `/health` is unlimited. Quota is **per replica** (O-RL5). A 429 includes `Retry-After`.

Staging 429 smoke must be a **parallel** burst. GCRA refills ~1 token/s at the prod default (60/min, burst 20), so 25 sequential curls over 20s never trip the limiter. Example that does:

```bash
python3 - <<'PY'
import json, urllib.request, urllib.error, concurrent.futures
url='https://operator.vote.cl8y.com/v1/register'
data=json.dumps({"chain":"terra"}).encode()
def one(_):
    req=urllib.request.Request(url, data=data, method='POST', headers={
        'Content-Type':'application/json','Origin':'https://vote.cl8y.com'})
    try:
        urllib.request.urlopen(req, timeout=15)
        return 200
    except urllib.error.HTTPError as e:
        return e.code
with concurrent.futures.ThreadPoolExecutor(max_workers=40) as ex:
    codes=list(ex.map(one, range(80)))
print(sorted(set(codes)), '429 count', codes.count(429))
PY
```

Expect some `422` (invalid body), many `429` with `Retry-After`, and `GET /health` still 200. A few proxy `502`s under burst are not a fail by themselves.

## 5. LocalTerra / LCD equality (optional)

LocalTerra on this workstation is typically `http://127.0.0.1:1317`. Hardening loop:

1. Mint TCL8Y to a test wallet (DEX LocalTerra fixtures — do not invent a new CW20).
2. Register via `operator-voting` (ledger takes a **live** LCD `Balance`, L1). Confirm `CL8Y_TOKEN_ADDRESS` is the pinned hpax3 contract (wallet ticker **CL8Y-cb** is that contract unless another is proven).
3. Transfer TCL8Y; wait for the ledger poller.
4. Assert `voting_registrations.initial_balance` ≈ live LCD `Balance` ≈ `GET /v1/balances` default `as_of_height` ≥ `registered_at_height` for that registered wallet.
5. Confirm ledger `GET /health` `terra_height` advances after register (`caught_up: true`).

Do not close [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) / [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) on a screenshot of “Registered” if `GET /v1/balances` is still `"0"` while LCD is not.

Do not close [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14) on a screenshot of Registering… or of the pending banner. #9 is LCD equality **after registered**; #14 is **never leaving pending**. Writer `/health` must not be 503; intents with a working LCD/`balanceOf` complete without manual DB edits.

Integration coverage without LocalTerra: [`../ledger/tests/ledger_integration.rs`](../ledger/tests/ledger_integration.rs) (`register_transfer_balance_at_and_no_backfill`) and [`../operator-voting/tests/api_flow.rs`](../operator-voting/tests/api_flow.rs) (`default_balance_clamps_when_tip_lags_register`). Live LCD equality remains an ops check.

Postgres integration tests take `pg_advisory_lock(739001)` ([`../ledger/src/test_lock.rs`](../ledger/src/test_lock.rs)) so parallel `cargo test` binaries cannot `TRUNCATE` each other.

## Still blocked without human ops

These cannot be completed from a repo-only agent:

- Coolify project create / DNS / TLS / secret install (dApp + `operator-voting` are up; ledger writer is private)
- Re-paste [`../deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf) so live `vote.cl8y.com` (nginx 1.31.x) sends O3 CSP / `X-Frame-Options` (include WC `frame-src`) — in-tree paste is not a Coolify save
- Legal admin property + CORS + portal allowlist write (done on [cl8y-ecosystem-legal#12](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal/-/issues/12))
- Keplr / MetaMask / Galaxy Station / BSC WalletConnect on a real staging origin — still blocked on [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) (registered balance 0). Do not close [#12](https://gitlab.com/PlasticDigits/voting/-/issues/12) on Playwright hatch E2E.
- Coolify frontend build-arg `VITE_WC_PROJECT_ID` set; WalletConnect Cloud project includes `https://vote.cl8y.com`
