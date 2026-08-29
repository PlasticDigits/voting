# Voting dApp

Cross-links: [OPERATOR_VOTING.md](OPERATOR_VOTING.md) · [OPS.md](OPS.md) · skills [AGENTS_LEGAL_CLICKWRAP.md](../skills/AGENTS_LEGAL_CLICKWRAP.md) · [AGENTS_WALLET_CONNECTORS.md](../skills/AGENTS_WALLET_CONNECTORS.md) · [AGENTS_OPS_STAGING.md](../skills/AGENTS_OPS_STAGING.md) · issues [#3](https://gitlab.com/PlasticDigits/voting/-/issues/3) · [#5](https://gitlab.com/PlasticDigits/voting/-/issues/5) · [#6](https://gitlab.com/PlasticDigits/voting/-/issues/6) · [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) · [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8)

Package: `frontend/`. Canonical routes on the dedicated host: `/`, `/new`, `/:id`. DEX-era `/vote`, `/vote/new`, `/vote/:id` stay as aliases.

## SPA documents (issue #8 / O8)

Legal Accept **full-navigates** back to `redirect_uri` (usually the current path). Client `<Link>` is not enough. The HTTP server must return **200** `text/html` (`index.html`) for every app path:

| Request | Expected |
|---------|----------|
| `GET /`, `/vote`, `/new`, `/vote/new`, `/vote/:id`, `/:id` | 200 HTML (SPA) |
| `GET /assets/<missing>.js` | 404 (do not SPA-fallback static files) |

Nginx: [`../deploy/docker/frontend.nginx.conf`](../deploy/docker/frontend.nginx.conf) (`try_files $uri /index.html`). Image HEALTHCHECK probes `/vote` and `/new`, not only `/`. CI: `test:frontend-spa-fallback`. Coolify must use that image **or** stamp the same `try_files` on the live edge — a parent 404 for `/vote` still breaks Legal return. Do not “fix” this with `error_page 404 = /index.html` (caches a 404).

## Legal (required)

`ConnectedTermsGate` uses `@plasticdigits/cl8y-clickwrap`. Property default `vote.cl8y.com` (`VITE_LEGAL_PROPERTY`). Connected Terra → `network="TerraClassic"`. Connected EVM → `network="EVM"`. Disconnected browse is open. Fail closed after connect. `redirect_uri` is **path-preserving** (`window.location.href`, origin allowlist `https://vote.cl8y.com`). `VITE_PLAYWRIGHT_E2E=true` may skip the gate in Playwright `webServer` only — **unset in production**. `vite.config.ts` and the Coolify image refuse any non-empty hatch, mnemonic, or `VITE_*` BSC RPC ([`../frontend/src/utils/prodEnvGuards.ts`](../frontend/src/utils/prodEnvGuards.ts)). Production nginx CSP (`connect-src`, no blanket `https:`) is stamped from [`../deploy/docker/frontend.security-headers.conf`](../deploy/docker/frontend.security-headers.conf).

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
sh deploy/docker/test-frontend-spa-fallback.sh
```

Playwright uses 5 workers and the Legal skip hatch on its `webServer` only. E2E covers canonical `/` and `/vote*` aliases (hard navigation).
