# Ops runbook (issue #7)

Cross-links: [HANDOFF.md](HANDOFF.md) · [ARCHITECTURE.md](ARCHITECTURE.md) · [OPERATOR_VOTING.md](OPERATOR_VOTING.md) · [FRONTEND.md](FRONTEND.md) · [LEDGER_INVARIANTS.md](LEDGER_INVARIANTS.md) · skill [AGENTS_OPS_STAGING.md](../skills/AGENTS_OPS_STAGING.md) · Legal [AGENTS_LEGAL_CLICKWRAP.md](../skills/AGENTS_LEGAL_CLICKWRAP.md) · wallets [AGENTS_WALLET_CONNECTORS.md](../skills/AGENTS_WALLET_CONNECTORS.md) · GitLab [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) · [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) · [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) · [#12](https://gitlab.com/PlasticDigits/voting/-/issues/12) · [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14) · [#16](https://gitlab.com/PlasticDigits/voting/-/issues/16) · [#17](https://gitlab.com/PlasticDigits/voting/-/issues/17)

In-tree ledger / `operator-voting` / `/vote` landed in !1. This document is the remaining **ops + public-expose** checklist. Do not mark production voting done until every required item below is true on staging.

## Invariants (O1–O8)

| ID | Rule |
|----|------|
| **O1** | Three Coolify services: `voting-ledger`, `operator-voting`, dApp. Ledger writer migrates; `operator-voting` uses the restricted role from [`../deploy/grants.sql`](../deploy/grants.sql) (L10). |
| **O2** | `APPLY_MIGRATIONS` is **false** for `operator-voting` in prod. The image pins it; `RUN_MODE=prod` **refuses to start** if it is true. Never give that process `INSERT` on ledger ingest tables. Apply [`../deploy/grants.sql`](../deploy/grants.sql) as the DB owner — privilege tests run that file, not a lookalike. |
| **O3** | Production / Coolify frontend builds **unset** `VITE_PLAYWRIGHT_E2E` and `VITE_DEV_MNEMONIC`, and **require** `VITE_WC_PROJECT_ID` ([#12](https://gitlab.com/PlasticDigits/voting/-/issues/12)). `vite.config.ts` (`prodEnvGuards`) and [`../deploy/docker/frontend.Dockerfile`](../deploy/docker/frontend.Dockerfile) fail the image if the hatch is on or the WC id is missing. CSP (`connect-src` / `frame-src`, no blanket `https:` or `frame-src *`) is stamped by the image from [`../deploy/docker/frontend.security-headers.conf`](../deploy/docker/frontend.security-headers.conf) **and** by the Coolify static paste [`../deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf). Live `vote.cl8y.com` on Coolify’s stock nginx (1.31.x) does **not** inherit the Dockerfile snippet — re-paste that file after pulling (include WC `frame-src` **and** `https://api.web3modal.org` on `connect-src`; `*.web3modal.com` does not match AppKit’s `.org` API), then `curl -sI https://vote.cl8y.com/vote` must show `X-Frame-Options: DENY` and a `Content-Security-Policy` without `connect-src https:`. |
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
3. As the DB owner, apply [`../deploy/grants.sql`](../deploy/grants.sql) with `psql -v ON_ERROR_STOP=1 -d "$WRITER_URL"`. Fresh roles have no usable password; provision a unique credential through authorized OpenBao tooling. Upgrades preserve the existing opaque credential. Privilege tests apply that same file via `psql -d`. `GRANT EXECUTE` must stay on `public.voting_*_balance_at` (L10 search_path).
4. Start `operator-voting` with the restricted URL. Confirm `APPLY_MIGRATIONS` is unset/false (image default; prod config refuses true). Set `CORS_ORIGINS=https://vote.cl8y.com` (or the staging origin), `RATE_LIMIT_TRUST_FORWARDED=false`, and `VOTING_COMMITTEE_ADDRESSES` to the approved Terra/BSC committee wallets. An empty committee allowlist fails closed: drafts cannot open.
5. Build the dApp with `VITE_OPERATOR_VOTING_URL=https://…` and `VITE_WC_PROJECT_ID` (WalletConnect Cloud project that lists `https://vote.cl8y.com`). Do not pass `VITE_PLAYWRIGHT_E2E`, `VITE_DEV_MNEMONIC`, or any `VITE_*` BSC RPC. The image writes Legal + API origins into nginx CSP. Confirm the service uses [`../deploy/docker/frontend.Dockerfile`](../deploy/docker/frontend.Dockerfile) (nginx 1.27 + `frontend.nginx.conf`). If Coolify’s default static nginx is in front instead, paste [`../deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf) (`try_files $uri /index.html`, `/assets/` `=404`). Prove:

   ```bash
   curl -sI https://vote.cl8y.com/vote
   curl -sI https://vote.cl8y.com/new
   curl -sI https://vote.cl8y.com/assets/missing.js
   ```

   `/` and `/vote` and `/new` must be 200 HTML. `/assets/missing.js` must be 404. [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) closed after live `/vote` became 200; keep proving it after nginx edits. Live edge is `https://vote.cl8y.com` (dApp) and `https://operator.vote.cl8y.com` (API). Ledger stays private. After pasting the Coolify snippet, SPA HTML must also send O3 headers (`X-Frame-Options: DENY`, explicit CSP).

Ledger and API `/ready` must return 200 before accepting transactional voting. Coolify checks ledger `/live` and API `/health` for process liveness; ledger `/health` is an alias of readiness and may return 503 during catch-up or provider/database failure. The private writer's diagnostics report each chain's height, network tip, lag, observation age, canonical/completeness flags and degradation reason, plus intent heartbeat/backlog/age. A failed heartbeat blocks readiness; an individual retrying intent does not halt otherwise healthy chains. Live `/v1/balances` reports registration/pending state and `snapshot_eligible`; future BSC baselines remain ineligible until the finalized cursor reaches their verified block. See [L14–L17](LEDGER_INVARIANTS.md).

For the current roughly 0.45-second BSC block cadence, set production `POLL_INTERVAL_MS=1000` and verify measured chain lag after deployment. The configured 12000ms interval found during live diagnosis consumes about 27 blocks between 32-block chunks, leaving little time for RPC and accounting. Eight bounded concurrent historical-header reads reduce repeated network checks while preserving the pinned provider, complete validation, ordered hashes and pre/post end-hash checks. The policy remains 15 confirmations plus at most two blocks of additional lag; the poll interval's validated range remains 100–60000ms. See [ledger regression evidence](../ledger/tests/hardening.rs) and [agent workflow](../skills/AGENTS_LEDGER_HARDENING.md).

Test each candidate BSC provider from the deployment network for chain56, fresh head/header, a CL8Y-filtered `eth_getLogs` request at one finalized block, and a bound `eth_call`. A successful balance call alone cannot certify forward indexing: [BNB Chain documents disabled logs on its public dataseed endpoints](https://docs.bnbchain.org/bnb-smart-chain/developers/json_rpc/json-rpc-endpoint/). Record only sanitized capability/latency/lag results; configured credentials stay in OpenBao and provider messages/URLs stay out of worker logs. Static `provider_class` labels distinguish RPC limits/method errors, transport/schema failures and canonicality failures.

Configure independent capable LCD providers for complete-observation failover. The worker refuses a fresh-looking head below the committed cursor after canonical rewind and tries the next provider; all-behind providers leave accounting/checkpoint freshness unchanged. Mutable latest-block and transaction-search requests ask gateways to revalidate cached data. A gateway ignoring these headers must still pass physical transaction-manifest, network, height/time and canonical hash checks. After deployment, sample readiness and safe provider classes across an extended window; a single healthy sample does not establish sustained progress. See [snapshot admission invariants](LEDGER_INVARIANTS.md#l15--snapshot-admission-38) and [HTTP-to-Postgres regressions](../ledger/tests/hardening.rs).

Successful commits can still be unready when RPC latency lets the head advance beyond the existing lag budget during accounting. BSC range planning now uses its own pinned fresh head, shares canonical header validation with log checks, and combines the final same-source head/end checks after SQL. A controlled 90ms-RPC/450ms-block regression retains actual final tips at lag 16–17 across five polls. Successful polls that advance but remain behind yield and immediately catch up instead of sleeping the idle interval; failures retain backoff. Verify sustained deployed lag/readiness rather than assuming these controlled timings match every provider. Policy remains unchanged. See [invariants](LEDGER_INVARIANTS.md) and [agent verification workflow](../skills/AGENTS_LEDGER_HARDENING.md).

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
| MetaMask | BSC 56 | Legal → register → vote (propose if ≥1000 BEP-20). Unsigned Chrome/Safari: Accept opens the portal (`account=` via clickwrap >= 0.1.1); voting also shows Open in MetaMask / Copy link ([#16](https://gitlab.com/PlasticDigits/voting/-/issues/16)). Portal phone next-steps are live ([cl8y-ecosystem-legal#15](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal/-/issues/15), closed). |
| Cosmos WalletConnect | Terra mobile | Galaxy Station / Keplr / Station / Cosmostation / LuncDash: Open/Copy sheet or desktop QR — not hung Connecting... ([#12](https://gitlab.com/PlasticDigits/voting/-/issues/12), WC-M1–M12) |
| BSC WalletConnect | BSC 56 | wagmi QR/modal → `0x…` on chain 56. Cancel must clear EVM connecting. |

Wrong-chain MetaMask (not 56) and missing `signArbitrary` must show the existing readable errors. Do not ask for seeds.

## 4. POST rate limits (in-tree, required before public expose)

`operator-voting` uses a bounded token bucket keyed by verified client IP and
transactional database quotas keyed by authenticated signed actors/actions.

| Env | Prod default | Meaning |
|-----|--------------|---------|
| `RATE_LIMIT_POST_PER_MINUTE` | `60` | POST quota per IP |
| `RATE_LIMIT_POST_BURST` | `20` | Burst tokens |
| `RATE_LIMIT_TRUST_FORWARDED` | `false` | Enable only with verified `RATE_LIMIT_TRUSTED_PROXY_CIDRS` |

`RUN_MODE=prod` refuses to start if the quota is `0` **or** `APPLY_MIGRATIONS` is true. GET `/health` is unlimited. The peer quota is per process; production requires `VOTING_API_REPLICAS=1`. Authenticated actor quotas are shared through PostgreSQL. A 429 includes `Retry-After`.

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

Expect some `422` (invalid body), many `429` with `Retry-After`, and `GET /health` still 200. Investigate proxy failures rather than treating them as successful API responses. Use a small isolated maintenance quota for release edge verification instead of a public abuse burst.

## 5. LocalTerra / LCD equality (optional)

LocalTerra on this workstation is typically `http://127.0.0.1:1317`. Hardening loop:

1. Mint TCL8Y to a test wallet (DEX LocalTerra fixtures — do not invent a new CW20).
2. Register via `operator-voting` with native `TERRA_RPC_URLS` configured. The ledger records an actual same-provider balance/height/hash observation (L1); an LCD header guess is insufficient. Confirm `CL8Y_TOKEN_ADDRESS` is the pinned hpax3 contract (wallet ticker **CL8Y-cb** is that contract unless another is proven).
3. Transfer TCL8Y; wait for the ledger poller.
4. Compare raw integer balances exactly at matching certified heights/hashes. The initial balance must equal the registration observation; after the transfer, the caught-up API balance must equal the fixture’s expected delta and corresponding live observation. Do not compare a historical initial balance with a later live balance.
5. Confirm writer `/ready` reports progressing Terra/BSC checkpoints and fresh intent progress; the registered wallet must become `snapshot_eligible` after its verified baseline is indexed under finality policy.

Do not close [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) / [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) on a screenshot of “Registered” if `GET /v1/balances` is still `"0"` while LCD is not.

Do not close [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14) on a screenshot of Registering… or of the pending banner. #9 is LCD equality **after registered**; #14 is **never leaving pending**. Registration intents with healthy native Terra RPC/BSC providers must complete without manual DB edits. Writer `/ready` must then pass canonical admission; `/live`200 alone is insufficient.

Integration coverage without LocalTerra: [`../ledger/tests/ledger_integration.rs`](../ledger/tests/ledger_integration.rs) (`register_transfer_balance_at_and_no_backfill`) and [`../operator-voting/tests/api_flow.rs`](../operator-voting/tests/api_flow.rs) (`default_balance_clamps_when_tip_lags_register`). Live LCD equality remains an ops check.

## 6. Live probe (2026-09-01)

Public checks only (no wallet identifiers, no Coolify rebuild from this repo):

| Check | Result |
|-------|--------|
| `GET https://vote.cl8y.com/vote` | **200** HTML, `X-Frame-Options: DENY`, explicit CSP (`frame-src` WalletConnect, `connect-src` includes `https://api.web3modal.org`, **no** `connect-src https:`). O3 headers are on the live edge. |
| Live JS `index-cN5sRfvA.js` (HTML last-modified 2026-08-31 05:05 UTC, **before** !11) | Still ships `Chrome and Safari cannot finish terms…`. In-tree `LEGAL_EVM_INAPP_HINT` is `Accept opens the Legal page…`. Rebuild is [#17](https://gitlab.com/PlasticDigits/voting/-/issues/17). |
| `GET https://operator.vote.cl8y.com/health` | **200** `{ ok, service: operator-voting }` |
| Unregistered `GET /v1/balances/terra1…` | Includes OV-B6 keys `registered`, `pending`, `as_of_height`, `initial_balance` (Terra tip `30162104`; BSC sample `as_of_height: 0`) |
| `GET /openapi.json` | Draft lifecycle paths present (`/comments`, `/analysis`, `/open`, `/sections`) |
| `GET /v1/proposals` | `[]` (no staging drafts) |
| `https://ledger.vote.cl8y.com/health` | Proxy **503** `no available server` (HTTPS cert also untrusted). Writer stays private; a 503 is not `intents_ok`. Blocks close of [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) / [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14). |

Do not treat this table as Keplr/MetaMask/Galaxy QA. Human wallet paths remain on #7 / #9 / #12 / #16.

Postgres integration tests take `pg_advisory_lock(739001)` ([`../ledger/src/test_lock.rs`](../ledger/src/test_lock.rs)) so parallel `cargo test` binaries cannot `TRUNCATE` each other. Connect uses an 8s timeout and retries. If `LEDGER_TEST_DATABASE_URL` is set, setup **fails** on connect/migrate errors instead of skipping as green. Local compose uses **host-network** `PGPORT=5433` so sqlx is not stuck behind docker-proxy (TCP accept, no Postgres handshake).

## Historical remaining checks (2026-09-01; superseded by release evidence)

These cannot be completed from a repo-only agent:

- Coolify project create / DNS / TLS / secret install (dApp + `operator-voting` are up; ledger writer is private — public `ledger.vote.cl8y.com` is still proxy 503)
- Coolify **frontend rebuild from `main` (`f000040` or later)** so live JS includes `Accept opens the Legal page…` ([#17](https://gitlab.com/PlasticDigits/voting/-/issues/17)). O3 CSP / `X-Frame-Options` **are** already on live `GET /vote` as of 2026-09-01; re-paste [`../deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf) only if a future nginx edit drops them
- Legal admin property + CORS + portal allowlist write (done on [cl8y-ecosystem-legal#12](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal/-/issues/12))
- Keplr / MetaMask / Galaxy Station / BSC WalletConnect on a real staging origin — still blocked on [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) (LCD equality after **registered**) and writer health ([#14](https://gitlab.com/PlasticDigits/voting/-/issues/14)). Do not close [#12](https://gitlab.com/PlasticDigits/voting/-/issues/12) on Playwright hatch E2E.
- Coolify frontend build-arg `VITE_WC_PROJECT_ID` set; WalletConnect Cloud project includes `https://vote.cl8y.com`
- GitLab CI minutes (`ci_quota_exceeded` on !11) — restore so the next MRs are not merge-blind ([#17](https://gitlab.com/PlasticDigits/voting/-/issues/17))


## Hardening release for Forgejo issues 36–43

The current security work is tracked on [36](https://git.cl8y.com/code/voting/issues/36),
[37](https://git.cl8y.com/code/voting/issues/37), [38](https://git.cl8y.com/code/voting/issues/38),
[39](https://git.cl8y.com/code/voting/issues/39), [40](https://git.cl8y.com/code/voting/issues/40),
[41](https://git.cl8y.com/code/voting/issues/41), [42](https://git.cl8y.com/code/voting/issues/42),
and [43](https://git.cl8y.com/code/voting/issues/43). Reported baseline defects and eventual
release results are distinct: never use a successful HTTP liveness check as chain readiness.

Run [`scripts/verify-release.sh`](../scripts/verify-release.sh) from the release worktree.
It creates a disposable, loopback-only PostgreSQL container and uniquely named
`voting_test_*` database, explicitly opts into destructive integration tests, runs Rust,
frontend, SPA and secret checks, and removes only its own container on exit. It ignores
inherited application database configuration. Each failed attempt gets a new database
on the next invocation; production ledger history must never be reset as a test fix.
The all-zero WalletConnect project ID in that local compilation is a build fixture,
not production enrollment or wallet verification.

Manual isolated compose uses database `voting_test_local` and requires both
`LEDGER_TEST_DATABASE_URL=postgres://voting:voting@127.0.0.1:5433/voting_test_local` and
`LEDGER_TEST_ALLOW_DESTRUCTIVE=1`. Required CI additionally sets `LEDGER_TEST_REQUIRED=1`;
its dedicated `postgres` service is admitted only with `CI=true` and
`LEDGER_TEST_ALLOW_CI_SERVICE=1`. Missing or unsafe configuration must fail before DDL.
Never substitute `DATABASE_URL` for the test URL.

Before deploying, merge the verified worktree with current origin/main and rerun checks
when that merge changes implementation. Open a reviewable PR, publish the tested commits,
and deploy the ledger migrations before the restricted API and frontend. Preserve prior
frozen proposals; historical hash-only signatures cannot be represented as complete
signed receipts. Follow the documented per-table grants and rotate credentials through
OpenBao without printing them.

Use `bao-exec --only COOLIFY_WIRE_TOKEN,COOLIFY_URL -- ...` for the existing Coolify
control plane. Workstation HTTP proxies can intercept private addresses; private API
clients must bypass ambient proxies explicitly. Record target resource IDs, exact deployed
commit and terminal deployment status. Never interpret queued/building as deployed.
Run live GET/readiness, allowed and disallowed PUT preflights, stable error checks, and SPA
security-header probes. Avoid destructive fixtures or production abuse bursts.

Grafana at `https://logs.cl8y.com` provisions the `Loki` datasource. The authenticated
read path is `/api/datasources/proxy/uid/Loki/loki/api/v1/query_range`; direct public Loki
query paths are blocked. Fetch read credentials into process memory via the authorized
OpenBao identity, and retain only sanitized release evidence. Query
`{service="voting-ledger"}` plus the named API/frontend services over the deployment window.
A datasource inventory containing only Loki does not establish Prometheus coverage.
Never emit provider credential URLs or raw secret-bearing deployment/environment dumps.

Close an issue only after its body and all comments have been checked against actual
results. Unavailable live checks remain unchecked and the issue remains open. Third-party
agent guidance and release evidence must link each issue to its implementing commits.

Combined issue 36–43 implementation and verification evidence is maintained in
[`verification/2026-10-14-issues-36-43.md`](verification/2026-10-14-issues-36-43.md).


Release control uses [`scripts/coolify-release.py`](../scripts/coolify-release.py)
under `bao-exec --only COOLIFY_WIRE_TOKEN,COOLIFY_URL -- python3 ...`. It validates
resource identity and the Forgejo source, accepts only a complete fetched `origin/main`
commit, pins that commit before deployment, and emits safe deployment IDs/statuses.
Stop the old ledger **and** old operator before migrations: the old writer lacks the
new accounting locks. Configure explicit one-time baseline recovery on the ledger,
deploy it, and verify canonical epochs; then clear the recovery flag. Deploy the
operator and frontend from the same commit and run
[`scripts/live-smoke.py`](../scripts/live-smoke.py). The baseline recovery flag remains
false normally and cannot silently authorize future history skips.

Ledger container/proxy health probes use `/live`. Ledger `/ready` and `/health`
report canonical readiness and may return503 during catch-up or provider failure;
using either as a restart probe would repeatedly kill the independent workers.
The operator's `/health` remains liveness and `/ready` reports ledger readiness.

The 2026-10-14 inspection found 15 historical registrations and two BSC event and
balance rows in `voting`, with empty canonical `public` electorate/history tables.
See the [bounded live inventory](verification/2026-10-14-live-storage-inventory.json).
Forward migration `20261014000105` archives and imports that legacy namespace,
preserves its original tables and the canonical migration tracker, rejects unequal
collisions, and requires explicit reconciliation. A higher empty public cursor is
not evidence that the legacy electorate can be discarded. Keep API and frontend
stopped until the import and both canonical baseline epochs have been verified.

Current configured resources are ledger `s6j6fx1870zbsmhbu2do1yiz`, operator
`x5j1g2xuc5p508g26ff003q0`, frontend `svvxk9gd93d8kwezvcrekdq0`.
The frontend uses the repository Dockerfile and root build context with Coolify
`is_static=false` (a static wrapper otherwise looks for nonexistent `/app/dist`
inside the final nginx image). The release helper verifies this mode and the
ledger `/live` health probe before queuing a build. Forwarded-header
trust is enabled only for the observed voting ingress peer `10.0.17.2/32`, with
`VOTING_API_REPLICAS=1`. The [dated live probe](verification/2026-10-14-live-proxy-quota.json)
proved forged incoming forwarding headers share the same quota bucket and recover
after `Retry-After`. Keep the normal quota at 60/minute with burst20. Revalidate
the peer and header replacement after proxy/network changes; never broaden this
trust to an entire private subnet. Database
identity additionally requires comment `cl8y-voting-ephemeral-test` before destructive
DDL. The marker must be installed only by initialization of a dedicated test database.

Production LCD selection is `TERRA_LCD_URL=https://api-lunc-lcd.binodes.com,https://terra-classic-lcd.publicnode.com`.
Set this existing variable: it takes precedence over `LCD_URLS`. The primary
passed the existing Rust adapter and deployment-network checks recorded in
[provider evidence](verification/2026-10-14-terra-provider-capabilities.json).
Keep native `TERRA_RPC_URLS` for bound balance observations. PublicNode's sole-LCD
configuration showed periodic head/search inconsistencies; longer live readiness
sampling is mandatory after provider/transport changes. Preserve all origin
completeness, hash, freshness and finality refusals while correcting selection.
