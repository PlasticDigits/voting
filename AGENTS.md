# AGENTS.md

Guidance for AI coding agents on **PlasticDigits/voting**.

Read [`docs/HANDOFF.md`](docs/HANDOFF.md) before implementing. Voting is **not** implemented in `cl8y-dex-terraclassic`.

## What this repo is

Standalone offchain CL8Y snapshot voting:

- Terra Classic CW20 ledger + BSC BEP-20 ledger (registered wallets only)
- `operator-voting` API (signatures, proposals, votes, blacklist)
- Voting dApp (`/vote`) gated by CL8Y Legal

**Suggested implementation order:** ledger (#1, from DEX #509) → `operator-voting` (#2, from DEX #510) → dApp (#3, from DEX #511) with BSC treated as **core** (#4, from DEX #588), not a later extra. Legal gate and wallet reuse are required for the dApp, not optional polish.

## Hard rules

1. **No new smart contracts** (no CosmWasm governor, no Solidity snapshot token).
2. **No archive node.** Live balance at registration + transfers forward.
3. **BSC is core.** A Terra-only electorate is an incomplete product.
4. **Legal.** Connected users must pass [cl8y-ecosystem-legal](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal) before transactional voting UI. Skill: [`skills/AGENTS_LEGAL_CLICKWRAP.md`](skills/AGENTS_LEGAL_CLICKWRAP.md).
5. **Wallets.** For wallet connecting software, as there has been many problems with it, the terraclassic connections on cl8y dex and the evm connections on cl8y bridge are the most well functional. Port those stacks. Do not start from a greenfield wallet library. Skill: [`skills/AGENTS_WALLET_CONNECTORS.md`](skills/AGENTS_WALLET_CONNECTORS.md).
6. **Identity v1 = one address, one voter.** Do not silently merge `terra1…` and `0x…`.
7. **No `VITE_*` BSC RPC** in the browser for balance reads. Indexer owns `eth_call` / `eth_getLogs`.
8. **Least privilege.** `operator-voting` must not use the ledger writer’s Postgres role.

## Sibling checkouts (typical)

```
~/repos/voting                 # this repo
~/repos/cl8y-dex-terraclassic  # Terra wallet + Legal gate reference
~/repos/cl8y-ecosystem-legal   # clickwrap SDK + property admin
# clone if missing:
#   git clone https://gitlab.com/PlasticDigits/cl8y-bridge-monorepo.git ~/repos/cl8y-bridge-monorepo
```

DEX paths that **describe** the original design (do not implement voting there):

- Indexer LCD / parser / Postgres: `cl8y-dex-terraclassic/indexer/`
- Terra wallet: `cl8y-dex-terraclassic/frontend-dapp/src/services/terraclassic/wallet.ts`
- Legal gate: `cl8y-dex-terraclassic/frontend-dapp/src/components/legal/ConnectedTermsGate.tsx`

Bridge EVM wallet (copy target):

- `packages/frontend/src/lib/wagmi.ts`
- `packages/frontend/src/hooks/useWallet.ts`, `useEvmWalletDiscovery.ts`
- `packages/frontend/src/stores/wallet.ts`

## Git commits

Use a **human** `git config user.name` / `user.email` (local: Plastic Digits identity). Do not commit as Cursor, Claude, Codex, or a GitLab project-bot.

Commit message **bodies** must not contain:

- Email addresses
- `Co-authored-by` or any line containing the word **author**
- AI/agent attribution trailers

Enable hooks: `git config core.hooksPath .githooks`

**Never** `git commit --no-verify` or `git push --no-verify`.

## GitLab

```bash
# this project
glab issue list --repo PlasticDigits/voting

# historical source (URLs redirect after move)
# https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/509
```

IID mapping: [`docs/ISSUE_MIGRATION.md`](docs/ISSUE_MIGRATION.md).

## Lint / secrets

```bash
gitleaks detect --source . --config .gitleaks.toml --verbose
```

CI runs Gitleaks plus `cargo test --workspace --lib` and frontend `npm test`. Postgres integration jobs need `LEDGER_TEST_DATABASE_URL` (see `docker-compose.test.yml`).
