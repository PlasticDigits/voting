# Agent handoff

**Last updated:** 2026-08-31  
**Repo:** [PlasticDigits/voting](https://gitlab.com/PlasticDigits/voting)  
**Local:** `~/repos/voting`  
**Implementation branch:** `feat/voting-bundle` (merged). Ops: [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7). Legal `/vote` 404 [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) is closed. Registered balance: [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9). Structured proposals: [#10](https://gitlab.com/PlasticDigits/voting/-/issues/10) (OV-S). Draft lifecycle: [#11](https://gitlab.com/PlasticDigits/voting/-/issues/11) (OV-D). WalletConnect: [#12](https://gitlab.com/PlasticDigits/voting/-/issues/12). Governance research: [#13](https://gitlab.com/PlasticDigits/voting/-/issues/13). Pending registration: [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14). EVM Legal in-app hint: [#16](https://gitlab.com/PlasticDigits/voting/-/issues/16).

## Current state

In-tree packages:

| Package | Role |
|---------|------|
| `ledger/` | Terra CW20 + BSC BEP-20 registration ledger |
| `operator-voting/` | ADR-36 + EIP-191 API |
| `frontend/` | dApp (`/`, `/new`, `/:id`; `/vote*` aliases), Legal gate, DEX Terra + Bridge EVM wallets |

GitLab issues **#1–#6** are implemented in this tree. [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) tracks remaining ops. [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) is **closed** on live `GET /vote` 200 HTML. [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) adds OV-B1 registered-balance handling. [#10](https://gitlab.com/PlasticDigits/voting/-/issues/10) adds structured `body_sections`, the OV-S canonical hash, labeled compose, list TL;DR, and detail sections. [#11](https://gitlab.com/PlasticDigits/voting/-/issues/11) adds draft/comment/analysis/open APIs and OV-D freeze-at-open. [#12](https://gitlab.com/PlasticDigits/voting/-/issues/12) adds the cosmes pairing patch, hook before `createRoot`, production project-id fail-closed, CSP frames, and EVM Cancel. [#13](https://gitlab.com/PlasticDigits/voting/-/issues/13) is governance research only. [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14) adds 120-second pending poll/retry, the independent ledger intent loop, L12, and OV-B6. [#16](https://gitlab.com/PlasticDigits/voting/-/issues/16) adds the EVM Legal in-app/copy hint; published clickwrap **>= 0.1.1** puts `account` on Accept (Legal #15 portal next-steps are **closed**).

Issue mapping: [`ISSUE_MIGRATION.md`](ISSUE_MIGRATION.md). Invariants: [`LEDGER_INVARIANTS.md`](LEDGER_INVARIANTS.md) (L12), [`OPERATOR_VOTING.md`](OPERATOR_VOTING.md) (OV-B6, OV-S, OV-D), [`FRONTEND.md`](FRONTEND.md) (L-EVM1–L-EVM5, [#16](https://gitlab.com/PlasticDigits/voting/-/issues/16)), [`OPS.md`](OPS.md). Hybrid governance doctrine (do not invent a model): [`GOVERNANCE_RESEARCH.md`](GOVERNANCE_RESEARCH.md) (issue [#13](https://gitlab.com/PlasticDigits/voting/-/issues/13)). #10 owns `20260830000001_proposal_sections.sql`; #11 applies `20260830000002_draft_lifecycle.sql` on top.

## Why a new repo

Voting is a separate product surface (dual-chain electorate, its own Postgres roles, its own Coolify service). Shipping it inside the DEX indexer/dApp would couple analytics ingest to a write-heavy signature API and would pull EVM wallets into a Cosmos-only app.

## What was built (order)

1. **Ledger (data plane)** — #1 + BSC from #4  
   Postgres registration + CL8Y CW20 transfer ledger + BEP-20 Transfer log ledger for **registered** wallets. Live LCD `Balance` / `balanceOf` at register time. No archive queries. No backfill before registration.

2. **`operator-voting` (control plane)** — #2 + EIP-191 from #4  
   Restricted DB role. ADR-36 **and** EIP-191. Drafts, comments, committee `open_vote`, proposals, votes, `VOTING_BLACKLIST_ADDRESSES` / `VOTING_COMMITTEE_ADDRESSES`. Snapshot freeze `{ terra_height, bsc_block }` at **open**, not draft create. ≥1000 CL8Y to start a draft (per registering address’s chain).

3. **Voting dApp** — #3 with #5 + #6 + #8 + #10 + #11 + #16
   `/` register / templated draft / review / committee-open / vote (`/vote*` aliases). Vote CTAs appear only when `status=open`. **Blocked behind CL8Y Legal.** Terra connect from DEX; EVM connect from Bridge. Unsigned EVM on Chrome/Safari: Accept opens the portal (`account=` via clickwrap >= 0.1.1) and the voting hint offers Open in MetaMask + copy ([#16](https://gitlab.com/PlasticDigits/voting/-/issues/16)). Portal phone CTAs are Legal #15 (closed).

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
