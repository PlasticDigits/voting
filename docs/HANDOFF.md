# Agent handoff

**Last updated:** 2026-08-30  
**Repo:** [PlasticDigits/voting](https://gitlab.com/PlasticDigits/voting)  
**Local:** `~/repos/voting`  
**Implementation branch:** `feat/voting-bundle` (merged). Ops follow-up: [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) · [`OPS.md`](OPS.md). Legal `/vote` 404 ([#8](https://gitlab.com/PlasticDigits/voting/-/issues/8)) is closed on live 200. Registered balance 0: [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9). Structured proposals: [#10](https://gitlab.com/PlasticDigits/voting/-/issues/10) · OV-S in [`OPERATOR_VOTING.md`](OPERATOR_VOTING.md).

## Current state

In-tree packages:

| Package | Role |
|---------|------|
| `ledger/` | Terra CW20 + BSC BEP-20 registration ledger |
| `operator-voting/` | ADR-36 + EIP-191 API |
| `frontend/` | dApp (`/`, `/new`, `/:id`; `/vote*` aliases), Legal gate, DEX Terra + Bridge EVM wallets |

GitLab issues **#1–#6** are implemented in this tree. [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) tracks remaining ops. [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) is **closed** on live `GET /vote` 200 HTML (`https://vote.cl8y.com`). [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) (registered holders shown as 0) is in-tree: default GET `/v1/balances` clamps to `registered_at_height` (OV-B1), pending register UX, BSC analogue; live LCD equality still blocks close. [#10](https://gitlab.com/PlasticDigits/voting/-/issues/10) (structured proposal template) is in-tree: `body_sections` JSONB, canonical section hash (OV-S), labeled compose / list TL;DR / detail sections. Close #10 after staging Terra **and** BSC compose a templated proposal. In-tree for #7: POST IP/QPS limits, Coolify Dockerfiles, prod refuses migrate + zero quota, `deploy/grants.sql` (USAGE, `current_database()`), Legal-hatch/O4 build guards, nginx CSP on **both** the Dockerfile image and the Coolify static paste. Live: dApp `https://vote.cl8y.com` (nginx 1.31.x, SPA fallback 200), API `https://operator.vote.cl8y.com` (CORS + parallel POST 429). Still human/ops: re-paste [`../deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf) so live HTML sends O3 CSP/`X-Frame-Options`; ledger writer stays private; Keplr+MetaMask QA blocked on #9. Runbook: [`OPS.md`](OPS.md). Skills: [`../skills/AGENTS_OPS_STAGING.md`](../skills/AGENTS_OPS_STAGING.md) · [`../skills/AGENTS_LEGAL_CLICKWRAP.md`](../skills/AGENTS_LEGAL_CLICKWRAP.md) · [`../skills/AGENTS_VOTING_BUNDLE.md`](../skills/AGENTS_VOTING_BUNDLE.md) · [`../skills/AGENTS_PROPOSAL_TEMPLATE.md`](../skills/AGENTS_PROPOSAL_TEMPLATE.md).

Issue mapping: [`ISSUE_MIGRATION.md`](ISSUE_MIGRATION.md). Invariants: [`LEDGER_INVARIANTS.md`](LEDGER_INVARIANTS.md), [`OPERATOR_VOTING.md`](OPERATOR_VOTING.md), [`FRONTEND.md`](FRONTEND.md), [`OPS.md`](OPS.md). #7 in-tree extras: prod refuses `APPLY_MIGRATIONS=true`, privilege tests apply [`../deploy/grants.sql`](../deploy/grants.sql), frontend CSP snippet, `prodEnvGuards` (O3/O4), CI `test:rust-integration`. #8 in-tree extras: [`../frontend/src/routes.ts`](../frontend/src/routes.ts), [`../deploy/docker/frontend.nginx.conf`](../deploy/docker/frontend.nginx.conf), CI `test:frontend-spa-fallback`. #9 in-tree: [`../operator-voting/src/balance_query.rs`](../operator-voting/src/balance_query.rs), pending poll in the dApp, ledger `/health.caught_up`. #10 in-tree: [`../operator-voting/src/proposal_sections.rs`](../operator-voting/src/proposal_sections.rs), [`../frontend/src/utils/proposalSections.ts`](../frontend/src/utils/proposalSections.ts), migration `20260830000001_proposal_sections.sql`.

## Why a new repo

Voting is a separate product surface (dual-chain electorate, its own Postgres roles, its own Coolify service). Shipping it inside the DEX indexer/dApp would couple analytics ingest to a write-heavy signature API and would pull EVM wallets into a Cosmos-only app.

## What was built (order)

1. **Ledger (data plane)** — #1 + BSC from #4  
   Postgres registration + CL8Y CW20 transfer ledger + BEP-20 Transfer log ledger for **registered** wallets. Live LCD `Balance` / `balanceOf` at register time. No archive queries. No backfill before registration.

2. **`operator-voting` (control plane)** — #2 + EIP-191 from #4  
   Restricted DB role. ADR-36 **and** EIP-191. Proposals, votes, `VOTING_BLACKLIST_ADDRESSES`, snapshot freeze `{ terra_height, bsc_block }`. ≥1000 CL8Y to propose (per registering address’s chain).

3. **Voting dApp** — #3 with #5 + #6 + #8 + #10  
   `/` register / templated propose / vote (`/vote*` aliases). **Blocked behind CL8Y Legal.** Terra connect from DEX; EVM connect from Bridge. SPA documents must be 200 HTML on hard navigation (Legal return). Compose uses labeled sections; the API rejects freeform `body_html` on create.

Do not mark production voting “done” while BSC holders cannot register and vote on staging with real wallets.

## Constraints added at repo split (2026-08-25)

| Constraint | Detail |
|------------|--------|
| Legal | Voting UI must fail closed behind [cl8y-ecosystem-legal](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal). Use `@plasticdigits/cl8y-clickwrap`. Register a dedicated property (proposed `vote.cl8y.com` — confirm before admin write). Terra **and** EVM networks. Skill: [`../skills/AGENTS_LEGAL_CLICKWRAP.md`](../skills/AGENTS_LEGAL_CLICKWRAP.md). |
| Wallets | Port DEX Terra Classic + Bridge EVM stacks. Skill: [`../skills/AGENTS_WALLET_CONNECTORS.md`](../skills/AGENTS_WALLET_CONNECTORS.md). |

## Do not

- Implement voting routes or ledger tables inside `cl8y-dex-terraclassic`.
- Overload the DEX Venus vFDUSD BSC poller for CL8Y Transfer logs.
- Put `ADMIN_TOKEN` or Legal admin credentials in the voting frontend.
- Ask users for seeds / mnemonics.
- Treat Legal portal EVM sign (`https://terms.cl8y.com/sign/evm`) as voting auth. Legal is terms acceptance only.
- Use fee-discount tiers, LP balances, or DEX `trader_positions` as vote weight.
- Sum Terra + BSC balances on **different** addresses into one voter.

## Tokens

- Terra CW20 CL8Y: `terra16wtml2q66g82fdkx66tap0qjkahqwp4lwq3ngtygacg5q0kzycgqvhpax3`
- BSC BEP-20 CL8Y: `0x8F452a1fdd388A45e1080992eFF051b4dd9048d2`
- Store raw 18-decimal integers as strings / `NUMERIC`.

## Env sketch

See [`.env.example`](../.env.example). Production compose must use a **restricted** `DATABASE_URL` for `operator-voting` (EXECUTE on balance functions + R/W signature/proposal/vote tables only). Grant SQL: [`../deploy/grants.sql`](../deploy/grants.sql).

## Legal ops (separate repo, often a separate MR)

From a `cl8y-ecosystem-legal` checkout, after the hostname is decided:

1. Register the property on the Legal admin API (interactive token — not `ADMIN_TOKEN` in this repo).
2. Add the voting origin to Legal `CORS_ORIGINS`.
3. Add the voting origin to portal `VITE_REDIRECT_URI_ALLOWLIST`.

## Verification

```bash
# unit
cargo test --workspace --lib
cd frontend && npm test

# postgres integration (docker compose -f docker-compose.test.yml up -d)
LEDGER_TEST_DATABASE_URL=postgres://voting:voting@127.0.0.1:5433/voting cargo test --workspace --tests

# Playwright (Legal hatch on webServer only)
cd frontend && npm run test:e2e

# nginx SPA fallback (issue #8 / O8)
sh deploy/docker/test-frontend-spa-fallback.sh
```

Still manual / staging: Keplr Terra register/vote **and** MetaMask BSC register/vote, each after Legal accept. Close #8 only after live `curl -sI https://vote.cl8y.com/vote` is 200.
