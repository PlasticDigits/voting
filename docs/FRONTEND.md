# Voting dApp

Cross-links: [OPERATOR_VOTING.md](OPERATOR_VOTING.md) · [OPS.md](OPS.md) · skills [AGENTS_LEGAL_CLICKWRAP.md](../skills/AGENTS_LEGAL_CLICKWRAP.md) · [AGENTS_WALLET_CONNECTORS.md](../skills/AGENTS_WALLET_CONNECTORS.md) · [AGENTS_OPS_STAGING.md](../skills/AGENTS_OPS_STAGING.md) · issues [#3](https://gitlab.com/PlasticDigits/voting/-/issues/3) · [#5](https://gitlab.com/PlasticDigits/voting/-/issues/5) · [#6](https://gitlab.com/PlasticDigits/voting/-/issues/6) · [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7)

Package: `frontend/`. Routes: `/vote`, `/vote/new`, `/vote/:id`.

## Legal (required)

`ConnectedTermsGate` uses `@plasticdigits/cl8y-clickwrap`. Property default `vote.cl8y.com` (`VITE_LEGAL_PROPERTY`). Connected Terra → `network="TerraClassic"`. Connected EVM → `network="EVM"`. Disconnected browse is open. Fail closed after connect. `VITE_PLAYWRIGHT_E2E=true` may skip the gate in Playwright `webServer` only — **unset in production**. `vite.config.ts` and the Coolify image refuse any non-empty hatch, mnemonic, or `VITE_*` BSC RPC ([`../frontend/src/utils/prodEnvGuards.ts`](../frontend/src/utils/prodEnvGuards.ts)). Production nginx CSP (`connect-src`, no blanket `https:`) is stamped from [`../deploy/docker/frontend.security-headers.conf`](../deploy/docker/frontend.security-headers.conf).

Legal portal sign URLs are **terms only**, not voting auth.

Ops (Legal repo, often a separate change): register property, add origin to `CORS_ORIGINS` and portal `VITE_REDIRECT_URI_ALLOWLIST`. Confirm hostname with ops before admin write. Full checklist: [OPS.md](OPS.md) §2 and [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7).

## Wallets (ported, not rewritten)

| Chain | Source | Voting sign |
|-------|--------|-------------|
| Terra Classic | DEX `wallet.ts` / WC pairing (WC-M1–M12) | `signArbitrary` ADR-36 |
| BSC | Bridge wagmi + `useEvmWalletDiscovery` | EIP-191 `personal_sign` |

One connected address at a time. No seed prompts. No `VITE_*` BSC RPC for balances. Wrong chain (not 56) and missing `signArbitrary` show readable errors.

## Copy

Votes are **offchain / advisory**. Register on the chain you hold **before** a proposal you care about is created. Identity v1 = one address, one voter.

## Tests

```bash
cd frontend && npm test && npm run test:e2e
```

Playwright uses 5 workers and the Legal skip hatch on its `webServer` only.
