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

## Implementation (landed in-tree)

| Issue | Code |
|-------|------|
| #1 + #4 ledger | [`ledger/`](../ledger/) · [`docs/LEDGER_INVARIANTS.md`](../docs/LEDGER_INVARIANTS.md) (L1–L11) |
| #2 + EIP-191 | [`operator-voting/`](../operator-voting/) · [`docs/OPERATOR_VOTING.md`](../docs/OPERATOR_VOTING.md) |
| #3 + #5 + #6 | [`frontend/`](../frontend/) · [`docs/FRONTEND.md`](../docs/FRONTEND.md) |
| #7 ops | [`docs/OPS.md`](../docs/OPS.md) · [`deploy/docker/`](../deploy/docker/) · [`deploy/grants.sql`](../deploy/grants.sql) · [`operator-voting/src/rate_limit.rs`](../operator-voting/src/rate_limit.rs) · [`frontend/src/utils/prodEnvGuards.ts`](../frontend/src/utils/prodEnvGuards.ts) · [`AGENTS_OPS_STAGING.md`](AGENTS_OPS_STAGING.md) |

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
