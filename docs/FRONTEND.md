# Voting dApp

Cross-links: [OPERATOR_VOTING.md](OPERATOR_VOTING.md) · [OPS.md](OPS.md) · skills [AGENTS_LEGAL_CLICKWRAP.md](../skills/AGENTS_LEGAL_CLICKWRAP.md) · [AGENTS_WALLET_CONNECTORS.md](../skills/AGENTS_WALLET_CONNECTORS.md) · [AGENTS_OPS_STAGING.md](../skills/AGENTS_OPS_STAGING.md) · [AGENTS_DRAFT_REVIEW.md](../skills/AGENTS_DRAFT_REVIEW.md) · issues [#3](https://gitlab.com/PlasticDigits/voting/-/issues/3) · [#5](https://gitlab.com/PlasticDigits/voting/-/issues/5) · [#6](https://gitlab.com/PlasticDigits/voting/-/issues/6) · [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) · [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) · [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) · [#10](https://gitlab.com/PlasticDigits/voting/-/issues/10) · [#11](https://gitlab.com/PlasticDigits/voting/-/issues/11) · [#16](https://gitlab.com/PlasticDigits/voting/-/issues/16)

Package: `frontend/`. Canonical routes on the dedicated host: `/`, `/new`, `/:id`. DEX-era `/vote`, `/vote/new`, `/vote/:id` stay as aliases.

## SPA documents (issue #8 / O8)

Legal Accept **full-navigates** back to `redirect_uri` (usually the current path). Client `<Link>` is not enough. The HTTP server must return **200** `text/html` (`index.html`) for every app path:

| Request | Expected |
|---------|----------|
| `GET /`, `/vote`, `/new`, `/vote/new`, `/vote/:id`, `/:id` | 200 HTML (SPA) |
| `GET /assets/<missing>.js` | 404 (do not SPA-fallback static files) |

Nginx: [`../deploy/docker/frontend.nginx.conf`](../deploy/docker/frontend.nginx.conf) (`try_files $uri /index.html`). Image HEALTHCHECK probes `/vote` and `/new`, not only `/`. CI: `test:frontend-spa-fallback` (Dockerfile conf **and** [`../deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf)). Coolify must use that image **or** paste the Coolify snippet (SPA fallback **and** O3 CSP / `X-Frame-Options` — stock 1.31.x has no repo snippets). A parent 404 for `/vote` still breaks Legal return. Do not “fix” this with `error_page 404 = /index.html` (caches a 404). Live `GET https://vote.cl8y.com/vote` is 200; [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) is closed — reopen if that 404s again.

## Legal (required)

`ConnectedTermsGate` uses `@plasticdigits/cl8y-clickwrap`. Property default `vote.cl8y.com` (`VITE_LEGAL_PROPERTY`). Connected Terra → `network="TerraClassic"`. Connected EVM → `network="EVM"`. Disconnected browse is open. Fail closed after connect. `redirect_uri` is **path-preserving** (`window.location.href`, origin allowlist `https://vote.cl8y.com`). `VITE_PLAYWRIGHT_E2E=true` may skip the gate in Playwright `webServer` only — **unset in production**. `vite.config.ts` and the Coolify image refuse any non-empty hatch, mnemonic, or `VITE_*` BSC RPC, and refuse a missing `VITE_WC_PROJECT_ID` ([`../frontend/src/utils/prodEnvGuards.ts`](../frontend/src/utils/prodEnvGuards.ts), [#12](https://gitlab.com/PlasticDigits/voting/-/issues/12)). Production nginx CSP (`connect-src` / `frame-src`, no blanket `https:` or `frame-src *`) is stamped from [`../deploy/docker/frontend.security-headers.conf`](../deploy/docker/frontend.security-headers.conf) (Dockerfile) and inlined in [`../deploy/coolify-frontend.nginx.conf`](../deploy/coolify-frontend.nginx.conf) (static paste; live `operator.vote.cl8y.com`). Re-paste that snippet after this merge so live HTML also sends WC `frame-src` and AppKit `https://api.web3modal.org` on `connect-src`.

Legal portal sign URLs are **terms only**, not voting auth. Vendor fallback `buildSignUrl` / `TermsGate` pass `account` (and `property`) so Accept stays continuous if the published SDK is missing.

### EVM in-app hint (issue #16 / L-EVM1–L-EVM5)

A voting WalletConnect or Chrome/Safari session does **not** follow the user to `terms.cl8y.com`. **Accept** still does: `@plasticdigits/cl8y-clickwrap` **>= 0.1.1** puts `account=` on that URL (`0.1.0` dropped it). The portal (Legal #15, **closed**) then offers Open in MetaMask / Binance Web3 / Copy / WalletConnect. Voting’s hint is a shortcut (Open in MetaMask + Copy link) — copy must say Accept opens Legal, not that Chrome/Safari cannot finish terms. Do not invent a voting-side `bnc://`. Desktop injected MetaMask hides the hint; Accept still full-navigates.

| ID | Rule |
|----|------|
| **L-EVM1** | Unsigned EVM with no inject, or any EVM WalletConnect session, shows the in-app/copy hint. Terra `LegalKeplrInAppHint` stays Terra-only (no MetaMask/Binance CTAs on Terra). |
| **L-EVM2** | Accept and copy/open URLs include `property` (`vote.cl8y.com` / `VITE_LEGAL_PROPERTY`), sanitized `redirect_uri`, `app_name=CL8Y Voting`, and the **connected** `account=0x…`. Never read `account` from the page query to overwrite the store. |
| **L-EVM3** | Hint hrefs are Legal terms origin + `/sign/evm` only, optionally wrapped in `https://link.metamask.io/dapp/`. Do not open arbitrary `https://` from wallet payloads. `account` is never a redirect target. |
| **L-EVM4** | WalletConnect success must **not** skip TermsGate ([#12](https://gitlab.com/PlasticDigits/voting/-/issues/12)). Status error / unknown stays fail-closed (no propose/vote). |
| **L-EVM5** | Do not reimplement portal EIP-191 in this dApp ([#5](https://gitlab.com/PlasticDigits/voting/-/issues/5)). Production clickwrap must be **>= 0.1.1** so Accept forwards `account`. Portal next-steps for phones are Legal’s (issue 15, closed). |

Code: [`../frontend/src/utils/legalEvmInAppHint.ts`](../frontend/src/utils/legalEvmInAppHint.ts) · [`../frontend/src/components/legal/LegalKeplrInAppHint.tsx`](../frontend/src/components/legal/LegalKeplrInAppHint.tsx) · skill [`../skills/AGENTS_LEGAL_CLICKWRAP.md`](../skills/AGENTS_LEGAL_CLICKWRAP.md).

Ops (Legal repo): property `vote.cl8y.com` is registered ([cl8y-ecosystem-legal#12](https://gitlab.com/PlasticDigits/cl8y-ecosystem-legal/-/issues/12)). Keep origin on Legal `CORS_ORIGINS` and portal `VITE_REDIRECT_URI_ALLOWLIST`. Full checklist: [OPS.md](OPS.md) §2 and [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7).

## Wallets (ported, not rewritten)

| Chain | Source | Voting sign |
|-------|--------|-------------|
| Terra Classic | DEX `wallet.ts` / WC pairing (WC-M1–M12) | `signArbitrary` ADR-36 |
| BSC | Bridge wagmi + `useEvmWalletDiscovery` | EIP-191 `personal_sign` |

One connected address at a time. No seed prompts. No `VITE_*` BSC RPC for balances. Wrong chain (not 56) and missing `signArbitrary` show readable errors.

## WalletConnect (issue #12 / WC-M1–WC-M12)

Hung **Connecting...** with no QR and no Open/Copy sheet is a product bug, not “ops only”. Pairing helpers already lived in-tree; the missing pieces were the **cosmes intercept**, **boot order**, **production project id**, and **CSP frames**.

| Invariant | Voting behavior |
|-----------|-----------------|
| **WC-M1** | Mobile UA / iPad desktop-UA / viewport ≤767px: pairing sheet Open + Copy, not QR-only. |
| **WC-M2** | Desktop: hook returns `false`; patched cosmes still shows Scan + canvas QR. |
| **WC-M3** | No `window.location.href` from the async `display_uri` callback. Open is a user-gesture `<a href>`. |
| **WC-M4** | Copy copies the raw `wc:` URI via `CopyButton`. |
| **WC-M5** | `isAllowedWalletConnectDeepLink` rejects `javascript:`, `data:`, and random `https://`. |
| **WC-M6** | `postinstall` `patch-package` on `@goblinhunt/cosmes`. Patched `QRCodeModal` calls `__CL8Y_WC_PAIRING_MODAL__`. Install the hook in [`../frontend/src/main.tsx`](../frontend/src/main.tsx) **before** `createRoot`. |
| **WC-M7** | Injected in-app browsers stay Extension. |
| **WC-M8** | Hook hides Connect when the mobile sheet opens (`z-[10001]` vs Connect `z-[9999]`). EVM WC closes Connect so the Reown QR is visible. |
| **WC-M9** | Cancel / backdrop / Escape / 90s timeout abort Terra `controller.connect()` **and** in-flight wagmi WC; late sessions do not attach. |
| **WC-M10** | Mobile Chrome without the matching extension offers Keplr / Station / Cosmostation via WC. No Leap. |
| **WC-M11** | Android Galaxy Open is `intent://…scheme=galaxystation`, not Hexxagon `https://…#Intent`. |
| **WC-M12** | Legal TermsGate still runs after WC. Portal sign URLs are terms only. |

Production `npm run build` / the Coolify image **fail** without `VITE_WC_PROJECT_ID`. Do not commit the secret. WalletConnect Cloud must list origin `https://vote.cl8y.com` on that same project id. Playwright hatch E2E is **not** live WC QA — do not close #12 or #7’s Cosmos WC checkbox on Simulated Wallet.

Do not add Reown AppKit / `@walletconnect/modal` as a new Terra pairing UI. Cosmes owns Terra QR; the dApp owns the mobile Open/Copy sheet.

## Copy

Votes are **offchain / advisory**. Register on the chain you hold **before votes open** (drafts do not freeze weight). Identity v1 = one address, one voter. Compose uses the structured template (issue [#10](https://gitlab.com/PlasticDigits/voting/-/issues/10)). Vote CTAs render only when `status=open` (issue [#11](https://gitlab.com/PlasticDigits/voting/-/issues/11)). Connected draft / comment / amend / open / vote stay behind clickwrap.

The dApp never reads CW20 `Balance` or BEP-20 `balanceOf` in the browser. The badge is `GET /v1/balances` ([OPERATOR_VOTING.md](OPERATOR_VOTING.md) OV-B1–B6, issues [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) · [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14)):

- Unregistered: “Register to snapshot this address’s CL8Y” — do **not** show `0 CL8Y` as if the chain were queried.
- Pending intent: “Registering…” while `GET /v1/registration` (and balances `pending`) show an in-flight intent. POST `/v1/register` `ok` is not Registered. Poll with backoff (1s→8s, default **120s** window — ledger poll 4s + LCD 20s/URL). Reload while pending **resumes** that wait.
- If the snapshot does not land: keep the pending error **and** a **Retry snapshot** control that re-polls the existing intent (no second seed prompt). Do not leave Registering… as a dead pill.
- Registered: format the server `balance` (18-decimal). A true ledger zero is allowed; an API error is not shown as 0 and propose stays fail-closed.
- Missing `registered` / `pending` on GET balances (pre-OV-B6 image) is unknown — never a live 0.
- Terra and BSC labels stay distinct. Never sum two addresses.

## Proposal template (issue #10 / OV-S)

Compose (`/new`) is labeled fields, not one TipTap blob. Client mirrors API minima (40 visible characters on required sections; summary ≤ 500) and still cannot bypass the server. Sign `body_hash` of canonical section JSON ([OPERATOR_VOTING.md](OPERATOR_VOTING.md) OV-S3). List shows `summary` as **text**. Detail renders labeled sections in display order; legacy `body_html` is a fallback after a client sanitize pass.

Success-criteria helper copy is goalposts, not punishment. Optional context does not block submit. Legal gate and 1000 CL8Y gate are unchanged.

Code: [`../frontend/src/utils/proposalSections.ts`](../frontend/src/utils/proposalSections.ts) · [`../frontend/src/pages/VoteNewPage.tsx`](../frontend/src/pages/VoteNewPage.tsx).

## Tests

```bash
cd frontend && npm test && npm run test:e2e
sh deploy/docker/test-frontend-spa-fallback.sh
```

Playwright uses 5 workers on **one** webServer (`npm run test:e2e`). Isolated agent shards must set `PW_PORT` (see [`../frontend/playwright.config.ts`](../frontend/playwright.config.ts)) so they do not collide on 5176. Legal skip hatch is on `webServer` only. E2E covers canonical `/` and `/vote*` aliases (hard navigation), pending→registered, timeout+**Retry snapshot**, and reload-while-pending (`snapshotNeverReady` / `alreadyPending` in [`../frontend/e2e/operatorMock.ts`](../frontend/e2e/operatorMock.ts)). A pending intent must not 404 as unregistered (L11). `VITE_REGISTER_POLL_TIMEOUT_MS` is Playwright-only (production default 120s; `prodEnvGuards` refuses it on Coolify builds). EVM Legal hint / `account` on Accept is unit-tested (`ConnectedTermsGate`, `legalEvmInAppHint`); Playwright hatch skips the gate so it is not live #16 QA. Live HTML still needs the [#17](https://gitlab.com/PlasticDigits/voting/-/issues/17) Coolify rebuild before users see `LEGAL_EVM_INAPP_HINT`.
