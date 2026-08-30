---
name: voting-issue-bundle
description: >-
  Offchain CL8Y snapshot voting issue bundle (ledger, operator-voting, dApp,
  BSC core electorate). Use when implementing or triaging voting issues.
---

# Voting issue bundle

Read [`docs/ISSUE_MIGRATION.md`](../docs/ISSUE_MIGRATION.md) for DEX IID → this-repo IID.

Original write-ups assumed implementation **inside** `cl8y-dex-terraclassic`. Ignore that layout. Implement here.

## Order

1. Ledger (DEX #509) including BSC tables (DEX #588)
2. `operator-voting` (DEX #510) including EIP-191 + dual snapshot
3. dApp (DEX #511) including EVM connect, **Legal gate**, dual wallet stacks
4. Treat #588 as the BSC/EVM **contract** (guardrails), not a later phase

dApp UI can stub the API with MSW in parallel, but production “done” requires BSC + Legal.

## Shared invariants

- No new contracts; no archive node; no pre-registration backfill
- Registered-set transfer indexing only
- 18-decimal raw amounts
- Propose threshold ≥1000 CL8Y on **that** address’s chain
- Blacklist: Terra bech32 **and** normalized `0x` (case-insensitive)
- Snapshot weight, not tip, after create
- Flash-in before snapshot is counted by design (document; not a bug)
- Default GET `/v1/balances` for a registered wallet uses `max(tip, registered_at_height)` (OV-B1, [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9)). Do **not** “fix” a 0 badge with browser LCD/`eth_call`. Unregistered 0 is not a live holding.
- Registration pending is not Registered (POST `/v1/register` `ok` is an intent). Do not swallow `pending_intents` SQL errors as an empty queue (L12). Do not leave Registering… without Retry ([#14](https://gitlab.com/PlasticDigits/voting/-/issues/14)). Do not close #9 instead of #14.

## Implementation (landed in-tree)

| Issue | Code |
|-------|------|
| #1 + #4 ledger | [`ledger/`](../ledger/) · [`docs/LEDGER_INVARIANTS.md`](../docs/LEDGER_INVARIANTS.md) (L1–L12) |
| #2 + EIP-191 | [`operator-voting/`](../operator-voting/) · [`docs/OPERATOR_VOTING.md`](../docs/OPERATOR_VOTING.md) |
| #3 + #5 + #6 + #8 | [`frontend/`](../frontend/) · [`docs/FRONTEND.md`](../docs/FRONTEND.md) · [`deploy/docker/frontend.nginx.conf`](../deploy/docker/frontend.nginx.conf) |
| #7 ops | [`docs/OPS.md`](../docs/OPS.md) · [`deploy/docker/`](../deploy/docker/) · [`deploy/grants.sql`](../deploy/grants.sql) · [`operator-voting/src/rate_limit.rs`](../operator-voting/src/rate_limit.rs) · [`frontend/src/utils/prodEnvGuards.ts`](../frontend/src/utils/prodEnvGuards.ts) · [`AGENTS_OPS_STAGING.md`](AGENTS_OPS_STAGING.md) |
| #8 Legal `/vote` 404 | [`frontend/src/routes.ts`](../frontend/src/routes.ts) · [`deploy/docker/frontend.nginx.conf`](../deploy/docker/frontend.nginx.conf) · [`docs/FRONTEND.md`](../docs/FRONTEND.md) · O8 in [`docs/OPS.md`](../docs/OPS.md) · [`AGENTS_LEGAL_CLICKWRAP.md`](AGENTS_LEGAL_CLICKWRAP.md) |
| #9 zero badge | [`operator-voting/src/balance_query.rs`](../operator-voting/src/balance_query.rs) (OV-B1) · [`frontend/src/hooks/useVotingSnapshot.ts`](../frontend/src/hooks/useVotingSnapshot.ts) · ledger `/health.caught_up` |
| #14 stuck pending | [`frontend/src/hooks/useVotingSnapshot.ts`](../frontend/src/hooks/useVotingSnapshot.ts) · [`ledger/src/ingest.rs`](../ledger/src/ingest.rs) · L12 · OV-B6 · `/health.intents_ok` |

## Files in this repo to read first

- [`AGENTS.md`](../AGENTS.md)
- [`docs/HANDOFF.md`](../docs/HANDOFF.md)
- [`docs/ARCHITECTURE.md`](../docs/ARCHITECTURE.md)
- [`docs/LEDGER_INVARIANTS.md`](../docs/LEDGER_INVARIANTS.md)
- [`docs/OPERATOR_VOTING.md`](../docs/OPERATOR_VOTING.md)
- [`docs/FRONTEND.md`](../docs/FRONTEND.md)
- [`docs/OPS.md`](../docs/OPS.md)
- [`AGENTS_WALLET_CONNECTORS.md`](AGENTS_WALLET_CONNECTORS.md)
- [`AGENTS_LEGAL_CLICKWRAP.md`](AGENTS_LEGAL_CLICKWRAP.md)
- [`AGENTS_OPS_STAGING.md`](AGENTS_OPS_STAGING.md) — Coolify, Legal admin, live QA, POST limits (#7)
