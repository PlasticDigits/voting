---
name: voting-ops-staging
description: >-
  Coolify/staging deploy, Legal property/CORS/allowlist, live Keplr+MetaMask QA,
  operator-voting POST rate limits, and optional LocalTerra LCD equality.
  Use when verifying or implementing GitLab voting issue #7 or public expose.
---

# Ops / staging (voting issue #7)

Read [`docs/OPS.md`](../docs/OPS.md) first. In-tree #1–#6 are not enough for production.

This skill is for **3rd-party agents** continuing Coolify, Legal admin, or live wallet QA. Do not invent a fourth deploy path.

## What is already in-tree

| Item | Where |
|------|--------|
| Three Dockerfiles | [`deploy/docker/`](../deploy/docker/) |
| Restricted DB grants | [`deploy/grants.sql`](../deploy/grants.sql) |
| Coolify env sketch | [`deploy/coolify.env.example`](../deploy/coolify.env.example) |
| POST IP/QPS (`governor`, O-RL1–O-RL5) | [`operator-voting/src/rate_limit.rs`](../operator-voting/src/rate_limit.rs) |
| Prod refuses `RATE_LIMIT_POST_PER_MINUTE=0` | [`operator-voting/src/config.rs`](../operator-voting/src/config.rs) |
| Ledger writer owns migrations | `APPLY_MIGRATIONS` false when `RUN_MODE=prod` |
| Legal hatch blocked on prod build | `frontend/vite.config.ts` + frontend Dockerfile |
| Runbook + invariants O1–O7 | [`docs/OPS.md`](../docs/OPS.md) |

## Do

1. Deploy **three** Coolify services. Ledger writer migrates; API uses `operator_voting`.
2. Confirm hostname, then register Legal property in [cl8y-ecosystem-legal](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal) with an interactive admin token. Skill: [`AGENTS_LEGAL_CLICKWRAP.md`](AGENTS_LEGAL_CLICKWRAP.md).
3. Live-QA Keplr Terra **and** MetaMask BSC 56 after Legal accept. Wallet skill: [`AGENTS_WALLET_CONNECTORS.md`](AGENTS_WALLET_CONNECTORS.md).
4. Keep POST rate limits on before public DNS.

## Do not

- Put `ADMIN_TOKEN` or Legal admin credentials in this frontend.
- Set `VITE_PLAYWRIGHT_E2E` or `VITE_DEV_MNEMONIC` on Coolify.
- Set any `VITE_*` BSC RPC for balances.
- Point `operator-voting` `DATABASE_URL` at the ledger writer.
- Run `APPLY_MIGRATIONS=true` on the restricted role.
- Treat Playwright Legal-hatch E2E as live wallet QA.
- Silently merge `terra1…` and `0x…`.
- Close #7 until Coolify, Legal admin, and both live wallets actually pass on staging.

## Verification (in-tree)

```bash
cargo test --workspace --lib
LEDGER_TEST_DATABASE_URL=postgres://voting:voting@127.0.0.1:5433/voting cargo test --workspace --tests
cd frontend && npm test
```

Rate-limit unit tests live in `operator-voting/src/rate_limit.rs` (no Postgres).
