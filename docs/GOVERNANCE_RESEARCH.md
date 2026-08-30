# Governance research: leadership proposes, holders ratify

**Issue:** [#13](https://gitlab.com/PlasticDigits/voting/-/issues/13)  
**Status:** research note (no schema, API, or UI change).  
**Date:** 2026-08-30  
**Related (in-flight, not done):** structured template [#10](https://gitlab.com/PlasticDigits/voting/-/issues/10) · draft/review gate [#11](https://gitlab.com/PlasticDigits/voting/-/issues/11)

This document is the product doctrine for *what voting is for*. It does not implement a governor, committee, or proposal type. Later issues should cite it instead of inventing a model from a single conversation or from a famous DAO that this repo cannot copy.

---

## 1. Product recap (stage 1)

This repo is an **impartial offchain snapshot poll** for registered Terra Classic CW20 CL8Y and BSC BEP-20 CL8Y wallets.

| Fact | Implication |
|------|------------|
| Votes are `advisory: true` | A passed poll is a mandate *signal*, not authority to move funds or change DEX factory fees. |
| Freeze `{ terra_height, bsc_block }` at **vote-open** (issue [#11](https://gitlab.com/PlasticDigits/voting/-/issues/11)) | Weight is frozen when the committee opens the poll, not at draft create. Selling after open does not change weight ([LEDGER_INVARIANTS.md](LEDGER_INVARIANTS.md) L9: buy-before-freeze *is* counted). |
| One address, one voter | `terra1…` and `0x…` are never merged. Supplies are separately circulating ([ARCHITECTURE.md](ARCHITECTURE.md)). |
| ≥1000 CL8Y to propose | Spam bar on **that** address’s chain, not a second electorate. |
| Blacklist is env | `VOTING_BLACKLIST_ADDRESSES` is ops, not a governance body. |
| Legal clickwrap | Transactional UI fails closed. A “governance emergency” does not skip terms. |
| No new contracts | Do **not** add a CosmWasm voting contract, `governor.sol`, or an on-chain treasury executor in this repo. |
| No archive node | Do **not** design around archive LCD / historical `eth_getLogs` backfill. |
| DEX factory is separate | Factory wasm admin / fee governance stays on `terra1zlmv2xydxcusurtr6rl78wsvytdc6mfex6hep7`. Do **not** bind this API to factory multisig control from this app. |

Stage 1 answers “holders can act.” It does not yet answer “which decisions belong on a ballot.” Stage 2 is process (#10 template, #11 draft + feasibility gate), not a constitution that blocks shipping stage 1.

---

## 2. Synthesized holder goals

Themes below are synthesized from a private community discussion (2026-08-29–30). Individual messages, handles, and quotes are **not** reproduced.

1. **Technical and day-to-day operations** stay with people who understand the system (a documented leadership role: founder and/or a small ops team), so execution stays coherent.
2. **Risk, community capital, issuance, and fundamental governance** have no single engineering answer. Holders who bear the risk need a mandate after being advised.
3. **Leadership proposes:** evaluate options, pick a recommended path, explain reasoning, then ask holders to approve or reject. An open menu of equal strategies is the failure mode (competing agendas, stalled progress).
4. **Oversight, if any, verifies feasibility** (implementable, legal, funded, scoped) rather than recasting the same vote.
5. **Process can land after impartial dual-chain tooling.** Do not block stage 1 on a constitution.
6. **Continuity:** written process should outlive any one person. Bus-factor belongs in runbooks, not as an excuse to delay stage 1.
7. **Large liquidity / treasury targets** should be phased (tranches, review windows, a halt). Halt is an ops/treasury process, not `POST /v1/votes`.

**Intended hybrid (read this first):** Leadership decides how to implement. For major fund, risk, issuance, and governance-rule items, leadership publishes one recommended option plus rationale; registered Terra **and** BSC holders ratify or reject. A reviewer can refuse to *open* an incomplete or infeasible poll; they cannot vote as a bloc or rewrite the outcome. Polls stay advisory. This API does not execute.

---

## 3. Research questions

### Q1 — Who decides what?

Comparable systems split three layers:

| Layer | Typical actor | Examples | CL8Y mapping |
|-------|----------------|----------|--------------|
| **Electorate** | Token holders (token-weighted) | UNI Governor, AAVE, SKY/MKR executive, LDO | Registered CL8Y on **that** chain, frozen snapshot |
| **Leadership / stewards / SPs** | Named teams with a mandate | Maker risk teams + facilitators; Aave Chaos/Llama/BGD/ACI; Yearn yTeams; ENS working-group stewards | Founder / ops team: engineering, wallets, RPC, Legal wiring, marketing execution |
| **Feasibility reviewer** | Payload/security review that cannot pass the *outcome* | Aave Certora/BGD payload review; Maker spell review; #11 committee `open_vote` | Block incomplete/unsafe *description*; do not recast the tally |

Optimism’s Citizens’ House is **not** a CL8Y v1 analog (reputation 1p1v, citizenship). Lido dual governance is **not** a Terra+BSC analog (stakers vs token holders, not two ledgers of the same token). Both teach a useful check: the group that bears a distinct risk should have a distinct voice — here, Terra and BSC remain **separate voters**, not a merged person.

### Q2 — How do hybrids implement “leadership proposes, holders ratify”?

Successful hybrids do **not** present N unstructured strategies as equal ballot options.

- **Maker / Sky:** risk and facilitator teams recommend parameter values; **governance polls** measure sentiment (often ranked/approval on a *parameter spectrum*); **executive spells** are a single bundled payload holders ratify. Weekly polls “cannot change system parameters independently; they merely dictate what will be included in the next Executive Vote” ([MIP16](https://github.com/makerdao/mips/blob/master/MIP16/mip16.md)).
- **Aave:** TEMP CHECK → ARFC → Snapshot → AIP payload. ACI Skyward *writes* the payload after a recommended path exists; Security SPs review that the payload matches the description. Holders still vote YAE/NAY. Skyward “does not guarantee a YAE vote.”
- **Uniswap:** RFC (≥7 days) → Snapshot temperature check (5 days, 10M UNI yes threshold) → on-chain Governor. Temp check is labeled **signal**. On-chain is a **single executable**, not a strategy contest.
- **Yearn:** holders ratify *structure* (which yTeams exist, fee/treasury powers). yTeams execute day-to-day. Snapshot is expected to be implemented by ops; the multisig executes or vetoes.

**CL8Y fit:** product copy and #10 should require a **recommended option + rationale + yes/no** (or a small bounded spectrum: e.g. LP depth $X / $Y / $Z with a stated preference). Do not ship an open strategy marketplace. Simulation (`/tmp/gov-research/sim_governance.py` sim3): a plurality “winner” can still **fail** a yes/no on that same option — that is honest ratification, not silent plurality capture.

### Q3 — How is risk/reward framed as a spectrum?

Holders choose a **point on a bound**, not an engineering design.

| Domain | Spectrum framing (examples) | Who implements |
|--------|------------------------------|----------------|
| LP depth | Pilot $X → target $Y in tranches; halt if IL / volume / custody bound is hit | Ops / market-makers |
| Asset mix | Recommended pair + why; optional bounded alternative (stable vs volatile) | Same |
| Issuance | Mint/unlock 0 / small / large; or “no issuance” | Counsel + ops (needs counsel) |
| Collateral / risk (Maker-style) | Stability fee, debt ceiling, liquidation ratio as numbers | Risk team |
| Steward envelope (Aave analog) | Holders ratify **bounds** (e.g. caps ±50% / 5-day); ops tune inside | Risk steward / ops — not a new poll per tick |
| Grants / RetroPGF | Round size and category, not each vendor invoice | Grants operators |

Maker polls often ask “which value” (IRV/approval) *after* a risk write-up. Uniswap LM on Optimism asked holders to pick a **phase budget split** (e.g. 50k/100k/650k OP) then ops ran phases with retrospectives. CL8Y #10 should include a **risk spectrum** section (recommended point, bounds, halt condition) so voters are not asked to design a pool.

### Q4 — Stage 1 vs stage 2

| | Tooling first (stage 1) | Constitution / process later (stage 2) |
|--|--------------------------|------------------------------------------|
| **Worked** | Snapshot + Safe (signal then separate execution); Uniswap temp-check before Governor; this repo’s register/propose/vote | Maker Atlas/MIPs after years of polls; Yearn YIP-61 after organic yTeams; ENS constitution + WG rules after executable DAO existed |
| **Broke when process skipped** | Arbitrum AIP-1 (2023): bundled constitution + $1B-scale foundation budget, framed as “ratification” of transfers **already made** — process theater. LUNC: permissionless on-chain marketplace with text/spend/param mixed, execution gap. Gitcoin: informal stewards with unclear rights → council retro still “ad hoc coordination.” Tribe/Fei (2022): Snapshot ~75% to reimburse Fuse victims, then treated as non-binding / reversed — retroactive rule-change. |
| **Broke when process arrived too early** | Nouns: every small grant on the governor → voter fatigue, then Prop House / fork. Compound Proposal 62: executable governor with no guardian pause; timelock delayed the *fix* as much as the bug. Optimism Working Constitution promised a Citizens check before that house could veto upgrades (sequential, not live at Token House genesis). Yearn YIP-41 gave the multisig “six months” of ops powers; the written mandate (YIP-61) arrived after the keys had already moved. |

**CL8Y:** ship dual-chain advisory snapshots now. #10/#11 are stage 2 *process*, not a written constitution. Do not block #7–#9 ops on this note.

### Q5 — Continuity / succession (no “the DAO runs itself”)

Written artifacts that survive a founder, with **no new contracts**:

| Survives today | Breaks if the operator is unavailable |
|----------------|----------------------------------------|
| Signed register/propose/vote (ADR-36 / EIP-191), ledger checkpoints, `advisory: true` copy | Coolify services, `DATABASE_URL` roles, GitLab, `VOTING_BLACKLIST_ADDRESSES` env, Legal admin property/CORS/allowlist |
| [OPS.md](OPS.md) O1–O8, [LEDGER_INVARIANTS.md](LEDGER_INVARIANTS.md) | Human rotation of those env/admin surfaces |

Bitcoin continuing after Satoshi is **not** a DAO analog; it is a lesson that **written rules + independent operators** beat personality. Yearn and ENS survive key-person risk by *documenting* who holds which delegated power and how signers rotate — still humans, still multisigs.

**Minimum written process this repo should eventually support:** (1) named leadership role in product copy (ops channel + signing address, not a display name); (2) dual-control / documented rotation for Coolify, GitLab, blacklist env, Legal admin ([OPS.md](OPS.md)); (3) #11 allowlist rotation procedure if a committee exists. No fantasy that polls operate the stack.

### Q6 — Tranches, review windows, halt

**Real example (primary):** Uniswap × Optimism liquidity mining (2022–2023). 1M OP grant; 800k for LM. The RFC offered **phased deployment** so Phase 1 could be small, reviewed, then scaled ([RFC](https://gov.uniswap.org/t/rfc-the-optimism-uniswap-protocol-liquidity-mining-program/17820)): e.g. 50k OP / 2 weeks → 100k / 3 weeks → 650k remainder, with analysis between phases. Phase 3 update and a later retrospective exist. Liquidity often left when incentives ended — the review window *surfaced* that; it did not require a new token vote to stop a phase once the plan included a stop.

**Maker:** LitePSM USDC migration in **phases** (Phase 1 setup executive 2024-07-25: 20M USDC migrated, DC-IAM, GSM delay, ESM threshold). Halt/breaker: `DIRECT_MOM` / ESM are **protocol** kill switches, not Snapshot.

**CL8Y:** describe tranche + halt **inside** the ratified proposal text (#10 section). Execution and halt are ops/treasury (Safe, market-maker, custody) — **not** `POST /v1/votes`. Informal after-the-fact “we’ll halt if we feel like it” is griefing; a halt used to bypass a failed poll is also griefing.

Simulation (`sim4`): 10M all-or-nothing vs 4×2.5M with halt after tranche 3 preserves the unspent remainder.

### Q7 — Negative cases (mechanisms)

1. **Terra Classic / LUNC** — permissionless `x/gov`: deposit → vote (bonded LUNC, validators default) → handler or *nothing*. Text/signal vs param vs spend vs upgrade share one UX. Competing agendas; **execution gap** between “passed” and “delivered.” Live LCD 2026-08-30: `min_deposit` 5e12 uluna, `voting_period` 604800s, quorum 0.40, threshold 0.50, `veto_threshold` 0.334, `burn_vote_veto` true. Proposal **12223** (tax 0.5%→1.5%) **passed** while **abstain was the largest bucket** (~38% of all votes; yes ~32% of all; yes/non-abstain ≈ 52.6%). Low-information path: participation without support still lets a param change land. Do not copy this UX.
2. **Arbitrum AIP-1 (2023)** — bundled constitution + Security Council + 750M ARB foundation budget; community discovered transfers **already executed**; vote reframed as ratification. Process theater. Later split into AIP-1.1/1.2.
3. **Beanstalk (2022)** — live-balance `emergencyCommit` + flash loan ≈ $182M. Snapshot-at-prior-block would have made the flash N/A. **Do not** copy an on-chain governor with live balances.
4. **Gitcoin stewards** — unclear rights (consultant vs decision-maker), informal selection, council term-1 retro: still dependent on ad hoc coordination. Do not copy an untitled “expert” that silently rewrites outcomes.
5. **Curve ve / bribes** — token-weighted capture is *explicit* and marketized (Votium). Honesty lesson: CL8Y is token-weighted; do not pretend 1w1v.
6. **Nouns** — frequent on-chain votes → fatigue; Prop House moved small grants off the governor; fork #0 (20% threshold) was an exit, not a strategy vote.

### Q8 — What maps onto this stack vs a new issue / different repo

| Maps onto stage 1 / #10 / #11 | Requires a later issue in *this* repo | Different repo / out of scope |
|--------------------------------|----------------------------------------|-------------------------------|
| Advisory snapshots, dual-chain, 1000 CL8Y gate, blacklist env, Legal | Template sections: recommendation, risk spectrum, success criteria, tranche/halt plan (#10 additive) | Executable treasury, CosmWasm voting contract, `governor.sol` |
| Leadership-proposes yes/no copy | Draft + feasibility `open_vote` (#11); freeze at open | Binding this API to DEX factory admin |
| Attribute “official recommendation” to a signing address or published ops channel | Proposal *category* / major-vs-ops flag (sketch only) | Merge terra+evm identity; archive LCD electorate |
| Document plutocracy + L9 buy-before-freeze | Committee allowlist rotation runbook | Securities/fiduciary regime (needs counsel, stop) |

---

## 4. Case-study table

Each row: model · voted vs not · proposer · executor · snapshot vs on-chain · attack/failure · CL8Y fit.

| # | System | Model | Voted | Leadership / SP | Proposer | Executor | Snapshot vs chain | Attack / failure | CL8Y fit |
|---|--------|--------|-------|-----------------|----------|----------|-------------------|------------------|----------|
| 1 | Terra Classic / LUNC | Cosmos `x/gov` marketplace | Params, spend, upgrades, **text/signal** | Validators inherit delegator votes | Anyone meeting deposit | Module handler **or nobody** | On-chain bonded LUNC | Competing agendas; execution gap; 12223 abstain-plurality pass | **No** (negative). Study only. |
| 2 | Maker / Sky | Polls + executive spells + delegated risk | Risk params, spells | Risk teams, facilitators, spell review | Facilitators / SPs | Spell `cast` after GSM delay | On-chain MKR/SKY | Spell bundling; GSM delay vs emergency | **Yes** for poll→ratify *shape*; **no** for spells |
| 3 | Aave | TEMP/ARFC/Snapshot/AIP + SPs | Caps, listings, SP budgets | Chaos, Llama, BGD, Certora, ACI | Token holder or SP-assisted | AIP payload | Snapshot then on-chain | Payload ≠ description; SP capture | **Yes** for payload *review* ≠ electorate |
| 4 | Uniswap | RFC → Snapshot temp check → Governor | Fees, treasury, governor upgrades | Foundation / UF coordinates | Delegate ≥1M UNI on-chain | Governor + timelock | Snapshot **signal**; Governor binding | High threshold plutocracy; temp check misread as binding if unlabeled | **Yes** for labeled temp check |
| 5 | Optimism | Bicameral + seasons | Token House economics; Citizens RetroPGF | DAB / Foundation for upgrades | Delegates / boards | Foundation / protocol | Mixed | Token plutocracy vs citizens; upgrade veto | **Later** (no 1p1v citizenship here) |
| 6 | Arbitrum | Constitution + Security Council | AIPs, treasury | Foundation; SC 9/12 emergency | AIP process | DAO / SC / Foundation | On-chain | AIP-1 ratification theater | **Yes** for “don’t ratify after the fact”; **no** for SC in this API |
| 7 | Lido | LDO + Dual Gov veto for stETH | Modules, fees, GOOSE | GOOSE/EGGs contributors | LDO | Dual-gov timelock | On-chain + escrow veto | Token vs staker conflict | **No** as dual-chain identity; **yes** as “distinct risk, distinct voice” |
| 8 | Yearn | Constrained delegation | Structure, fees, treasury, signers | yTeams day-to-day | stYFI Snapshot | yChad Safe (execute/veto) | Snapshot expected-binding *socially* | Multisig veto; key-person yTeams | **Yes** for ops not on ballot |
| 9 | Snapshot + Safe | Offchain signal, separate execution | Whatever the space allows | Safe signers | Threshold to propose | **Different system** (Safe / oSnap deprecated) | Snapshot block | Treating Snapshot as execution; strategy **sum** across chains | **Yes** — closest analog |
| 10 | ENS | Constitution + working groups | Social vs executable; quarterly funding | Elected stewards (discretion inside budget) | Token holders | WG multisigs | Snapshot + Governor | Steward capture; WG dissolution freeze | **Yes** for not voting every invoice |
| 11 | Gitcoin | Stewards / council experiments | Grants vs DAO | Core team vs stewards (unclear) | Informal | Multisigs | Snapshot / Tally | Steward role confusion | **No** as a role model; **yes** as warning |
| 12 | Compound | Governor Bravo + timelock | Comptroller, COMP | (weak guardian historically) | Threshold COMP | Timelock | `getPriorVotes` snapshot | Prop 62 bug; timelock delays **fixes**; flash N/A if snapshot holds | **No** governor; **yes** for snapshot vs flash |
| 13 | Curve | ve-lock + gauges | Gauge weights vs params | Convex/Yearn wrappers | veCRV | Gauge controller | On-chain | Bribe markets (Votium) | **No** ve; **yes** plutocracy honesty |
| 14 | Nouns | Governor + Prop House + fork | Every grant historically | Prop House for small grants | Nouns / sigs | Treasury | On-chain | Fatigue; last-minute swing; fork exit | **Yes** for not balloting every ops item |
| 15 | Cosmos Hub | `x/gov` typed msgs | Spend, upgrade, params, **text** | Validators | Deposit | Handler or none | On-chain | Deposit spam; signal unenforceable | **No** as product; **yes** for labeled signaling |

---

## 5. Case-study notes (short)

### 5.1 Terra Classic / LUNC (negative)

Mechanism: cosmos-sdk gov — `min_deposit` → voting period → tally (`Yes`/`No`/`NoWithVeto`/`Abstain`) → `Handler` if any. Independent docs stress the **execution gap**: text, funding, and roadmap items can pass without delivery ([proposal lifecycle](https://docs.terra-classic.money/governance/proposal-lifecycle/)). LCD params (fetched 2026-08-30 from `terra-classic-lcd.publicnode.com`): see Q7. Recent proposals mix tax params (12223), community-pool ceilings (12224 rejected), software upgrades, Hyperlane funding, and an explicit **signal** proposal (12219). That mix is the open marketplace. **Do not** copy deposit/vote UX or imply on-chain finality.

### 5.2 MakerDAO / Sky

Polls = sentiment / parameter choice. Executives = one spell. Risk teams propose numbers; holders ratify. GSM pause delay is a review window, not a second vote. Instant-access / MOM breakers are **execution** halt — not this API. Fit: template should look like a **poll that could become an ops spell elsewhere**, never like a LUNC forum dump.

### 5.3 Aave

[Process document v1](https://governance.aave.com/t/aave-governance-process-document-v1/18577): forum → Snapshot → on-chain, ~19+ days. Security SPs review payloads for malice/mismatch. ACI Skyward helps *write* ARFC/AIP; it is not a second token. Fit: #11 = completeness/safety gate; reviewers must not vote as a bloc.

### 5.4 Uniswap

[Uniswap Foundation governance](https://uniswapfoundation.org/governance): RFC 7d → Snapshot temp check 5d / 10M UNI yes → on-chain 10d / 40M UNI for, 1M UNI propose threshold. Snapshot GraphQL `uniswapgovernance.eth` (2026-08-30): strategy `uni` on network `1`, `voting.period` 432000s (5d), `quorum` 10_000_000. Temp check is the honest analog of CL8Y `advisory: true`. **Do not** hide that label (Arbitrum AIP-1 lesson).

### 5.5 Optimism Collective

Token House (economic) vs Citizens’ House (1p1v RetroPGF). Protocol upgrades: Developer Advisory Board + joint-house **veto**, not a second full electorate ([Operating Manual](https://github.com/ethereum-optimism/OPerating-manual/blob/main/manual.md), [upgrade process](https://gov.optimism.io/t/introducing-improvements-to-the-protocol-upgrade-process/10284)). Fit: veto/feasibility ≠ second tally. Do not add citizenship.

### 5.6 Arbitrum

[Constitution](https://docs.arbitrum.foundation/dao-constitution): Security Council 12 members, emergency 9/12 **no delay**, then transparency report. DAO can curtail SC via constitutional AIP. AIP-1: do not ask holders to “ratify” execution that already happened; keep `advisory: true` so a poll cannot be relabeled as DAO finality after ops moved funds.

### 5.7 Lido

[Dual Governance explainer](https://blog.lido.fi/dual-governance-101-explainer/): LDO decides; stETH can delay/rage-quit. GOOSE is annual *goals*, not daily ops. Fit: Terra vs BSC is **two token ledgers**, not staker vs governor. Do **not** merge identities to mimic “one holder.” Do keep “who bears the risk speaks separately.”

### 5.8 Yearn

[Governance and operations](https://docs.yearn.fi/contributing/governance/governance-and-operations): constrained delegation. Holders: fees, treasury spend, yTeam ratification, signer changes. yBrain/yDev/yGuard: strategies and emergencies. Multisig: execute or veto. Fit: **do not put every merge or RPC change on a ballot.**

### 5.9 Snapshot + Safe (closest analog)

Snapshot computes scores at a **snapshot block** (`blockTag = snapshot` else `latest`) — [strategy docs](https://docs.snapshot.box/developer-guides/voting-strategy). Do not confuse `erc20-balance-of` (proposal block freeze) with `erc20-balance-of-at` (OpenZeppelin **contract** `snapshotId`, a different machine). Combining strategies **sums** power. [Balancer](https://docs.balancer.fi/concepts/governance/voting.html) does this across multiple chains **only** where L2 BAL is treated as canonical wrap; CL8Y supplies are independently circulating, so the same sum is a double-count. oSnap/SafeSnap tried to *bind* Snapshot to a Safe; UMA oSnap support was end-dated **1 Dec 2025**. Most DAOs still have a **human Safe gap**. That gap is exactly CL8Y: signal here, execution elsewhere. Last-minute **wallet** transfers after freeze are useless; last-minute **vote overwrite** on Snapshot is still allowed. **Do not** add a second Snapshot strategy that sums Terra+BSC into one address.

### 5.10 ENS

[Working group rules](https://docs.ens.domains/dao/wg/rules): stewards elected annually; quarterly collective funding windows; discretion inside the budget; dissolution **freezes** WG funds. Constitution articles constrain spend. Fit: holders ratify **budget envelope**; leadership spends inside it. Do not vote every invoice.

### 5.11 Gitcoin (steward warning)

[Enhancing stewardship](https://gov.gitcoin.co/t/enhancing-our-stewardship-towards-a-more-unified-and-impactful-governance-approach/17099) and [council term-1 retro](https://gov.gitcoin.co/t/gitcoin-governance-council-first-term-retro/21773): unclear whether stewards consult or decide; core team still drove major moves; constitution delayed. [COI thread](https://gov.gitcoin.co/t/request-for-feedback-conflict-of-interest-with-steward-voting/14971): voting your own workstream budget treated as invalid by default. 2025: treasury moved off Governor to a Safe; votes become **instruction** to signers ([security update](https://gov.gitcoin.co/t/security-update-treasury-protection-governance-transition-what-we-did-and-why/25228)). Fit: if #11 has a committee, **write the rights** (open/refuse poll only). A passed CL8Y poll is the same class as Gitcoin Snapshot, not Tally.

### 5.12 Compound / Beanstalk / Curve / Nouns / Cosmos Hub

Covered in the table and Q7. Compound snapshot + timelock: flash N/A; Prop 62 shows executable governors need a **narrow guardian pause** — still not this repo. Curve: bribes. Nouns: fatigue + fork exit. Cosmos Hub: typed msgs vs unenforceable text — CL8Y is *all* text/signal; say so.

---

## 6. Vote-scope matrix (CL8Y)

Columns: **L** leadership decides · **R** leadership proposes + holders ratify · **H** holders originate (permissionless ballot) · **X** out of scope for this repo.

| Topic | Stage 1 (now) | Stage 2 (#10/#11) | Later / never |
|-------|----------------|-------------------|---------------|
| Protocol engineering (how to implement a pool, indexer, wallets) | **L** | L | Never **H** |
| Wallet / RPC / Coolify / rate limits | **L** | L | X as a poll |
| Legal clickwrap / property admin | **L** (ops in legal repo) | L | Never skip via “emergency” poll |
| Day-to-day marketing | **L** | L | — |
| LP depth / asset mix / community treasury size | Informal / none | **R** (recommended + spectrum + tranche/halt) | Execution **X** |
| Token issuance / unlocks | **R** if it happens at all | **R** | Needs counsel |
| Fundamental governance rules (what may be voted, committee existence) | — | **R** | Not **H** open marketplace |
| DEX factory wasm admin / fees | **X** | X | Never from this app |
| On-chain governor / CosmWasm voting contract | **X** | X | X |
| Merge terra1 + 0x | **X** | X | X |
| Archive-node historical electorate | **X** | X | X |

**Holders originate (H)** is out of scope for major strategy: anyone with ≥1000 CL8Y can *draft* (#11) but opening a vote is a feasibility gate, and the ballot is still recommended-option yes/no, not N competing strategies. Stage 1 today is closer to H (permissionless `POST /v1/proposals` immediately `open`) — that is why #11 exists.

---

## 7. Mapping to #10 and #11 (additive only)

Do not rewrite those issues’ APIs.

**#10 template — emphasize:**

| Section (issue #10) | Additive emphasis from this research |
|----------------------|--------------------------------------|
| Problem | Decision class: ops vs major risk/fund/issuance/governance |
| Solution | **One recommended option** and why alternatives were rejected |
| Context | Bounded spectrum if useful (depth/asset/issuance points), not an open menu |
| Pros/cons | Include execution risk and **who executes off-API** |
| Summary | TL;DR must state recommended option + advisory |
| Success criteria | Goalposts for spend/outcome; not slashing |
| *(optional later field)* | Tranche sizes, review window, **halt conditions** (must be in the ratified text) |
| *(optional later field)* | “Official recommendation” signer/ops URL (not a display name) |

Independent analysis is **not** a proposer-owned field (#10 already says so).

**#11 review gate — emphasize:**

- Reviewer may **refuse to open** if sections incomplete, infeasible, unfunded, legally undescribed, or missing a recommended option.
- Reviewer must **not** vote as a bloc, rewrite holder tally, or open a competing “expert” ballot.
- AI optional, labeled, non-authoritative (already in #11).
- Freeze at **open**, not draft create (already in #11).
- Allowlist is a **key-compromise surface**: document rotation in OPS, not on-chain magic.
- Dual-chain: committee addresses may be Terra **or** `0x`; actions stay per address.

---

## 8. Continuity (documentary)

If the founder is unavailable:

**Still works:** existing signatures, ledger, public `advisory` copy, this note, OPS/LEDGER docs.

**Breaks:** Coolify, GitLab, restricted `DATABASE_URL`, blacklist env, Legal admin, DNS.

**Actions (no new keys in git):**

1. Dual-control / documented deputies for Coolify, GitLab, Legal admin, blacklist env ([OPS.md](OPS.md)). Pattern: [ENS Meta-Gov runbook](https://github.com/ensdao/metagov-runbook) (onboarding, signer verification, per-term archive) — not Harmony/Wintermute *loss* cases.
2. Published ops channel + at least one **signing address** for “official recommendation” so impersonation is harder.
3. If #11 ships, two-person rotation of the committee allowlist. Temporary permissions that are not revoked are an attack (Ronin leftover allowlist).
4. Sunset “temporary” ops powers with a date (Yearn [YIP-41](https://docs.yearn.fi/contributing/governance/yips/yip-41) six-month clock drifted into YIP-61). Write expiry + what happens if the replacement is late.
5. Do not claim the electorate runs production. Bitcoin continued via **written protocol + independent operators**, not a token vote. BitDAO’s documented 60% Bybit allocation is the cautionary sponsor-theater pattern.

---

## 9. Attack catalog

| # | Vector | Comparable | Stage 1 | #10/#11 | Follow-up |
|---|--------|------------|---------|---------|-----------|
| 1 | Low-information / herd | LUNC 12223 abstain-plurality pass | Advisory copy only | Structured sections, not longer HTML | — |
| 2 | Agenda flooding | LUNC mixed types; Nouns volume | 1000 CL8Y weak | Leadership-proposes + open-gate | Do not add permissionless executable payloads |
| 3 | Whale / plutocracy | UNI 40M quorum; Curve bribes | **Explicit** token-weight | Disclose in UI copy | Do not pretend 1w1v |
| 4 | Flash-loan / same-block | Beanstalk 2022 | **N/A** if unregistered+freeze | — | Copying `governor.sol` would re-introduce it |
| 5 | Identity merge / sybil | Snapshot multi-strategy **sum** | Hard rule: no merge | Same | No social-graph linking as v1 identity |
| 6 | Committee capture | Gitcoin unclear stewards | No committee yet | Feasibility ≠ electorate | Rotation runbook |
| 7 | Impersonation | Fake “official rec” | Weak | Attribute to signed addr / ops URL | Product copy issue sketch |
| 8 | Process theater | Arbitrum AIP-1; Tribe/Fei 2022 Snapshot treated as mandate then reversed | Keep `advisory: true` | Do not hide it | — |
| 18 | Leftover ops allowlist | Ronin 2022: unrevoked gas-free RPC allowlist | Blacklist/committee env | Document revoke | #11 allowlist rotation |
| 9 | Scope creep to execution | Factory / treasury | Architecture forbids | Repeat in template | Never bind factory multisig from this app |
| 10 | Continuity / ops keys | Single operator | OPS.md | — | Rotation, not a voting feature |
| 11 | Tranche griefing | Halt in bad faith | N/A | Plan **inside** proposal | — |
| 12 | Legal bypass | “Emergency” skip terms | Fail closed | Same | Out of scope to weaken Legal |
| 13 | Research supply chain | Hallucinated DAOs | This doc URLs | — | Flag unsourced claims |
| 14 | Privacy leak | Re-paste private chat | This issue forbids | Reviewer grep | — |
| 15 | Copyright dumps | Verbatim whitepapers | Not pasted | — | — |
| 16 | L9 buy-before-freeze | Documented | Counted | Disclose | Lockup = later issue |
| 17 | Dump-after-freeze | Same | Weight unchanged | Disclose | — |
| 19 | Stale registered weight | UNI temp-check miss (5.35M vs 10M); Arbitrum votable-supply quorum | Registration is the denom; silence inflates it | Copy: turnout of *rolls* | Roll TTL / missed-vote prune |
| 20 | Only-cast / empty ballot | BonkDAO 2026-07 (1% supply, 7 wallets, ~$20M drain); Build Finance 2022 | Tally is cast-only; no quorum in API; **advisory** so no treasury handler | Do not relabel as “passed” | Floor on registered turnout; keep execution off this API |
| 21 | Concave weight theater | Snapshot QV sybil warning; Circle arXiv:2605.18990 | 1:1 token weight | Same | Do **not** add a curve |
| 22 | Wallet VP exemption | Curve/Convex wrap; special wallets | Blacklist exists; no boost | — | Veto *role*, not exempt wallets |
| 23 | Operator-peek “secret ballot” | Snapshot Shutter vs MACI; GET `/votes/:addr` is public | Choices in Postgres plaintext | Hide *running* tally only | No MACI contracts |
| 24 | Re-register swarm | EOS decay gamed by revote; cheap signatures | Legal + register friction | — | Cooldown after prune |
| 25 | Live-tally herding | Aave ARFC 12730; Nouns last-minute swing | API returns `tally`; UI does not render it yet | Do not add a live bar without a hide-until-close plan | Shielded-until-close |
| 26 | Permanent anonymity | Shutter “permanent” PoC; ACI delegate-accountability objection | Comments show `wallet_address` | Keep draft comments attributed | Never hide #11 reviewers |

---

## 10. Explicit non-recommendations

Do **not**:

- Add a CosmWasm voting contract or `governor.sol` / Solidity snapshot governor in this repo.
- Use archive LCD or historical BSC backfill to enlarge the electorate.
- Merge `terra1…` and `0x…` (or Snapshot-sum them as one voter).
- Bind this API to DEX factory multisig control from this app.
- Copy LUNC deposit/vote UX (bonded-token gov, validator-default votes, mixed executable/text).
- Hide `advisory: true` or relabel polls as on-chain DAO finality.
- Let a committee recast holder outcomes.
- Skip Legal clickwrap for a governance emergency.
- Treat Snapshot oSnap as a required executor (deprecated; execution stays off this API).
- Add a concave / quadratic / “Tier 9 dampener” vote-weight curve (wallet-split theater; dual-chain already two identities).
- Set quorum on circulating supply (LP, bridge, and discount-trading wallets are not the electorate).
- Treat “only cast votes count” as a pass rule with **no** registered-turnout floor.
- Promise a true secret ballot (MACI / receipt-free). The operator holds plaintext choices; `GET /v1/proposals/:id/votes/:addr` is unauthenticated.
- Use a 50% token position as a veto, or grant curve-exempt wallets. Assign a **veto role** (env), distinct from the #11 open-gate committee.

---

## 11. Dual-chain check (every recommendation)

| Recommendation | Terra | BSC |
|---------------|-------|-----|
| Leadership proposes, holders ratify | Registered `terra1` weight at freeze | Registered `0x` weight at freeze |
| Feasibility committee | May include Terra addrs | May include `0x`; no merge |
| Electorate complete only if both can register/vote | Yes | **Core** ([AGENTS.md](../AGENTS.md) rule 3) |
| Official recommendation attribution | ADR-36 address or ops URL | EIP-191 address or same ops URL |
| Template / review | Same schema | Same schema |
| Quorum denom (if added later) | Registered Terra freeze-set | Registered BSC freeze-set; **not** summed |
| Roll TTL / prune | Per `terra1` registration | Per `0x` registration; no cross-chain prune |
| Veto role | May be a Terra addr | May be `0x`; not 50% of either supply |

---

## 12. Follow-up issue sketches (do not file unless a maintainer asks)

1. **`docs/copy`: attribute official recommendations** — Product copy must name a signing address and/or published ops channel. Display names are not authz. Complements #10; no schema required if copy-only.
2. **`feat(voting): proposal decision-class` (after #10)** — Optional enum `operational` vs `major_risk_fund` for list filters. Must not become a second electorate. Leadership still proposes majors.
3. **`ops: committee/blacklist/Legal rotation runbook`** — Extend [OPS.md](OPS.md) for key-person: who rotates `VOTING_BLACKLIST_ADDRESSES`, future #11 allowlist, Legal admin. Not a voting feature.
4. **`feat(voting): recommendation + spectrum fields`** — Only if #10’s freeform sections prove too weak: explicit `recommended_option` + `risk_bounds` JSON. Do not implement in #13.
5. **Never-file from this note:** on-chain governor; identity merge; executable treasury from `operator-voting`; factory fee control; concave vote-weight curve; MACI / CosmWasm privacy voting; circulating-supply quorum.
6. **`docs/copy` then later `feat(voting): quorum-on-registered`** — If polls ever get a “passed” label: quorum = fraction of the **frozen registered set**, not circulating supply. Abstain counts toward quorum (Snapshot basic; Cosmos). Silence does **not**. Pair with roll hygiene: Optimism-style **activity window** (6-month utilized VP) or Hive HF25-style prune, not Cosmos validator inheritance. Floor so only-cast cannot pass a 5-wallet ballot. Do not implement in #13.
7. **`feat(frontend): hide running tally until close`** — Herding fix (Snapshot/Shutter analog) without claiming secret ballot. Restrict `GET .../votes/:addr` to the signer or delay it until close. Keep draft **comments attributed**. Operator still sees Postgres — disclose that. Do not publish a voter list plus exact per-choice totals and then claim whale privacy (B-Privacy).
8. **`ops: veto role env`** — Named Guardian (Yearn yChad analog): nullify / refuse execution, cannot propose, cannot recast tally. Distinct from `VOTING_COMMITTEE_ADDRESSES` (#11 open-gate) and `VOTING_BLACKLIST_ADDRESSES`. Rotation in OPS. Because polls are advisory, this is an ops/copy rule until a later status exists.

---

## 13. Simulations (`/tmp/gov-research/`)

Not CI. Illustrative:

| Sim | Result |
|-----|--------|
| Token-weight vs 1w1v | 80k whale beats 100×100; 1w1v would reverse. Product is token-weighted. |
| Identity merge | Separate 1000+1000 vs Bob 1500: merge-max makes Bob win; sum/separate makes Alice win. Do not merge. |
| Open menu vs yes/no | Plurality can “win” while yes/no on that option **fails** (45 vs 55). Honest ratification. LP-coalition framing can pass 70–30 — framing is leadership’s job in the write-up, not extra ballot options. |
| Tranche+halt | Halt after t3 spends 7.5M of 10M. Halt is ops. |
| Flash vs freeze | Live-balance governor: flash wins. Unregistered flash on CL8Y = 0. Registered buy-before-freeze = L9. |
| 12223 LCD | Abstain largest; tax still passed on yes/non-abstain ≈ 52.6%. |
| Stale turnout (`/tmp/gov-research-13b/`) | 40% of circulating unreachable; 40% of registered fails with a silent whale; only-cast 80% “passes” and a 5-of-100 attack also “passes.” Piecewise 10k vs 5k = 1.14× weight; split 2×5k restores 1:1. Founder 50k vs ten 5k wallets → 18% curve share. Prune missed>3 drops denom 39k→19k. Silent whale cannot Cosmos-veto. |

---

## 14. Second-pass additions (parallel case-study reviews)

Folded from deeper primary-source passes after the first draft. Do not treat these as a change of hybrid recommendation.

| Finding | Why it matters for CL8Y | Source |
|--------|---------------------------|--------|
| Tribe/Fei 2022: Snapshot majority to reimburse, then core treated it as non-binding | Process theater is *either* “community voted” without `advisory` *or* ignoring a labeled signal without a written rule | [Decrypt](https://decrypt.co/108298/how-a-tribe-dao-revote-shows-the-flaws-in-dao-governance); Uniswap/Aave **label** temp checks |
| Balancer Snapshot **sums** BAL across seven chains | Additive strategies assume a **partition of one supply**. CL8Y CW20 and BEP-20 are independent issuance | [Balancer voting](https://docs.balancer.fi/concepts/governance/voting.html) |
| Aave v3: voting UX on L2, **balances from Ethereum** | Honest hub. Do not copy the UX if it looks like a merge | [Aave governance](https://www.aave.com/docs/ecosystem/governance) |
| Aave Risk Steward: ≤50% cap increase, 5-day frequency | Holders ratify the **envelope**; ops move inside it | [Process document](https://governance.aave.com/t/aave-governance-process-document-v1/18577) |
| Uniswap UAC: ops/escrow, “not evaluatory on strategy” | Feasibility/ops arm ≠ second electorate | [UAC charter](https://gov.uniswap.org/t/uniswap-accountability-committee-charter/25807) |
| Maker DSChief 1.2: `lock`/`free` cannot share a block | Flash-loan vote on live-balance Chief is a **governor** problem; CL8Y freeze already N/A | [Forum](https://forum.makerdao.com/t/dschief-1-2-flash-loan-protection-for-maker-governance/5115) |
| Lido Easy Track: authorized addresses, 72h, passes unless 0.5% LDO objects | Optimistic ops with a named starter list — that list is the attack surface | [Easy Track](https://docs.lido.fi/guides/easy-track-guide/) |
| Lido GOOSE: contributors propose goal bundles; holders consent on Snapshot | Closest published analog to leadership-recommends, holders-ratify **goals** | [GOOSE genesis](https://research.lido.fi/t/the-guided-open-objective-setting-exercise-goose-proposal-a-genesis-step-to-jump-start-a-dao-wide-goal-setting-exercise-and-cadence/5355) |
| Optimism Working Constitution is transitory (max four years from Apr 2022) | Do not freeze a Bedrock-style charter on day one; Operating Manual > constitution for stage 1 | [Working Constitution](https://gov.optimism.io/t/working-constitution-of-the-optimism-collective/55) |
| Arbitrum AIP-1.1: remaining foundation ARB on 4-year lockup after the failed omnibus | After theater, split constitutional vs funding | [AIP-1.1](https://forum.arbitrum.foundation/t/proposal-aip-1-1-lockup-budget-transparency/13360) |
| Yearn YIP-88 (2025): revenue teams + yBudget II until on-chain budget votes | Constrained delegation is still evolving; copy **enumerated powers**, not the latest org chart | [YIP-88](https://docs.yearn.fi/contributing/governance/yips/yip-88) |
| BitDAO tokenomics: Bybit 15% flexible + 45% locked = 60% | Holder polls can **ratify** a sponsor; they do not dissolve key-person risk | [BitDAO tokenomics](https://docs.bitdao.io/litepaper-1/tokenomics) |
| Uniswap v2 LM: 18 Sep–17 Nov 2020, `endTime` halt | Timebox is a halt without a new vote | [UNI blog](https://blog.uniswap.org/uni) |
| Optimism Collective Grant Policies: refuse next milestone; clawback locked OP | Halt the **next tranche**, not `POST /v1/votes` | [Grant policies](https://gov.optimism.io/t/collective-grant-policies/5833) |
| Do not confuse L2Beat rollup “stages” with this issue’s stage 1/2 | Different framework (training wheels on a rollup) | [l2beat.com/stages](https://l2beat.com/stages) |
| Securities / DAO-as-association | Public cases cluster on treasury control, profit, and votes that run the business. Advisory ≠ automatic safe harbor | SEC [34-81207](https://www.sec.gov/files/litigation/investreport/34-81207.pdf); **needs counsel** |

---

## 15. Follow-up research: stale weight, turnout, secrecy, curves, veto (2026-08-30)

Themes synthesized from a later private community discussion. Individual messages, handles, and quotes are **not** reproduced. Hybrid recommendation from §2 is unchanged: leadership proposes, holders ratify, polls stay advisory. This section answers “what else besides 1:1 weight vs only-cast vs a dampening curve?”

### 15.1 The problem (stale registered weight)

A large holder can register (and therefore sit in the freeze-set) and then never vote — fear of herding, indifference, or a discount-trading bot that registered once. If quorum (or “mandate”) is a fraction of that freeze-set, silence **raises** the bar and can stall every poll. If only ballots that were cast count, a small coordinated set can “pass” a harmful yes/no while everyone else stays home.

Stage 1 today has **no quorum**. `GET /v1/proposals/:id` returns `tally` as a sum of **cast** weights (`operator-voting` `db::tally`). The UI does not yet render that tally. Relabeling a live tally as “passed” without a written rule would be process theater (Tribe/Fei lesson, §14).

### 15.2 Option 1 — only cast votes count

**What it is:** majority of `{for, against}` among ballots received. Snapshot *type* `basic` counts `abstain` toward quorum but not toward yes/no. Cosmos `x/gov` does the same for threshold, and uses **bonded/staked** supply as the quorum denominator (LCD 2026-08-30: quorum 0.40, threshold 0.50, `veto_threshold` 0.334).

**Failure:** no floor. Sim `minority_only_cast`: 5 attackers vs 95 home = 100% yes and “passes.” Uniswap Snapshot `[Temp Check] - Four for V4` (fetched 2026-08-30): **5.35M / 10M quorum**, 118 votes, scores ≈ `[5.35M, 0, 1.8k]` — almost unanimous For, still **below quorum**. That is the opposite failure (unanimity without enough weight). Compound Governor Bravo `quorumVotes = 400000e18` is a **fixed for-vote floor**, not “whoever showed up.”

**CL8Y:** do not adopt only-cast as a pass rule. Keep it as the *tally display* (already true). If a later issue adds “passed,” add a registered-turnout floor.

### 15.3 Option 2 — concave / exponential weight curve

**What it is:** 1:1 up to a cap (e.g. 5k), then diminishing extras (sketch: 10k → 1.14× the 5k weight). Snapshot [quadratic voting type](https://docs.snapshot.box/proposals/voting-types) documents the intent (dilute whales) and the **cons**: whales split wallets unless a Sybil-resistance validation exists. Circle Research [arXiv:2605.18990](https://arxiv.org/html/2605.18990) (AFT 2026): *no* positive concave wallet rule stays anti-plutocratic on a permissionless chain; Sybil-optimal power is asymptotically linear. Empirical QV amplification on ENS/Compound/Uniswap/Arbitrum/ZKsync: **1,172×–4,039×**.

Sim `curve_sybil`: piecewise 10k in one wallet = 5700 weight; split 2×5k = 10000 (1.75×). `sqrt` 10k / 10k wallets restores full linear. Dual-chain identity (already two addresses) plus no proof-of-personhood makes this **worse** than a single-chain DAO.

**Wallet exemptions** to preserve a ~50% token veto under a curve are special voting rights. That is a **role**, implemented as the most attackable form (magic wallets). Curve ve + Convex/Yearn wrappers are the historical “votes based on wallets become theater” pattern (already in §5.12 / attack #3).

**CL8Y:** do **not** ship a curve. Keep 1:1 at freeze. Be honest that this is plutocracy.

### 15.4 Another way (recommended combination)

| Lever | Do | Do not |
|-------|----|--------|
| Weight | 1:1 registered balance at freeze | Concave curve, 1w1v theater, Terra+BSC merge |
| Quorum denom | Frozen **registered** set (per chain) | Circulating supply (LP, bridge, CEX, discount bots) |
| Silence | Not a vote. Abstain is explicit and counts toward quorum | Auto-against; only-cast pass with no floor |
| Rolls | TTL (e.g. 90 days) **or** prune after N missed *open* polls | Permanent registration that freezes liveness |
| Herding | Hide **running** for/against until close (Shutter analog) | Promise receipt-free privacy; hide #11 comments |
| Veto | Named **role** (env), nullify/refuse execution, cannot propose | 50% token share; curve-exempt wallets; #11 recasting tally |
| Bots / 30% | Assume most discount-traders **never register** (Legal + signature). Measure turnout of rolls | Treat 30% of circulating as the turnout story |

**Why registration is the right denom (product, not just ops):** (1) LP and bridge wallets cannot vote; circulating quorum would bake them in and force the bar down or stall it. (2) The ledger already indexes **registered** wallets only (L2). Arbitrum’s 2025–26 [DVP quorum](https://forum.arbitrum.foundation/t/constitutional-aip-dvp-quorum/30053) is the public analog: votable *supply* drifted away from tokens **registered to vote**, constitutional proposals started failing, and the fix is quorum as a fraction of **delegated/registered** power with a baseline floor.

**Roll hygiene:** EOS `stake2vote` decays un-revoted weight (~1 year half-life) rather than deleting voters — still requires a periodic action. A 90-day re-register or “missed > N open polls → drop from future freeze-sets” is the same idea. Attack: bots re-sign before the next freeze (signature is cheap). Mitigations already in-tree: Legal clickwrap, ≥ live balance to matter. Additive: cooldown after prune so a swarm cannot appear the same day.

**Bots holding supply for fee tiers:** if they never register, they are **out of the electorate** — that is the design. If they register and go dormant, TTL/prune. Do not wait for a 30% circulating turnout that will never exist.

### 15.5 Secret ballot vs shielded-until-close

Three different products:

1. **Hide running tally** (herding). Snapshot/Shutter: choices encrypted until close, then **fully revealed**. Aave [ARFC 12730](https://governance.aave.com/t/arfc-private-voting-for-aave-governance-2-month-trial/12730) trial May–Jul 2023: unique voters **32.6k → 14.2k**, total votes **879.9k → 119k**. ACI/delegates objected that public votes are how delegators audit them. Paladin: you cannot rally or detect collusion until it is too late (a16z-style last-minute swing would be invisible).
2. **Permanent secret ballot** (never reveal who voted for what). Shutter [permanent shielded voting](https://blog.shutter.network/permanent-shielded-voting-is-coming-to-snapshot/) PoC (ElGamal + ZK) is **not** this stack. MACI ([maci.pse.dev](https://maci.pse.dev/blog/maci-1-0-technical-introduction)) is collusion-resistance via contracts + coordinator SNARKs — **forbidden** here (no new contracts).
3. **Operator-honest hide.** `operator-voting` stores `choice` in Postgres. `GET /v1/proposals/:id/votes/:addr` returns choice **without** a signature (anyone who knows an address — including from public comments — can look it up). A UI that “doesn’t show who voted” is not a secret ballot.

**CL8Y fit:** if herding/intimidation is the actual fear, hide the **running** tally and delay per-address choice until close (or authz that GET). Disclose that ops can still read the DB. Keep draft comments and committee analysis **attributed** (#11 accountability). Do not expect turnout to rise (Aave trial went the other way). Do not hide who opened the poll or who wrote independent analysis.

### 15.6 Veto: role vs token share

A silent ~50% holder **cannot** token-veto (sim `veto_mechanics`). Cosmos `NoWithVeto` is a share of **participating** power, not of supply — the vetoer must show up. Yearn [yChad Guardian](https://docs.yearn.fi/developers/security/multisig) (YIP-81): *nullify a proposal or governance decision but cannot make proposals*; signers rotate via YIPs. Optimism Foundation retains a **cancel** / manager role after Security Council admin transfer ([Onchain Controls MVP](https://gov.optimism.io/t/governor-upgrade-proposal-onchain-controls-mvp/10371)).

**CL8Y:** assign `VOTING_VETO_ADDRESSES` (or equivalent) as a documented role. Distinct from:

| Env | Right |
|-----|--------|
| `VOTING_COMMITTEE_ADDRESSES` | Open or refuse a *poll* (#11). Not the outcome. |
| `VOTING_BLACKLIST_ADDRESSES` | Exclude an address from the electorate. |
| Veto role (sketch) | Nullify / refuse **execution** of a ratified advisory mandate. Cannot propose. Cannot rewrite the tally. |

Because votes are `advisory: true`, a veto is already possible as ops (don’t execute). Writing it as a role is honesty and rotation, not a new governor. Do not encode a 50% token veto; do not grant curve-exempt wallets.

### 15.7 Stage 1 vs later

Do **not** block #7–#9 or #10/#11 on quorum, TTL, shielded tally, or a veto env. Stage 1 stays impartial dual-chain advisory snapshots. Document the intended interpretation now so a later issue does not invent only-cast or a weight curve from one conversation.

### 15.8 Second-pass fold-in (parallel reviews)

Hybrid recommendation unchanged. New evidence after the first write of this section:

| Finding | Why it matters for CL8Y | Source |
|--------|---------------------------|--------|
| BonkDAO 2026-07-06: 1% of supply (~$4.4M) bought quorum; **7 wallets**; ~$20M treasury moved; **0s hold-up** | “Only those who vote count” plus an executable treasury is an empty-room capture. CL8Y stage 1 has **no** handler — keep `advisory: true` and a registered-turnout **floor** if “passed” is ever labeled | [CoinDesk](https://www.coindesk.com/markets/2026/07/07/bonk-faces-usd20-million-treasury-drain-after-attacker-spends-usd4-million-to-pass-malicious-proposal) · [QuillAudits](https://www.quillaudits.com/blog/hack-analysis/bonk-dao-governance-takeover-exploit) |
| Build Finance 2022-02: low threshold + unnoticed proposal → mint keys + ~$470k | Same family as Bonk: cheap quorum, no guardian | [The Block](https://www.theblock.co/post/134180/build-finance-dao-suffers-hostile-governance-takeover-loses-470000) |
| Optimism Token House quorum = **30% of active votable OP** (delegated VP **used in the last 6 months**); abstain counts | Better roll-hygiene analog than EOS weekly recast. Do **not** copy Cosmos “inherit the validator’s vote” | [Operating Manual](https://github.com/ethereum-optimism/OPerating-manual/blob/main/manual.md) |
| Hive HF25: witness/proposal votes expire after **1 year** with no governance action | Inactivity prune of the *roll*, not of the token | [HF25 announcement](https://hive.blog/hive/@hiveio/hive-hardfork-25-is-on-the-way-hive-to-reach-equilibrium-on-june-30th-2021) |
| Falk et al.: ~**5% of total supply** cast (Aave 3.2%, UNI 4.6%, COMP 7.7%, Lido 5.6%). Feichtinger et al.: **~20–40% of delegated** VP (COMP 32%, UNI 21%, ENS 39%) | **30% of circulating is unattainable** for dual-use tokens. **30% of the registered freeze-set is ordinary** | [arXiv 2407.10945](https://arxiv.org/html/2407.10945) · [arXiv 2302.12125](https://ar5iv.labs.arxiv.org/html/2302.12125) |
| Uniswap Franchiser **12.5M UNI** treasury stuffing then recall; Balancer cut quorum **50%** when Aura stopped voting | Circulating/assumed-bloc denominators force stuffing or panic cuts. Registration denom shrinks honestly if bots leave | [UF proposal 97](https://vote.uniswapfoundation.org/proposals/97) · [BIP-924](https://forum.balancer.fi/t/bip-924-exclude-aura-from-governance-and-reduce-quorum/7092) |
| Snapshot Shutter: **who voted** is still visible; choice reveals at close. SSRN: overall turnout **falls**; top 5% **unaffected** | Hide-until-close is herding, not “whales will show up.” Permanent secrecy is the dictator-optics test; barely deployed | [ENS temp-check](https://discuss.ens.domains/t/temp-check-shielded-voting-for-ens-snapshot-proposals/22142) · [SSRN 4982940](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=4982940) |
| B-Privacy: voter list + exact weighted totals often reconstruct a whale’s ballot | Do not publish turnout roll **and** exact for/against/abstain and then claim privacy | [arXiv 2509.17871](https://arxiv.org/abs/2509.17871) |
| Compound Bravo quorum is **For-only**; OZ Governor counts **For+Abstain**. Snapshot X: Against does **not** help | If CL8Y adds quorum, pick explicitly: Snapshot-basic (abstain helps) vs Bravo (for-only) | Bravo `state()` · OZ `GovernorCountingSimple` |
| Snapshot `anti-whale` strategy: inflection + static multiplier “to reduce infinite incentive for multiple wallet exploits” | Vendor documents the split; the patch is a fudge, not identity | [anti-whale.md](https://github.com/snapshot-labs/snapshot-docs/blob/master/user-guides/spaces/space-handbook/anti-whale.md) |
| Compound Golden Boys / Humpy 2024: linear + 400k COMP made the capture **visible** | A curve would have rewarded splitting the same COMP. Keep 1:1 | [The Defiant](https://thedefiant.io/news/defi/compound-community-accuses-notorious-whale-of-engineering-governance-attack) |
| Lido Easy Track: named starters, 72h, passes unless **0.5% LDO** objects | Optimistic **ops** invert turnout. Do not use for major fund/risk ratification | [Easy Track](https://docs.lido.fi/guides/easy-track-guide/) (already in §14) |

---

## 16. Sources (fetched or confirmed 2026-08-30)

Primary / official (prefer these):

- Terra Classic lifecycle: <https://docs.terra-classic.money/governance/proposal-lifecycle/>
- LCD gov params: `https://terra-classic-lcd.publicnode.com/cosmos/gov/v1/params/{voting,deposit,tallying}`
- LCD proposal 12223: `https://terra-classic-lcd.publicnode.com/cosmos/gov/v1/proposals/12223`
- Maker MIP16: <https://github.com/makerdao/mips/blob/master/MIP16/mip16.md>
- Sky/Maker executive example: <https://raw.githubusercontent.com/sky-ecosystem/community/master/governance/votes/Executive%20vote%20-%20October%2004%2C%202024.md>
- Maker ESM: <https://docs.makerdao.com/smart-contract-modules/shutdown/emergency-shutdown-module.md>
- Aave process: <https://governance.aave.com/t/aave-governance-process-document-v1/18577>
- ACI Skyward: <https://governance.aave.com/t/introducing-skyward-a-free-service-for-aave-dao-by-aave-chan-initiative/13173>
- Uniswap Foundation: <https://uniswapfoundation.org/governance>
- Snapshot GraphQL space `uniswapgovernance.eth` via `https://hub.snapshot.org/graphql`
- Snapshot strategies: <https://docs.snapshot.box/user-guides/voting-strategies>
- Snapshot strategy `blockTag`: <https://docs.snapshot.box/developer-guides/voting-strategy>
- Optimism houses: <https://gov.optimism.io/t/about-the-optimism-collective/6118>
- Optimism future governance: <https://optimism.io/blog/the-future-of-optimism-governance>
- Arbitrum constitution: <https://docs.arbitrum.foundation/dao-constitution>
- AIP-1 forum: <https://forum.arbitrum.foundation/t/aip-1-arbitrum-improvement-proposal-framework/30>
- Lido Dual Gov: <https://blog.lido.fi/dual-governance-101-explainer/> · <https://lido.fi/governance>
- Yearn ops: <https://docs.yearn.fi/contributing/governance/governance-and-operations>
- Yearn YIP-61: <https://docs.yearn.fi/contributing/governance/yips/yip-61>
- ENS WG rules: <https://docs.ens.domains/dao/wg/rules>
- Cosmos Hub text/signaling: <https://docs.cosmos.network/hub/latest/governance/proposal-types/text-prop>
- Uniswap OP LM RFC: <https://gov.uniswap.org/t/rfc-the-optimism-uniswap-protocol-liquidity-mining-program/17820>
- Beanstalk flash-loan postmortem (secondary, cited): <https://veridise.com/blog/audit-insights/flash_loan_governance_vulnerability_beanstalk_182m/>
- Nouns V3 / fatigue: <https://nouns.wtf/vote/356>
- Gitcoin stewardship: <https://gov.gitcoin.co/t/enhancing-our-stewardship-towards-a-more-unified-and-impactful-governance-approach/17099>
- Gitcoin council retro: <https://gov.gitcoin.co/t/gitcoin-governance-council-first-term-retro/21773>
- Compound Governor Bravo: <https://github.com/compound-finance/compound-protocol/blob/master/contracts/Governance/GovernorBravoDelegate.sol>
- veToken bribes (academic): <https://arxiv.org/pdf/2311.17589>
- Tribe/Fei Snapshot vs later reverse (secondary): <https://decrypt.co/108298/how-a-tribe-dao-revote-shows-the-flaws-in-dao-governance>
- Balancer multi-chain sum: <https://docs.balancer.fi/concepts/governance/voting.html>
- Aave hub balances: <https://www.aave.com/docs/ecosystem/governance>
- Uniswap UAC charter: <https://gov.uniswap.org/t/uniswap-accountability-committee-charter/25807>
- Maker DSChief 1.2: <https://forum.makerdao.com/t/dschief-1-2-flash-loan-protection-for-maker-governance/5115>
- Lido Easy Track: <https://docs.lido.fi/guides/easy-track-guide/>
- Lido GOOSE: <https://research.lido.fi/t/the-guided-open-objective-setting-exercise-goose-proposal-a-genesis-step-to-jump-start-a-dao-wide-goal-setting-exercise-and-cadence/5355>
- Optimism Working Constitution: <https://gov.optimism.io/t/working-constitution-of-the-optimism-collective/55>
- Yearn YIP-41: <https://docs.yearn.fi/contributing/governance/yips/yip-41>
- ENS Meta-Gov runbook: <https://github.com/ensdao/metagov-runbook>
- Gitcoin treasury-to-Safe: <https://gov.gitcoin.co/t/security-update-treasury-protection-governance-transition-what-we-did-and-why/25228>
- Optimism grant halt: <https://gov.optimism.io/t/collective-grant-policies/5833>
- Uniswap UNI LM timebox: <https://blog.uniswap.org/uni>
- Immunefi Beanstalk (same-tx): <https://immunefi.com/blog/bug-fix-reviews/hack-analysis-beanstalk-governance-attack-april-2022/>
- SEC DAO report: <https://www.sec.gov/files/litigation/investreport/34-81207.pdf>
- Snapshot shielded voting (Shutter): <https://docs.snapshot.box/user-guides/spaces/settings>
- Snapshot voting types / QV sybil warning: <https://docs.snapshot.box/proposals/voting-types>
- Snapshot GraphQL (2026-08-30, browser UA): `https://hub.snapshot.org/graphql` space `uniswapgovernance.eth` quorum 10_000_000, `privacy` empty; proposal `0x5ae3…54ee` Four for V4 scores_total 5_349_528 vs quorum 10_000_000
- Compound `quorumVotes = 400000e18`: <https://github.com/compound-finance/compound-protocol/blob/master/contracts/Governance/GovernorBravoDelegate.sol>
- Cosmos tally (quorum of bonded, veto of participating, abstain in quorum): <https://github.com/cosmos/cosmos-sdk/blob/main/x/gov/keeper/tally.go>
- Arbitrum DVP quorum (registered/delegated denom): <https://forum.arbitrum.foundation/t/constitutional-aip-dvp-quorum/30053>
- Arbitrum constitutional quorum 5%→4.5% interim: <https://forum.arbitrum.foundation/t/constitutional-aip-constitutional-quorum-threshold-reduction/29145>
- Uniswap turnout decline / 40M on-chain quorum: <https://gov.uniswap.org/t/rfc-governance-logistics-improvements/25737>
- Aave Shutter trial + results: <https://governance.aave.com/t/arfc-private-voting-for-aave-governance-2-month-trial/12730>
- Circle Research concave=linear: <https://arxiv.org/html/2605.18990>
- Yearn Guardian (nullify, cannot propose): <https://docs.yearn.fi/developers/security/multisig> · YIP-81: <https://docs.yearn.fi/contributing/governance/yips/yip-81>
- Optimism Foundation cancel role: <https://gov.optimism.io/t/governor-upgrade-proposal-onchain-controls-mvp/10371>
- MACI (contracts; not this repo): <https://maci.pse.dev/blog/maci-1-0-technical-introduction>
- EOS vote decay: <https://github.com/EOSIO/eosio.contracts/blob/52fbd4ac7e6c38c558302c48d00469a4bed35f7c/contracts/eosio.system/include/eosio.system/eosio.system.hpp>
- Shutter permanent shielded (PoC, not CL8Y): <https://blog.shutter.network/permanent-shielded-voting-is-coming-to-snapshot/>
- BonkDAO 2026-07 empty-room quorum (secondary, cited): <https://www.coindesk.com/markets/2026/07/07/bonk-faces-usd20-million-treasury-drain-after-attacker-spends-usd4-million-to-pass-malicious-proposal>
- Build Finance 2022 takeover: <https://www.theblock.co/post/134180/build-finance-dao-suffers-hostile-governance-takeover-loses-470000>
- Optimism active-votable 6-month quorum: <https://github.com/ethereum-optimism/OPerating-manual/blob/main/manual.md>
- Hive HF25 vote expiration: <https://hive.blog/hive/@hiveio/hive-hardfork-25-is-on-the-way-hive-to-reach-equilibrium-on-june-30th-2021>
- Falk et al. % of total supply cast: <https://arxiv.org/html/2407.10945>
- Feichtinger et al. % of delegated VP: <https://ar5iv.labs.arxiv.org/html/2302.12125>
- Snapshot Shutter still shows who voted: <https://discuss.ens.domains/t/temp-check-shielded-voting-for-ens-snapshot-proposals/22142>
- Guo et al. shutter turnout (SSRN): <https://papers.ssrn.com/sol3/papers.cfm?abstract_id=4982940>
- B-Privacy tally leakage: <https://arxiv.org/abs/2509.17871>
- Snapshot anti-whale strategy: <https://github.com/snapshot-labs/snapshot-docs/blob/master/user-guides/spaces/space-handbook/anti-whale.md>

In-tree: [ARCHITECTURE.md](ARCHITECTURE.md), [OPERATOR_VOTING.md](OPERATOR_VOTING.md), [LEDGER_INVARIANTS.md](LEDGER_INVARIANTS.md), [OPS.md](OPS.md), [FRONTEND.md](FRONTEND.md), `ledger/migrations/20260825000002_voting.sql`.

**No legal advice.** Token-holder votes on treasury or issuance may raise fiduciary or securities questions — **needs counsel**; this note does not invent a regime.
