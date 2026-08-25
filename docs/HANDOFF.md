# Agent handoff

**Last updated:** 2026-08-25  
**Repo:** [PlasticDigits/voting](https://gitlab.com/PlasticDigits/voting)  
**Local:** `~/repos/voting`

## Current state

Bootstrap only. There is no ledger service, no `operator-voting` binary, and no voting frontend in this tree.

GitLab issues **509 / 510 / 511 / 588** were moved here from `PlasticDigits/cl8y-dex-terraclassic`. Their descriptions still mention DEX paths (`indexer/`, `frontend-dapp/`). Those paths are **design references**. Implement the services **in this repo**.

Issue mapping: [`ISSUE_MIGRATION.md`](ISSUE_MIGRATION.md).

## Why a new repo

Voting is a separate product surface (dual-chain electorate, its own Postgres roles, its own Coolify service). Shipping it inside the DEX indexer/dApp would couple analytics ingest to a write-heavy signature API and would pull EVM wallets into a Cosmos-only app.

## What to build (in order)

1. **Ledger (data plane)** — former DEX #509, plus BSC from #588  
   Postgres registration + CL8Y CW20 transfer ledger + BEP-20 Transfer log ledger for **registered** wallets. Live LCD `Balance` / `balanceOf` at register time. No archive queries. No backfill before registration.

2. **`operator-voting` (control plane)** — former DEX #510, plus EIP-191 from #588  
   Restricted DB role. ADR-36 **and** EIP-191. Proposals, votes, `VOTING_BLACKLIST_ADDRESSES`, snapshot freeze `{ terra_height, bsc_block }`. ≥1000 CL8Y to propose (per registering address’s chain).

3. **Voting dApp** — former DEX #511, plus EVM UX from #588  
   `/vote` register / WYSIWYG propose / vote. **Blocked behind CL8Y Legal.** Terra connect from DEX; EVM connect from Bridge.

Do not mark production voting “done” while BSC holders cannot register and vote.

## Constraints added at repo split (2026-08-25)

These were **not** in the original DEX issue text. They are now required:

| Constraint | Detail |
|------------|--------|
| Legal | Voting UI must fail closed behind [cl8y-ecosystem-legal](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal). Use `@plasticdigits/cl8y-clickwrap`. Register a dedicated property (proposed `vote.cl8y.com` — confirm before admin write). Terra **and** EVM networks. Skill: [`../skills/AGENTS_LEGAL_CLICKWRAP.md`](../skills/AGENTS_LEGAL_CLICKWRAP.md). |
| Wallets | For wallet connecting software, as there has been many problems with it, the terraclassic connections on cl8y dex and the evm connections on cl8y bridge are the most well functional. Port those implementations. Skill: [`../skills/AGENTS_WALLET_CONNECTORS.md`](../skills/AGENTS_WALLET_CONNECTORS.md). |

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

See [`.env.example`](../.env.example). Production compose must use a **restricted** `DATABASE_URL` for `operator-voting` (SELECT on balance views + R/W signature/proposal/vote tables only).

## Legal ops (separate repo, often a separate MR)

From a `cl8y-ecosystem-legal` checkout, after the hostname is decided:

1. Register the property on the Legal admin API (interactive token — not `ADMIN_TOKEN` in this repo).
2. Add the voting origin to Legal `CORS_ORIGINS`.
3. Add the voting origin to portal `VITE_REDIRECT_URI_ALLOWLIST`.

Mirror the DEX playbook in `cl8y-dex-terraclassic/skills/AGENTS_FRONTEND_CLICKWRAP.md` (invariants C1–C10), with **two** networks: `TerraClassic` and `EVM`.

## Verification when code exists

Until packages land, verification is docs + GitLab mapping only. After implementation, expect:

- Ledger unit + Postgres integration (Terra fixtures + mocked BSC)
- `operator-voting` sig / blacklist / privilege tests
- Frontend vitest + Playwright with Legal skip hatch (`VITE_PLAYWRIGHT_E2E`) **off** for production builds
- Manual: Keplr Terra register/vote **and** MetaMask BSC register/vote, each after Legal accept
