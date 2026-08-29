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
| [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8) | Legal `/vote` 404 (separate from #9) |
| [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9) | Registered holders shown as 0 after live snapshot |

## Suggested order

`#1` (incl. BSC tables from `#4`) → `#2` (incl. EIP-191) → `#3` with `#5` + `#6` required. Do not ship Terra-only as done.

Descriptions on `#1`–`#4` still mention DEX paths; those are design references. Implement in this repo.

Implementation: `feat/voting-bundle` — [`HANDOFF.md`](HANDOFF.md), [`LEDGER_INVARIANTS.md`](LEDGER_INVARIANTS.md), [`OPERATOR_VOTING.md`](OPERATOR_VOTING.md), [`FRONTEND.md`](FRONTEND.md). Ops leftovers: [`OPS.md`](OPS.md) · skill [`../skills/AGENTS_OPS_STAGING.md`](../skills/AGENTS_OPS_STAGING.md). In-tree #7 extras (grants fidelity, prod migrate refuse, CSP, O3/O4 guards, CI integration) stay on `main`; Coolify / Legal admin / live wallets still block close. In-tree #9: default balance height clamp (OV-B1), pending register UX; live LCD equality on staging still blocks close.
