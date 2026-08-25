# CL8Y Voting

Offchain **CL8Y snapshot voting** for Terra Classic CW20 holders and BNB Smart Chain BEP-20 holders. This is a standalone repo. It is not part of the DEX indexer or dApp.

**GitLab:** [PlasticDigits/voting](https://gitlab.com/PlasticDigits/voting)

Voting used to be tracked as issues **#509 / #510 / #511 / #588** on [cl8y-dex-terraclassic](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic). Those issues live here now. Mapping: [`docs/ISSUE_MIGRATION.md`](docs/ISSUE_MIGRATION.md).

This checkout is a **bootstrap**: docs, agent playbooks, and GitLab project. Ledger, `operator-voting`, and the `/vote` dApp are not implemented yet.

## Product (short)

- No new CosmWasm or Solidity contracts.
- No archive LCD / archive BSC node. Register with a live balance, then track transfers for registered wallets only.
- Dual electorate: Terra Classic CW20 **and** BSC BEP-20 (majority of CL8Y is on BSC).
- ≥1000 CL8Y on the registering address’s chain to create a proposal.
- Votes are signed (Terra ADR-36 / EVM EIP-191), weighted by a frozen snapshot, and advisory / offchain (not factory-multisig governance).

## Hard product constraints (do not skip)

1. **Legal gate.** The voting dApp must be blocked behind [cl8y-ecosystem-legal](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal) clickwrap (`@plasticdigits/cl8y-clickwrap`). See [`skills/AGENTS_LEGAL_CLICKWRAP.md`](skills/AGENTS_LEGAL_CLICKWRAP.md).
2. **Wallets.** Do not invent a third wallet stack. For wallet connecting software, as there has been many problems with it, the terraclassic connections on cl8y dex and the evm connections on cl8y bridge are the most well functional. See [`skills/AGENTS_WALLET_CONNECTORS.md`](skills/AGENTS_WALLET_CONNECTORS.md).

## Sibling repos

| Repo | Role for voting |
|------|-----------------|
| [cl8y-dex-terraclassic](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic) | Copy Terra Classic connect + `signArbitrary` patterns. Do **not** add voting routes there. |
| [cl8y-bridge-monorepo](https://gitlab.com/PlasticDigits/cl8y-bridge-monorepo) | Copy EVM (wagmi / WalletConnect `eip155:56`) connect + `personal_sign` patterns. |
| [cl8y-ecosystem-legal](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal) | Terms property, CORS, portal redirect allowlist, SDK. |

## Tokens (canonical)

| Chain | Token | Decimals |
|-------|-------|----------|
| Terra Classic CW20 | `terra16wtml2q66g82fdkx66tap0qjkahqwp4lwq3ngtygacg5q0kzycgqvhpax3` | 18 |
| BSC BEP-20 | [`0x8F452a1fdd388A45e1080992eFF051b4dd9048d2`](https://bscscan.com/token/0x8F452a1fdd388A45e1080992eFF051b4dd9048d2) | 18 |

## Docs for agents

| Doc | When to read |
|-----|----------------|
| [`AGENTS.md`](AGENTS.md) | Start here |
| [`docs/HANDOFF.md`](docs/HANDOFF.md) | Current state + next work |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Planned packages |
| [`docs/ISSUE_MIGRATION.md`](docs/ISSUE_MIGRATION.md) | Old DEX IID → this repo |
| [`skills/AGENTS_VOTING_BUNDLE.md`](skills/AGENTS_VOTING_BUNDLE.md) | Issue order and invariants |

## Local git hooks

```bash
git config core.hooksPath .githooks
```

Commit message **bodies** must not contain email addresses, `Co-authored-by`, or the word **author**. Never use `--no-verify`.
