---
name: proposal-template
description: >-
  Structured CL8Y voting proposal sections (issue #10 / OV-S). Use when
  changing POST /v1/proposals, body_hash, compose UI, list TL;DR, or
  proposal HTML sanitize.
---

# Proposal template (issue #10)

Read [`docs/OPERATOR_VOTING.md`](../docs/OPERATOR_VOTING.md) OV-S1–S8 before changing propose.

## Do not

- Accept freeform `body_html` on `POST /v1/proposals`
- Hash concatenated UI strings or unsigned HTML
- Add an “independent analysis” field the proposer can self-author
- Merge Terra and BSC identities
- Change the ammonia allowlist without updating the TS strip
- Treat list `summary` as trusted HTML (`innerHTML`)
- Implement draft/comments/committee here — that is [#11](https://gitlab.com/PlasticDigits/voting/-/issues/11). [#13](https://gitlab.com/PlasticDigits/voting/-/issues/13) is governance research.

## Hash

SHA-256 hex of compact JSON, **sorted keys**:

`context`, `problem`, `pros_cons`, `solution`, `success_criteria`, `summary`

Values are ammonia-cleaned HTML; empty-visible → `""`. Golden vector is in OPERATOR_VOTING.md. Keep Rust [`operator-voting/src/proposal_sections.rs`](../operator-voting/src/proposal_sections.rs) and TS [`frontend/src/utils/proposalSections.ts`](../frontend/src/utils/proposalSections.ts) in lockstep.

## UI

Labeled fields on `/new`. List shows TL;DR as text. Detail uses display order (summary first). Legacy rows (`body_sections` null) render sanitized `body_html`.

## Protocol version 2 (#41/#43)

For new signing, server `/v1/canonicalize` is authoritative; client display sanitation is independent. Do not attempt to duplicate ammonia byte serialization with DOMPurify. See [AGENTS_PROTOCOL_SECURITY.md](AGENTS_PROTOCOL_SECURITY.md) P5–P10 and the shared canonical fixtures. Metadata supplies the configured threshold; revision/nonce binding follows [AGENTS_DRAFT_REVIEW.md](AGENTS_DRAFT_REVIEW.md).
