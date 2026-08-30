---
name: voting-draft-review
description: >-
  Draft collaboration, structured proposal template, committee open-vote,
  freeze-at-open snapshot, comments, and independent analysis (issues #10 / #11).
  Use when implementing or verifying GitLab voting issues #10 or #11, or when
  changing proposal status, body_hash, committee allowlist, or vote CTAs.
---

# Draft / review gate (voting issues #10 and #11)

Read [`docs/OPERATOR_VOTING.md`](../docs/OPERATOR_VOTING.md) (OV-S, template) and [`docs/FRONTEND.md`](../docs/FRONTEND.md) first.

This skill is for **3rd-party agents** continuing the pre-vote lifecycle. Do not invent a CosmWasm governor, an in-browser AI key, or identity merge between `terra1…` and `0x…`.

## What is in-tree

| Item | Where |
|------|--------|
| Template sections + canonical hash | [`operator-voting/src/sections.rs`](../operator-voting/src/sections.rs) · [`frontend/src/utils/proposalSections.ts`](../frontend/src/utils/proposalSections.ts) |
| Status `draft` → `open` | [`ledger/migrations/20260830000004_draft_lifecycle.sql`](../ledger/migrations/20260830000004_draft_lifecycle.sql) |
| API | [`operator-voting/src/api.rs`](../operator-voting/src/api.rs) |
| Committee env | `VOTING_COMMITTEE_ADDRESSES` in [`.env.example`](../.env.example) · [`deploy/coolify.env.example`](../deploy/coolify.env.example) |
| dApp | [`frontend/src/pages/VoteNewPage.tsx`](../frontend/src/pages/VoteNewPage.tsx) (create **draft**) · `VoteDetailPage.tsx` (comments, hide votes until open) |

## Invariants (do not violate)

1. **No freeze at draft create.** `proposal_snapshots` is filled only on `open`. Client-supplied freeze heights are ignored.
2. **Votes require `status=open`.** UI must hide vote CTAs on drafts.
3. **Purposes are domain-separated:** `draft`, `amend`, `comment`, `analyze`, `open_vote`, `vote`. A `propose` blob must not create or open.
4. **≥1000 CL8Y** starts a draft. Commenting needs registration only. Opening is committee-only (may hold less than 1000).
5. **Proposer-only amends** while draft. `409` on stale `prev_body_hash`. After open, amends are rejected.
6. **Independent analysis** is a separate signed committee record. The proposer cannot write it. Optional — open still works with none attached.
7. **AI is not implemented.** Do not add `VITE_*` model keys. Do not auto-`open`.
8. **Blacklist, Legal, ammonia, O-RL1, dual-chain, no identity merge** still apply to every new POST.
9. New tables stay in schema `voting`. Restricted role still cannot write ledger ingest. [`deploy/grants.sql`](../deploy/grants.sql) `GRANT ALL ON ALL TABLES IN SCHEMA voting` covers comments/analysis.

## Ops

Set `VOTING_COMMITTEE_ADDRESSES` on `operator-voting` (same address syntax as the blacklist). Invalid entries fail startup. Empty allowlist means **no poll can open**. Do not paste committee keys into the frontend.

Live QA after Legal accept: Terra **and** BSC — register → draft → second wallet comments → committee open → both chains vote. A wallet that registered **during draft** and holds CL8Y **can** vote; one that registers after open cannot.

## Do not

- Auto-transition `draft` → `open` from comments, analysis, or any model output.
- Treat display names as committee membership.
- Hash freeform `body_html` for new drafts; hash the canonical section JSON.
- Freeze twice or take heights from the client.
- Ship an unstructured comment thread on freeform HTML (template is required).
