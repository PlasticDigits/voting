---
name: voting-ops-staging
description: >-
  Coolify/staging deploy, Legal property/CORS/allowlist, live Keplr+MetaMask QA,
  operator-voting POST rate limits, SPA document fallback for /vote (issue #8),
  registered-holder balance reads (issue #9 OV-B1), and optional LocalTerra LCD equality.
  Use when verifying or implementing GitLab voting issues #7, #8, or #9 or public expose.
---

# Ops / staging (voting issue #7)

Read [`docs/OPS.md`](../docs/OPS.md) first. In-tree #1–#6 are not enough for production. [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) is a production-blocking Legal-return 404 when the edge does not SPA-fallback `/vote`.

This skill is for **3rd-party agents** continuing Coolify, Legal admin, or live wallet QA. Do not invent a fourth deploy path. If a registered holder shows **0 CL8Y** after Register, read [OPERATOR_VOTING.md](../docs/OPERATOR_VOTING.md) OV-B1 and issue [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) before touching LCD in the browser.

## What is already in-tree

| Item | Where |
|------|--------|
| Three Dockerfiles | [`deploy/docker/`](../deploy/docker/) |
| Restricted DB grants | [`deploy/grants.sql`](../deploy/grants.sql) (`USAGE` on `voting`; CONNECT uses `current_database()`). Apply with `psql -v ON_ERROR_STOP=1`. Privilege tests apply **this file**. |
| Coolify env sketch | [`deploy/coolify.env.example`](../deploy/coolify.env.example) |
| Coolify static SPA nginx | [`deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf) — paste over Coolify’s `try_files … =404` if the dApp is not the Dockerfile image |
| POST IP/QPS (`governor`, O-RL1–O-RL5) | [`operator-voting/src/rate_limit.rs`](../operator-voting/src/rate_limit.rs) — 429 includes `Retry-After` |
| Prod refuses zero POST quota **and** `APPLY_MIGRATIONS=true` | [`operator-voting/src/config.rs`](../operator-voting/src/config.rs) |
| Ledger writer owns migrations | Image pins `APPLY_MIGRATIONS=false`; prod config refuses true |
| Legal hatch + O4 BSC RPC blocked on prod build | [`frontend/src/utils/prodEnvGuards.ts`](../frontend/src/utils/prodEnvGuards.ts) + frontend Dockerfile |
| Production CSP (`connect-src`, no blanket `https:`) | [`deploy/docker/frontend.security-headers.conf`](../deploy/docker/frontend.security-headers.conf) |
| SPA `try_files` + HEALTHCHECK `/vote` (O8 / #8) | [`deploy/docker/frontend.nginx.conf`](../deploy/docker/frontend.nginx.conf) · [`deploy/docker/frontend.healthcheck.sh`](../deploy/docker/frontend.healthcheck.sh) · CI `test:frontend-spa-fallback` |
| Flattened dApp routes + `/vote*` aliases | [`frontend/src/routes.ts`](../frontend/src/routes.ts) · [`docs/FRONTEND.md`](../docs/FRONTEND.md) |
| Runbook + invariants O1–O8 | [`docs/OPS.md`](../docs/OPS.md) |
| Default GET balance clamp (OV-B1, #9) | [`operator-voting/src/balance_query.rs`](../operator-voting/src/balance_query.rs) · ledger `/health.caught_up` |

## Do

1. Deploy **three** Coolify services. Ledger writer migrates; API uses `operator_voting`. Frontend: Dockerfile (SPA `try_files` already in-image) or static + [`deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf). Coolify’s default `=404` breaks Legal/WC redirects.
2. Confirm hostname, then register Legal property in [cl8y-ecosystem-legal](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal) with an interactive admin token. Skill: [`AGENTS_LEGAL_CLICKWRAP.md`](AGENTS_LEGAL_CLICKWRAP.md).
2b. When applying grants, `GRANT EXECUTE` must name `public.voting_*_balance_at`. The writer role is often `voting` and schema `voting` exists, so unqualified names follow `"$user", public` and miss the SECURITY DEFINER originals (L10).
3. Live-QA Keplr Terra **and** MetaMask BSC 56 after Legal accept. Wallet skill: [`AGENTS_WALLET_CONNECTORS.md`](AGENTS_WALLET_CONNECTORS.md).
4. Keep POST rate limits on before public DNS.
5. Prove SPA fallback (O8 / #8): Coolify dApp service uses `frontend.Dockerfile` / `frontend.nginx.conf`, or stamp `try_files $uri /index.html` on the live nginx. `curl -sI https://vote.cl8y.com/vote` and `/new` must be 200 HTML before Legal Accept QA.

## Do not

- Put `ADMIN_TOKEN` or Legal admin credentials in this frontend.
- Set `VITE_PLAYWRIGHT_E2E` or `VITE_DEV_MNEMONIC` on Coolify.
- Set any `VITE_*` BSC RPC for balances.
- Point `operator-voting` `DATABASE_URL` at the ledger writer.
- Run `APPLY_MIGRATIONS=true` on the restricted role.
- Treat Playwright Legal-hatch E2E as live wallet QA.
- Silently merge `terra1…` and `0x…`.
- Close #7 until Coolify, Legal admin, and both live wallets actually pass on staging.
- Close #8 while `curl -sI https://vote.cl8y.com/vote` is 404. Flattening `/` is not a substitute for edge `try_files` on `/new` and `/vote/:id`.
- Use `error_page 404 = /index.html` to hide missing SPA fallback (caches 404).
- Close #9 until a registered Terra (and BSC) holder with ≥1000 pinned CL8Y sees a non-zero `GET /v1/balances` that matches LCD/`balanceOf` and the UI badge. “Registered” plus balance `"0"` is not done.
- Query CW20 `Balance` or `eth_call` from the voting frontend (`VITE_*` LCD/RPC).

## Verification (in-tree)

```bash
cargo test --workspace --lib
LEDGER_TEST_DATABASE_URL=postgres://voting:voting@127.0.0.1:5433/voting cargo test --workspace --tests
cd frontend && npm test
sh deploy/docker/test-frontend-spa-fallback.sh
```

SPA fallback (O8): `GET /vote` and `GET /new` must be 200 HTML; `GET /assets/missing.js` must be 404. Live check: `curl -sI https://vote.cl8y.com/vote`.

Rate-limit unit tests live in `operator-voting/src/rate_limit.rs` (no Postgres). CI also runs the Postgres integration job (`test:rust-integration` in [`.gitlab-ci.yml`](../.gitlab-ci.yml)).
