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

- [`frontend/src/utils/legalClickwrap.ts`](../frontend/src/utils/legalClickwrap.ts) — property `vote.cl8y.com`, redirect sanitize
- [`frontend/src/components/legal/ConnectedTermsGate.tsx`](../frontend/src/components/legal/ConnectedTermsGate.tsx) — TerraClassic **or** EVM
- [`docs/FRONTEND.md`](../docs/FRONTEND.md)

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
5. **Redirect safety.** Sanitize `redirect_uri`; allowlist the voting origin. Portal allowlist remains authoritative.
6. **CSP.** Production `connect-src` includes Legal API + terms origin. No blanket `https:`.
7. **Secrets.** No Legal `ADMIN_TOKEN` in this frontend.
8. **E2E hatch.** `VITE_PLAYWRIGHT_E2E=true` may skip the gate in Playwright `webServer` only. Production / Coolify unset.
9. **Copy.** Legal is terms evidence, not a substitute for “votes are offchain / advisory” disclosure.

## Ops (Legal repo — often a separate change)

1. Register property (interactive admin token in Legal checkout — not committed here).
2. Legal API `CORS_ORIGINS` includes `https://vote.cl8y.com` (and localhost only if needed).
3. Rebuild Legal portal with `VITE_REDIRECT_URI_ALLOWLIST` including the voting origin.

DEX documents this for `dex.cl8y.com` in `AGENTS_FRONTEND_CLICKWRAP.md`. Repeat that checklist for the voting hostname.

## npm

`@plasticdigits/cl8y-clickwrap` is published from Legal (GitLab project id `82547916`). Scope `@plasticdigits` at the GitLab npm registry the same way the DEX `.npmrc` does.

## Rules of thumb

1. Do not treat a localStorage NFA/risk flag as Legal proof.
2. Do not use Legal sign URLs as voting `signArbitrary` / `personal_sign`.
3. After WalletConnect on mobile Chrome, if Accept still needs `window.keplr`, say so (DEX #554 WC-M12). Same class of hint for EVM in-app browsers.
