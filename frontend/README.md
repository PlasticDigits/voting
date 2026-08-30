# CL8Y Voting dApp

Standalone Vite + React voting UI for offchain, advisory CL8Y snapshot votes.

Votes are **not** on-chain execution. Register the wallet that holds CL8Y **on that chain** before votes open. Identity v1 is **one address, one voter** — a `terra1…` and a `0x…` are never merged.

## Env

See [`.env.example`](./.env.example). Copy to `.env.local`.

| Variable | Purpose |
| --- | --- |
| `VITE_OPERATOR_VOTING_URL` | operator-voting API |
| `VITE_LEGAL_PROPERTY` | Legal property (default `vote.cl8y.com`) |
| `VITE_LEGAL_API_BASE_URL` | Legal public API |
| `VITE_LEGAL_TERMS_BASE_URL` | Legal portal |
| `VITE_WC_PROJECT_ID` | WalletConnect (Terra + EVM) |
| `VITE_NETWORK` | `local` / `testnet` / `mainnet` for Terra connect |
| `VITE_DEV_MODE` / `VITE_DEV_MNEMONIC` | Simulated Terra wallet only. Never ask users for a seed. |
| `VITE_PLAYWRIGHT_E2E` | Legal skip hatch. Playwright `webServer` only. |

**Do not** set `VITE_BSC_RPC` or any `VITE_*` JSON-RPC URL for balance reads. wagmi `http()` without a Vite env is used for BSC chain-switch only. The indexer / operator-voting owns `eth_call` and `eth_getLogs`.

## Coolify

- Unset `VITE_PLAYWRIGHT_E2E`, `VITE_DEV_MNEMONIC`, and any `VITE_*` BSC RPC on production builds (`prodEnvGuards` + [`../deploy/docker/frontend.Dockerfile`](../deploy/docker/frontend.Dockerfile) fail the build otherwise).
- Production nginx CSP is stamped from [`../deploy/docker/frontend.security-headers.conf`](../deploy/docker/frontend.security-headers.conf) (Legal + operator-voting origins; no blanket `https:`).
- Point `VITE_OPERATOR_VOTING_URL` at the public operator-voting origin.
- The image must use [`../deploy/docker/frontend.nginx.conf`](../deploy/docker/frontend.nginx.conf) so `GET /vote` and `GET /new` are 200 HTML (Legal return, [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8)). If Coolify serves static files with a default nginx, paste [`../deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf) (`try_files $uri /index.html`). Missing `/assets/*` must stay 404.
- Register Legal property `vote.cl8y.com` in [cl8y-ecosystem-legal](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal) (interactive admin token — never `ADMIN_TOKEN` in this app).
- Add `https://vote.cl8y.com` to Legal `CORS_ORIGINS` and portal `VITE_REDIRECT_URI_ALLOWLIST`.
- Full Coolify / Legal / live-QA checklist: [`../docs/OPS.md`](../docs/OPS.md) · [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7).

## CSP `connect-src`

The Coolify image stamps [`../deploy/docker/frontend.security-headers.conf`](../deploy/docker/frontend.security-headers.conf) (Legal C6 / O3). Production `connect-src` must list explicit origins — **no blanket `https:`**:

- Legal API (`https://api.terms.cl8y.com` or your `VITE_LEGAL_API_BASE_URL`)
- Legal portal (`https://terms.cl8y.com` or your `VITE_LEGAL_TERMS_BASE_URL`)
- operator-voting (`VITE_OPERATOR_VOTING_URL`)
- WalletConnect / wallet vendor endpoints as required by the ported DEX/Bridge stacks
- Terra LCD/RPC used for **connect / suggest-chain only** (not CL8Y balance)

## Legal

Connected wallets must accept CL8Y Legal before propose / vote CTAs. Browse while disconnected stays open. Fail closed if status is unknown. Use `@plasticdigits/cl8y-clickwrap`. If the GitLab npm registry is unauthenticated, the app falls back to `src/vendor/cl8y-clickwrap.ts` so it compiles — **production must use the published SDK**.

## Signing

- Client signs a canonical JSON payload (`app: "cl8y-voting"`, purpose-separated).
- `body_hash` is SHA-256 hex of the HTML the client **submits** (after a local script/`on*` strip). operator-voting hashes that submitted body, then stores ammonia-sanitized HTML. Ammonia is not part of the signed hash — keep the client strip close to the allowlist so TipTap markup stays stable.
- Terra: `window.keplr.signArbitrary` (clear error if missing). Simulated wallet uses cosmes `MnemonicWallet.signArbitrary` (ADR-36).
- EVM: wagmi / viem `personal_sign`. Simulated EVM uses the wagmi mock connector.

## Scripts

```bash
npm run dev
npm run build
npm test
npm run test:e2e
```
