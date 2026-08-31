---
name: voting-legal-clickwrap
description: >-
  Gate the CL8Y voting dApp behind cl8y-ecosystem-legal clickwrap for Terra
  Classic and EVM wallets. Use when adding TermsGate, Legal property, CORS,
  redirect allowlists, or connected-wallet voting UI.
---

# Legal clickwrap (voting)

The voting dApp **must** be blocked behind [cl8y-ecosystem-legal](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal). Unsigned connected wallets must not see proposal-create or vote actions (fail closed). Browse-while-disconnected may stay open.

Use `@plasticdigits/cl8y-clickwrap` (`TermsGate` / `createClient`). **Do not** reimplement Terra ADR-036 or EVM verify for **terms**. Voting signatures are a separate payload on `operator-voting`.

## In-tree wiring

- [`frontend/src/utils/legalClickwrap.ts`](../frontend/src/utils/legalClickwrap.ts) — property `vote.cl8y.com`, path-preserving redirect sanitize
- [`frontend/src/components/legal/ConnectedTermsGate.tsx`](../frontend/src/components/legal/ConnectedTermsGate.tsx) — TerraClassic **or** EVM
- [`frontend/src/components/legal/LegalKeplrInAppHint.tsx`](../frontend/src/components/legal/LegalKeplrInAppHint.tsx) — Terra Keplr hint **and** EVM MetaMask/copy hint ([#16](https://gitlab.com/PlasticDigits/voting/-/issues/16))
- [`frontend/src/utils/legalEvmInAppHint.ts`](../frontend/src/utils/legalEvmInAppHint.ts) — L-EVM1–L-EVM5
- [`frontend/src/routes.ts`](../frontend/src/routes.ts) — `/` + `/vote*` aliases (Legal return / #8)
- [`deploy/docker/frontend.nginx.conf`](../deploy/docker/frontend.nginx.conf) — SPA `try_files` (O8)
- [`docs/FRONTEND.md`](../docs/FRONTEND.md)
- [`docs/OPS.md`](../docs/OPS.md) §1 step 5 (live `curl /vote` must be 200)

## Canonical references

| Doc / code | Purpose |
|------------|---------|
| Legal repo README | Properties, networks, public API |
| `packages/cl8y-clickwrap` | SDK |
| DEX `skills/AGENTS_FRONTEND_CLICKWRAP.md` | Invariants **C1–C10** (adapt; DEX is Terra-only) |
| DEX `frontend-dapp/src/utils/legalClickwrap.ts` | Property, client singleton, redirect sanitize |
| DEX `frontend-dapp/src/components/legal/ConnectedTermsGate.tsx` | Shell gate around routes |

## Voting-specific invariants

1. **SDK only** for terms. Portal full navigation — no iframe Accept.
2. **Dedicated property.** Do not reuse `dex.cl8y.com` or `cl8y.com`. Proposed hostname: `vote.cl8y.com` (confirm with ops before admin register). Env: `VITE_LEGAL_PROPERTY`.
3. **Two networks.** Connected Terra wallet → `network="TerraClassic"` (`TERRA_CLASSIC`). Connected EVM wallet → `network="EVM"`. Never check the wrong network for the connected account.
4. **Fail closed** after connect if status is unknown/error. Disconnected browse is OK.
5. **Redirect safety.** Sanitize `redirect_uri`; allowlist the voting origin. Paths such as `/`, `/new`, `/vote` are allowed on that origin. Portal allowlist remains authoritative. After Accept the portal **full-navigates** to that URI — the voting edge must serve it as 200 HTML (O8 / [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8)). Do not send `redirect_uri` to a different origin to paper over a 404.
6. **CSP.** Production `connect-src` includes Legal API + terms origin + operator-voting (`https://operator.vote.cl8y.com`). No blanket `https:`. Coolify **image** stamps [`../deploy/docker/frontend.security-headers.conf`](../deploy/docker/frontend.security-headers.conf). Coolify **static** paste stamps the same policy in [`../deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf) (Legal C6 / O3). Do not add `connect-src https:`. Nested nginx `add_header Cache-Control` drops parent CSP — the pasteable block avoids that.
7. **Secrets.** No Legal `ADMIN_TOKEN` in this frontend.
8. **E2E hatch.** `VITE_PLAYWRIGHT_E2E=true` may skip the gate in Playwright `webServer` only. Production / Coolify unset — [`../frontend/src/utils/prodEnvGuards.ts`](../frontend/src/utils/prodEnvGuards.ts) and the frontend Dockerfile fail the build if it is set.
9. **Copy.** Legal is terms evidence, not a substitute for “votes are offchain / advisory” disclosure.
10. **EVM in-app hint is required** ([#16](https://gitlab.com/PlasticDigits/voting/-/issues/16), L-EVM1–L-EVM5). Unsigned EVM without `window.ethereum`, or any WalletConnect EVM session, must get Open in MetaMask (`https://link.metamask.io/dapp/…`) plus Copy link **in addition to** Accept. Copy: Accept opens the Legal page; on a phone, open it in MetaMask or paste the link in Binance Web3. Do **not** say Chrome/Safari cannot finish terms — Accept (`@plasticdigits/cl8y-clickwrap` **>= 0.1.1**) lands on the portal, which already has Open in MetaMask / Binance Web3 / Copy / WalletConnect ([cl8y-ecosystem-legal#15](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal/-/issues/15), closed). Terra Keplr copy stays Terra-only. Do not invent a Binance `bnc://` on voting. Pass the connected `0x…` as `account` on Accept (published SDK 0.1.1 + vendor fallback). Do **not** reimplement portal EIP-191 in voting.

## Ops (Legal repo — often a separate change)

1. Register property (interactive admin token in Legal checkout — not committed here).
2. Legal API `CORS_ORIGINS` includes `https://vote.cl8y.com` (and localhost only if needed).
3. Rebuild Legal portal with `VITE_REDIRECT_URI_ALLOWLIST` including the voting origin.

DEX documents this for `dex.cl8y.com` in `AGENTS_FRONTEND_CLICKWRAP.md`. Repeat that checklist for the voting hostname. Staging/Coolify steps and #7 leftovers: [`docs/OPS.md`](../docs/OPS.md) §2 · [`AGENTS_OPS_STAGING.md`](AGENTS_OPS_STAGING.md). Live `/vote` 404 after Accept: [`docs/FRONTEND.md`](../docs/FRONTEND.md) · [`docs/OPS.md`](../docs/OPS.md) O8 · [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8).

## npm

`@plasticdigits/cl8y-clickwrap` is published from Legal (GitLab project id `82547916`). Scope `@plasticdigits` at the GitLab npm registry the same way the DEX `.npmrc` does. Production must be **>= 0.1.1** — `0.1.0` `TermsGate` Accept omitted `account=` on the portal URL.

## Rules of thumb

1. Do not treat a localStorage NFA/risk flag as Legal proof.
2. Do not use Legal sign URLs as voting `signArbitrary` / `personal_sign`.
3. After WalletConnect on mobile Chrome, if Accept still needs `window.keplr`, show the Terra Keplr in-app hint (DEX #554 WC-M12). **EVM in-app hint is required the same way** ([#16](https://gitlab.com/PlasticDigits/voting/-/issues/16)): no inject or WalletConnect → Open in MetaMask + copy Legal `/sign/evm` link. WC success must **not** skip TermsGate ([#12](https://gitlab.com/PlasticDigits/voting/-/issues/12)).
4. Do not skip TermsGate after injected or WalletConnect connect. `account` on the Accept URL is the connected store address only — never a redirect, never read from the page query.
