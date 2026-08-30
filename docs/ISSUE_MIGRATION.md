# Issue migration

Source: [PlasticDigits/cl8y-dex-terraclassic](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic) (id `80162261`)  
Destination: [PlasticDigits/voting](https://gitlab.com/PlasticDigits/voting) (id `85726758`)  
Moved: 2026-08-25 via GitLab issue move (comments preserved; old DEX IIDs are **closed** and redirect).

DEX tracking stub (closed): [#637](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/637)

## Mapping

| DEX IID (closed) | Voting IID (open) | Title (short) |
|------------------|-------------------|---------------|
| [#509](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/509) | [#1](https://gitlab.com/PlasticDigits/voting/-/issues/1) | Indexer registration + CL8Y CW20 ledger |
| [#510](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/510) | [#2](https://gitlab.com/PlasticDigits/voting/-/issues/2) | `operator-voting` package |
| [#511](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/511) | [#3](https://gitlab.com/PlasticDigits/voting/-/issues/3) | Frontend `/vote` UX |
| [#588](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/588) | [#4](https://gitlab.com/PlasticDigits/voting/-/issues/4) | BSC BEP-20 + EVM voting (core) |

## New issues (voting only)

| Voting IID | Title |
|------------|-------|
| [#5](https://gitlab.com/PlasticDigits/voting/-/issues/5) | Gate dApp behind cl8y-ecosystem-legal clickwrap (Terra + EVM) |
| [#6](https://gitlab.com/PlasticDigits/voting/-/issues/6) | Reuse DEX Terra Classic and Bridge EVM wallet connectors |
| [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7) | Ops: Coolify, Legal property, live wallet QA, POST rate limits |
| [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) | Legal accept and deep links 404 on `/vote` |
| [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) | Registered holders shown as 0 after live snapshot |
| [#10](https://gitlab.com/PlasticDigits/voting/-/issues/10) | Structured proposal template and minimum required sections |
| [#11](https://gitlab.com/PlasticDigits/voting/-/issues/11) | Draft + review gate before votes open (in-flight) |
| [#12](https://gitlab.com/PlasticDigits/voting/-/issues/12) | WalletConnect stays on Connecting... (Galaxy Station, iPhone, desktop Chrome) |
| [#13](https://gitlab.com/PlasticDigits/voting/-/issues/13) | Hybrid governance research — [`GOVERNANCE_RESEARCH.md`](GOVERNANCE_RESEARCH.md) |
| [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14) | Registration stays pending; no CL8Y amount after snapshot timeout |

## Suggested order

`#1` (incl. BSC tables from `#4`) → `#2` (incl. EIP-191) → `#3` with `#5` + `#6` required. Do not ship Terra-only as done.

Descriptions on `#1`–`#4` still mention DEX paths; those are design references. Implement in this repo.

Implementation: `feat/voting-bundle` — [`HANDOFF.md`](HANDOFF.md), [`LEDGER_INVARIANTS.md`](LEDGER_INVARIANTS.md), [`OPERATOR_VOTING.md`](OPERATOR_VOTING.md), [`FRONTEND.md`](FRONTEND.md). Ops leftovers: [`OPS.md`](OPS.md) · skill [`../skills/AGENTS_OPS_STAGING.md`](../skills/AGENTS_OPS_STAGING.md). In-tree #7 extras (grants fidelity, prod migrate refuse, CSP on Dockerfile **and** Coolify static paste, O3/O4 guards, CI integration, parallel 429 smoke) stay on `main`; live O3 header re-paste, ledger-private, and Keplr/MetaMask QA (blocked on #9) still block close. #8 is **closed** (live `GET /vote` 200); CI SPA smoke covers both nginx confs. In-tree #9: default balance height clamp (OV-B1), pending register UX; live LCD equality on staging still blocks close. In-tree #10: `body_sections` + canonical hash (OV-S); staging Terra/BSC compose still human before close. Draft lifecycle is [#11](https://gitlab.com/PlasticDigits/voting/-/issues/11), not this issue. In-tree #12: cosmes patch + hook-before-`createRoot` + `VITE_WC_PROJECT_ID` fail-closed + CSP `frame-src`; live Galaxy/iPhone/desktop Chrome + BSC WC + Cloud origin still block close. In-tree #14: pending poll+retry, intent loop vs ingest, L12, OV-B6; live writer `/health` not 503 still blocks close.
