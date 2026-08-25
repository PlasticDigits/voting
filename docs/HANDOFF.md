# Agent handoff

**Last updated:** 2026-08-25  
**Repo:** [PlasticDigits/voting](https://gitlab.com/PlasticDigits/voting)  
**Local:** `~/repos/voting`  
**Implementation branch:** `feat/voting-bundle` (merged). Ops follow-up: [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) · [`OPS.md`](OPS.md).

## Current state

In-tree packages:

| Package | Role |
|---------|------|
| `ledger/` | Terra CW20 + BSC BEP-20 registration ledger |
| `operator-voting/` | ADR-36 + EIP-191 API |
| `frontend/` | `/vote` dApp, Legal gate, DEX Terra + Bridge EVM wallets |

GitLab issues **#1–#6** are implemented in this tree. [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) tracks remaining ops. In-tree for #7: POST IP/QPS limits, Coolify Dockerfiles, prod refuses migrate + zero quota, `deploy/grants.sql` (USAGE, `current_database()`), Legal-hatch/O4 build guards, nginx CSP. Still human/ops: Coolify project, Legal admin property/CORS/allowlist, live Keplr+MetaMask QA. Runbook: [`OPS.md`](OPS.md). Skill: [`../skills/AGENTS_OPS_STAGING.md`](../skills/AGENTS_OPS_STAGING.md).

Issue mapping: [`ISSUE_MIGRATION.md`](ISSUE_MIGRATION.md). Invariants: [`LEDGER_INVARIANTS.md`](LEDGER_INVARIANTS.md), [`OPERATOR_VOTING.md`](OPERATOR_VOTING.md), [`FRONTEND.md`](FRONTEND.md), [`OPS.md`](OPS.md). #7 in-tree extras: prod refuses `APPLY_MIGRATIONS=true`, privilege tests apply [`../deploy/grants.sql`](../deploy/grants.sql), frontend CSP snippet, `prodEnvGuards` (O3/O4), CI `test:rust-integration`.

## Why a new repo

Voting is a separate product surface (dual-chain electorate, its own Postgres roles, its own Coolify service). Shipping it inside the DEX indexer/dApp would couple analytics ingest to a write-heavy signature API and would pull EVM wallets into a Cosmos-only app.

## What was built (order)

1. **Ledger (data plane)** — #1 + BSC from #4  
   Postgres registration + CL8Y CW20 transfer ledger + BEP-20 Transfer log ledger for **registered** wallets. Live LCD `Balance` / `balanceOf` at register time. No archive queries. No backfill before registration.

2. **`operator-voting` (control plane)** — #2 + EIP-191 from #4  
   Restricted DB role. ADR-36 **and** EIP-191. Proposals, votes, `VOTING_BLACKLIST_ADDRESSES`, snapshot freeze `{ terra_height, bsc_block }`. ≥1000 CL8Y to propose (per registering address’s chain).

3. **Voting dApp** — #3 with #5 + #6  
   `/vote` register / WYSIWYG propose / vote. **Blocked behind CL8Y Legal.** Terra connect from DEX; EVM connect from Bridge.

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
```

Still manual / staging: Keplr Terra register/vote **and** MetaMask BSC register/vote, each after Legal accept.
