# ADR 0001: Remove catch-all CODEOWNERS

## Status

Proposed. Original tracker
[#30](https://git.cl8y.com/code/voting/issues/30) is **merged** incomplete
S1 (`merged: true` at `2026-09-21T13:10:28Z`; merge `1d85943`; head
`eb352b8`; 1 file / 6 deletions; commit statuses on `eb352b8` and
`1d85943` are `[]`). Remaining land is a **new** remaining-work issue
`{R}` plus a **new** product PR from current `origin/main`. This pass
does not accept the design. Keywords in the issue body are not
architecture approval. Ordinary design is not a founder card.

`#30` and `pulls/30` are the same closed/merged number. Implement
cannot occupy `#30`, draft it, keep `eb352b8` unmerged, or push onto
closed `pulls/30`. `Closes #30` on a successor is a no-op.

Overview (product tree, merge gate, pipeline path/layout):
[`ARCHITECTURE.md`](../ARCHITECTURE.md). Do not copy that table here. Local
invariant IDs are **H30-*** (this ticket). `code/hello` architecture **H15**
and sister-repo **H3** / **H5** / **MG** tables are other trees’ copies of
the same flags, not this repo’s issue number.

Design branch `cac-design-issue-30` is transport only; it is not the product
PR. Do not open a design-only PR. Do not merge that transport ref to
`main`. Copy **the independently accepted SHA** (this SHA or a successor
after independent review) **files** onto a branch forked from current
`origin/main`. Never use `cac-design-issue-30` / this SHA as the product
tip: `git diff origin/main <this-sha> -- CODEOWNERS` is a **restore** of
`.* @code/maintainers`.

Land criterion: product tip `docs/adr/0001-remove-catchall-codeowners.md`
and `docs/ARCHITECTURE.md` are **byte-identical** to that SHA. Allowed
extras versus that SHA **plus current `origin/main`** are README Merge-gate
+ docs index + the quoted root `.woodpecker.yaml` only. S3 plant-check
`{n}` is the named throwaway `docs/h30-plant-check.md`, **closed without
merging** (`GET .../pulls/{n}` `merged == false`). Architecture **H30-1**
is four-path **absence** (`test ! -e`), not “file exists but is not
requesting reviewers.” **H30-1** already holds on current `origin/main`;
the successor must not re-plant.

This ADR does not authorize deploy, spend, custody rotation, Coolify
`vote.cl8y.com` publish, Legal admin writes, or Forgejo protection PATCH
([agent-control #297](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/297)
/ [ADR 0004](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/src/branch/main/docs/adr/0004-autonomy-policy.md)).
This repo has no `autonomy.rs` / HMAC surface; do not add one to
self-approve.

`operator-voting/src/crypto/adr36.rs` is Terra ADR-36 verify, not this
`docs/adr/` series. This is the first record under `docs/adr/`.

## Outcome

Delete the catch-all `CODEOWNERS` so Forgejo does not plant **official**
review requests on every change. Merge to `main` stays: pull request,
Woodpecker context `ci/woodpecker/pr/woodpecker`, SHA-pinned `Do: merge`, no
direct push, no `force_merge`.

**S1 is historical and incomplete.** PR
[#30](https://git.cl8y.com/code/voting/pulls/30) already merged a
CODEOWNERS-only delete (`eb352b8` / `1d85943`). That merge is **not**
land. Current `origin/main` has four-path `CODEOWNERS` absence, **no**
root `.woodpecker.yaml`, **no** H30 on `docs/ARCHITECTURE.md`, **no**
README Merge-gate. Open trackers at review time were only
[#29](https://git.cl8y.com/code/voting/pulls/29) and
[#13](https://git.cl8y.com/code/voting/issues/13); there is no leftover
plant-check issue and no remaining-work issue yet.

**Remaining land** is S2+**WP-pre** on a **new** PR forked from current
`origin/main`, tracked by a **new** remaining-work issue `{R}` (Decision
4). Do not push onto closed `pulls/30`. Do not require `Closes #30`.
Merge of `#30` with empty statuses does **not** waive **WP-pre** or
**H30-3**. Slice S3 must **not** fire just because `#30` merged.

**Leftover-complete** (S3: dedicated post-merge plant-check PR `{n}` whose
**only** changed path is the throwaway `docs/h30-plant-check.md`, **non-WIP**,
**closed without merging**) is tracked on leftover issue `{L}` opened
before `Do: merge` of the **successor**, not of `#30`. Point leftover-complete
at `{R}`/`{L}`, not at closed `#30`. S3 is not a close gate for `{R}`.
Closing the leftover **issue** is not the same contract as closing `{n}`
unmerged. Operator attestation may **record** the plant-check GETs of that
dedicated PR; it is not a substitute for opening `{n}`, and it does not
authorize merging `{n}`.

This is a product-tree copy of the pattern named by
[cl8y-forgejo#48](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/issues/48).
The canary `code/hello`
[#15](https://git.cl8y.com/code/hello/issues/15) also merged delete-only
at `2026-09-21T13:08:39Z`; `CODEOWNERS` is absent on hello `main`. Do not
treat hello#15 as still open, and do not wait on it. The **landed product
pattern** (CODEOWNERS-only delete with pre-existing green WP) is
[code/cl8y-dex-terraclassic#1309](https://git.cl8y.com/code/cl8y-dex-terraclassic/issues/1309).
This repo has **no** in-tree Woodpecker pipeline and empty commit statuses
on the `#30` merge, so remaining land must still produce the posting
context here (**WP-pre**). Protection on `code/voting` `main` already
matches the intended flags (GET `updated_at` 2026-09-21T07:29:30Z;
**H30-8** / **H30-9** / **H30-2** / **H30-3** / **H30-5**; unauthenticated
GET is 401). Close of the successor still re-GETs. Remaining-work `{R}`
does not re-roll protection, does not implement CAC autoland, and does
not deploy ledger / API / dApp, Legal, or Coolify.

## Context

`1006f8b` (2026-09-02) added root `CODEOWNERS`:

```
.* @code/maintainers
```

Forgejo uses Go regular expressions, not GitHub globs, and searches **root**,
`docs/CODEOWNERS`, `.gitea/CODEOWNERS`, and `.forgejo/CODEOWNERS`. Combined
with historical `block_on_official_review_requests`, every non-WIP PR that
touches a matching path requests team **maintainers** (CODEOWNERS line
`@code/maintainers`; PR JSON `requested_reviewers_teams[].name` is
`maintainers`, org `code`, team id 4). That team’s usual member is the PR
submitter, so self-approve is 422 and merge is 405. CAC `RECOMMEND: ACCEPT` is
not a Forgejo `APPROVED` review.
[cl8y-agent-control#388](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/388)
skipped the deadlock; it did not remove this file or this repo’s protection.
Drain must not delete `CODEOWNERS` as a workaround; the designed product-tree
path is the delete (already on `main`) plus standing docs plus WP bootstrap.

This tree has no root `.woodpecker.yaml` on current `origin/main`. Do not
add `.woodpecker.yml` or `.woodpecker/` (directory mode can ignore root YAML
and post `ci/woodpecker/pr/<other>`). There is **no** voting sister ticket
that will add one before remaining land (open numbers at review time: #29
Renovate and #13 governance research). Hello#15 and dex#1309 could merge a
CODEOWNERS-only delete because those trees already posted
`ci/woodpecker/pr/woodpecker` (hello’s delete-only merge is still not this
repo’s WP-pre). This tree cannot: land criterion 3 / **H30-3** requires
that context, and this slice must not `force_merge` or drop **H30-3**.
Therefore the successor PR **may and must** add the quoted **#30 land
bootstrap** (architecture **Pipeline**). Missing WP YAML **or** missing
Woodpecker project enablement is a land blocker for remaining-work `{R}`,
not leftover. Merge of `#30` with empty statuses does not waive that.

[`.gitlab-ci.yml`](../../.gitlab-ci.yml) already runs gitleaks / cargo /
frontend unit / SPA fallback on GitLab. Forgejo `has_actions=false`; those
jobs do **not** post the required context. They stay leftover hosting CI.
Do not delete them in `{R}`. Do not port `test:rust` / `test:frontend` /
`lint:gitleaks` into Woodpecker on the successor.

[cl8y-forgejo ADR 0003](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/src/branch/main/docs/adr/0003-community-forge-and-woodpecker.md)
item 8 (“CODEOWNERS plus protected `main`”) is amended on the forge repo for
the **official-review** half only. This tree’s README is product deploy docs
and does not currently claim CODEOWNERS is the trusted-PR gate; S2 still
adds an explicit **H30** pointer so later agents do not re-add
`.* @code/maintainers`. The original issue body links cl8y-forgejo
[`docs/INVARIANTS.md`](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/src/branch/main/docs/INVARIANTS.md);
that file is not in this tree. Do not invent a local copy.

Live proof (this pass):

| PR | Role | Plant / land |
| --- | --- | --- |
| [#30](https://git.cl8y.com/code/voting/pulls/30) | Historical incomplete S1. `merged: true` at `2026-09-21T13:10:28Z`; merge `1d85943`; head `eb352b8`; **1 file** / 6 deletions; statuses `[]`. | While open: `requested_reviewers` = `[]`; `requested_reviewers_teams` = `maintainers` (id 4, org `code`); `official: true` `REQUEST_REVIEW` id **217** at 2026-09-21T07:43:10Z. **Disqualified** as “no new plant” / leftover-complete / remaining land. Do not occupy, draft, or push onto this closed PR. |
| [#29](https://git.cl8y.com/code/voting/pulls/29) | Renovate onboarding (open, 1 file) | `requested_reviewers` = `[]`; `requested_reviewers_teams` = `maintainers`; `official: true` `REQUEST_REVIEW` id **163** at 2026-09-20T23:56:23Z. Still open. **Disqualified** as leftover-complete. |

Those leftovers are **disqualified** as “no new plant” evidence. Protection
GET on `main` has official-review **block** off (**H30-8**), so leftover
requests must not be treated as merge blockers.

`#30` as merged cannot finish land: S1-only, empty Woodpecker statuses, no
standing docs / README pointer on `main`. Convert-to-draft of `#30` is
unavailable (already merged). Remaining work is a new PR.

Drain comments on #30 (`no occupying job…`; queued `design_author` without a
Hetzner VM) are
[cl8y-agent-control#429](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/429)
/ CAC, not a remaining-work failure. They are not permission to treat the
merge as land.

This tree has **no** vendor `CODEOWNERS`. Scope absence checks to the four
Forgejo search paths. Current `origin/main` already satisfies four-path
`test ! -e`.

## Non-goals

- Forgejo protection JSON / `apply_repo_policy.py` / migrate
  `_ensure_codeowners` / templates
  ([cl8y-forgejo#48](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/issues/48)).
- CAC autoland predicates, occupying jobs, or `DrainSkip::OfficialReview`
  cleanup ([cl8y-agent-control#429](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/429),
  leftover of #388).
- Dismissing reviewers from the controller (forbidden substitute in #388),
  including the leftover on merged #30 / open #29.
- Path-specific CODEOWNERS, a second maintainer, or `required_approvals: 1`.
- Weakening **H30-3**. The quoted root `.woodpecker.yaml` (**#30 land
  bootstrap**, architecture **Pipeline**) **is allowed** on the successor
  PR so **H30-3** can post; see **WP-pre**. MUST path: root `.woodpecker.yaml`.
  Do not add `.woodpecker.yml` or `.woodpecker/`. Do not `force_merge`. Do
  not post a fake commit status. Do not add Coolify, Telegram, BSC RPC,
  Terra LCD, Legal admin, or bind deploy secrets to `pull_request`. Do not
  copy hello’s `coolify-deploy` / `telegram-failure`. Do not copy dex’s
  `scripts/ci/gitleaks-scan-tracked.sh`. Do not port `.gitlab-ci.yml`
  `test:rust` / `test:frontend` / `lint:gitleaks` onto the successor.
- Rewriting `ledger/`, `operator-voting/`, `frontend/`, `deploy/`,
  `skills/`, `docs/LEDGER_INVARIANTS.md`, `docs/OPERATOR_VOTING.md`,
  `docs/FRONTEND.md`, `docs/OPS.md`, `docs/HANDOFF.md`,
  `docs/GOVERNANCE_RESEARCH.md`, or product `L*` / `OV-*` / `O8` /
  `L-EVM*` narrative. Do not enable direct `main`. Cargo and npm tests
  stay ungated by this ticket.
- Coolify rebuild, Legal property admin, wallet stacks, snapshot freeze,
  or live `vote.cl8y.com` QA
  ([agent-control #297](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/297);
  product [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7)).
- Renovate onboarding ([#29](https://git.cl8y.com/code/voting/pulls/29)).
- A local `docs/INVARIANTS.md` (issue body cites the forge copy).
- Editing `autonomy.rs` / HMAC, self-approval, or a founder card for this
  ordinary design.
- Opening `cac-design-issue-30` as a PR, merging that transport ref to
  `main`, or using it / `bc15b2d` as the product tip (that re-plants
  `CODEOWNERS`).
- Occupying, drafting, or pushing onto closed `pulls/30`. Requiring
  `Closes #30`. Treating merge of `#30` as **WP-pre**, leftover-complete,
  or S3 start.
- Re-deleting `CODEOWNERS` versus current `origin/main` (already absent;
  a delete hunk would mean the tip restored then deleted, or forked the
  wrong base).

## Decision

1. **Delete** root `CODEOWNERS`. Do not leave an empty or comments-only file
   (Forgejo still parses it). **Already done** on `origin/main` by
   historical S1. The successor must keep four-path absence (**H30-1**).
2. **Do not add** `docs/CODEOWNERS`, `.gitea/CODEOWNERS`, or
   `.forgejo/CODEOWNERS`. After land, `test ! -e` is true on all four
   paths (no file, including empty or comments-only). None of those
   paths may contain a reviewer rule for any pattern (not only `.*`).
3. **Keep** the merge gate in [`ARCHITECTURE.md`](../ARCHITECTURE.md)
   **H30**. On the successor product PR, **add** exactly this paragraph to
   root `README.md` after the **Local git hooks** section. Keep existing
   product tables, token registry, and Docs-for-agents index; add ADR 0001
   + architecture **H30** to that table. Do not claim maintainers review
   every change. Architecture-only is **not** sufficient.

```markdown
## Merge gate

Merge to `main` is a pull request, Woodpecker context
`ci/woodpecker/pr/woodpecker`, and SHA-pinned `Do: merge`
([architecture **H30**](docs/ARCHITECTURE.md)). Official CODEOWNERS review
is not a merge gate. Do not re-add catch-all `CODEOWNERS`
(`.* @code/maintainers`); see
[ADR 0001](docs/adr/0001-remove-catchall-codeowners.md).
```

4. **One remaining-land product PR**, **new** from current `origin/main`.
   Track it with a **new** remaining-work issue `{R}` (paste below). Diff
   versus current `origin/main` is **only**: the two standing `docs/`
   files **byte-identical** to the independently accepted SHA + README
   pointer (Decision 3) + the quoted **WP-pre** `.woodpecker.yaml`. There
   is **no** `CODEOWNERS` hunk (already absent). Copy
   `docs/adr/0001-remove-catchall-codeowners.md` and
   `docs/ARCHITECTURE.md` from that SHA; do not rewrite **H30** while
   combining. `cac-design-issue-30` is transport; do not merge it to `main`,
   do not open it as the product PR, and do not reset the product branch
   to it (that re-plants `CODEOWNERS`). Default vehicle is **not**
   `pulls/30`. Do not push onto closed `pulls/30`. Do not require
   `Closes #30`. The successor body **must** contain `Closes #{R}`, and
   leftover issue `{L}` **must** already exist before `Do: merge`.

   File `{R}` before opening the successor PR:

```text
fj issue create --body-file remaining-work.md --no-template \
  "chore: standing docs and Woodpecker after incomplete CODEOWNERS delete"
```

`remaining-work.md` body:

```markdown
Remaining S2 + WP-pre after historical incomplete S1
(PR #30 merged delete-only at 2026-09-21T13:10:28Z, head eb352b8,
merge 1d85943, empty commit statuses).

Do not occupy closed pulls/30. Do not Closes #30 (no-op).
Fork from current origin/main. Copy ADR 0001 and architecture H30
byte-identical from the independently accepted design SHA.
Allowed extra diffs versus that SHA plus origin/main: README
Merge-gate + docs index + quoted root .woodpecker.yaml only.
Do not use cac-design-issue-30 as the product tip (that re-plants
CODEOWNERS).

This issue is land (S2+WP-pre). Leftover-complete (S3 plant-check)
lives on a separate leftover tracker opened before Do: merge of
the successor. Do not Fixes / Closes / Resolves that leftover
number from this issue or from the successor PR.
```

5. **Leave** already-planted official requests on open PRs (including #29)
   and on merged #30. They are non-blocking under **H30-8** / **H30-9**. Do
   not dismiss them from CAC. Human dismiss is optional leftover, not AC.
   They do not count as “no new plant.”
6. **Do not** PATCH branch protection from this repository.
7. **Split remaining land from leftover-complete.** Remaining-work `{R}`
   (successor PR) is S2+**WP-pre** only. S1 is already on `main`. S3 lives
   on leftover issue `{L}` opened before `Do: merge` of the **successor**
   (plus the leftover checklist below), not before merge of `#30`. Require
   a **dedicated** post-merge plant-check PR `{n}` that is **non-WIP**,
   opened **after** S2+WP-pre are on `main` (successor merged), and whose
   diff versus `main` is **exactly** one added throwaway docs note:
   `docs/h30-plant-check.md` (one line, e.g. `H30 plant-check; do not merge.`).
   `{n}` **must not** change `ledger/`, `operator-voting/`, `frontend/`,
   `deploy/`, or any of the four CODEOWNERS paths. **Close `{n}` without
   merging it.** Merging `{n}` onto `main` is a failure mode (hello ADR:
   “Merging the plant-check PR”). Closing the leftover **issue** after
   the record is pasted is a different contract and does not authorize
   merging `{n}`. Do not accept `#30`’s leftover plant, `#29`’s plant,
   merge of `#30` itself, or “the next natural PR.” A no-op dedicated PR
   is not evidence: `.*` matches every path, and Forgejo does not plant on
   a no-op even if `CODEOWNERS` is still on `main` (dex#1309 no-op
   warning). A draft/WIP follow-up is not evidence (Forgejo skips
   CODEOWNERS on WIP). A plant-check that edits product code to “prove” a
   non-empty diff is not evidence. Operator attestation may **record** the
   GETs of that dedicated PR; it is not a substitute for opening it and
   does not authorize merging `{n}`. S3 must not fire just because `#30`
   merged.
8. **WP-pre (land prerequisite, owned).** The successor-PR head SHA must
   show a real Woodpecker **success** status `ci/woodpecker/pr/woodpecker`
   (creator/target `ci.cl8y.com`, not a manual Forgejo POST) before
   SHA-pinned `Do: merge`. Pending, failure, or a hand-posted status is not
   **WP-pre**. Merge of `#30` with empty statuses does **not** waive
   **WP-pre** or **H30-3**.

**File owner:** implement of remaining-work `{R}` — no sister voting
ticket exists to add YAML first, so put **exactly** this file at root
`.woodpecker.yaml` on the successor PR (same bytes as architecture
**Pipeline**):

```yaml
# #30 land bootstrap. Posts context `ci/woodpecker/pr/woodpecker`.
# Not this repo's product test gate. Cargo, npm, and GitLab jobs stay ungated.
# Any later real CI must extend this same root file.
# MUST path: repository-root `.woodpecker.yaml`.
# Do not add `.woodpecker.yml` or `.woodpecker/`.

when:
  - event: [push, manual]
    branch: main
  - event: pull_request

steps:
  - name: tree
    image: alpine:3.22@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce
    commands:
      - test -n "$(find . -type f ! -path './.git/*' | head -1)"
```

**Enablement owner (pre-merge gate):** confirm `code/voting` is an
active Woodpecker project **before** pushing the successor (Forgejo repo
id **43** on `ci.cl8y.com`; webhook to `https://ci.cl8y.com/api/hook` for
`push` + `pull_request`). Pass/fail GET (optional extra proof; still a
land blocker if it fails): Forgejo `GET /api/v1/repos/code/voting/hooks`
shows an **active** hook whose URL is `https://ci.cl8y.com/api/hook` and
whose events include `push` **and** `pull_request`, **or** Woodpecker
repo GET for `code/voting` has `active == true`. Fail closed on missing
hook, wrong URL, missing events, or `active == false`. **WP-pre**
(success status from `ci.cl8y.com`) remains the land proof; this GET
does not replace it. File owner ≠ posting owner (forge ops). Empty
statuses with no YAML are ambiguous: missing file **or** repo not
registered. dex#1247’s 44/44 `code/*` enablement on 2026-09-12 is **not**
this preflight. Empty statuses on merged `#30` are **not** this
preflight and do not waive it. A leftover to extend this same root file
with product CI is optional and is **not** a close gate for `{R}`. Do
not expand this ticket into cargo / frontend / gitleaks on Woodpecker.

If the YAML is on the successor and nothing posts: **land blocked**. Do
not treat a missing context as leftover, “out of this slice,” or silent
sister-ops. Block on a **named** forge unblock
([cl8y-forgejo#48](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/issues/48)
or a local leftover shaped like
[dex#1247](https://git.cl8y.com/code/cl8y-dex-terraclassic/issues/1247))
**plus retrigger**. Do not drop **H30-3**. Do not `force_merge`. Do not POST
a fake status.

9. **Leftover tracker `{L}`** — `fj issue create`, not a PR. Open it
   **before** `Do: merge` of the **successor**, not of `#30`.
   Leftover-complete closes that **issue** (issue close after the record
   is pasted), not via merge. Point leftover-complete at `{R}`/`{L}`, not
   at closed `#30`. **`{R}` and the successor must not contain close
   keywords for `{L}`.** Remaining-work issue `{R}` and successor PR
   bodies, titles, and later comments must not use `Fixes #{L}`,
   `Closes #{L}`, `Resolves #{L}`, or equivalent Forgejo close keywords
   pointing at the leftover tracker (merging the successor would swallow
   the evidence issue). `#30` is already closed; do not retarget leftover
   complete onto it.

Paste:

```text
fj issue create --body-file leftover-tracker.md --no-template \
  "chore: leftover plant-check after catch-all CODEOWNERS remaining land"
```

`leftover-tracker.md` body (fields to fill later; `{n}` is the
dedicated plant-check PR):

```markdown
Post-merge evidence for removing catch-all CODEOWNERS (remaining
land after incomplete #30).

Merging the successor closes remaining-work `{R}`; this tracker
survives. Closed #30 is not this tracker.

Do not merge a PR whose body uses Fixes / Closes / Resolves for this
issue number. Leftover-complete is an issue close after the record
below is pasted, not a merge.

S3 must not fire just because #30 merged. Open the plant-check only
after the successor (S2+WP-pre) is on main.

**Close plant-check PR `{n}` without merging it.** Closing this leftover
issue is a different contract and does not authorize merging `{n}`.

`{n}` diff versus `main` is **exactly** the added throwaway
`docs/h30-plant-check.md`. Forbidden paths: `ledger/`,
`operator-voting/`, `frontend/`, `deploy/`, and any CODEOWNERS path.

## Record

- plant-check PR `{n}` (exactly `docs/h30-plant-check.md`; **closed without merge**):
- `{n}` GET `merged` (must be `false`):
- `{n}` GET `state` (must be `closed`):
- `GET /api/v1/repos/code/voting/pulls/{n}` body (the
  JSON used for the pass decision):
- `GET /api/v1/repos/code/voting/pulls/{n}/reviews` body
  (the JSON used for the pass decision):
- four-path check on `main`
  (`test ! -e CODEOWNERS && test ! -e docs/CODEOWNERS && test ! -e .gitea/CODEOWNERS && test ! -e .forgejo/CODEOWNERS`):
```

## Component / state / interface changes

| Surface | Change |
| --- | --- |
| `CODEOWNERS` (root) | Already removed on `origin/main` by historical S1. Successor keeps it absent. |
| `docs/CODEOWNERS`, `.gitea/CODEOWNERS`, `.forgejo/CODEOWNERS` | Must remain absent (no empty file). |
| Forgejo PR review interface | After remaining land, a **dedicated** non-WIP plant-check PR `{n}` against `main` whose **only** changed path is `docs/h30-plant-check.md` must not get an official CODEOWNERS team request. **Close `{n}` without merging.** `requested_reviewers_teams` from this file becomes empty for those new PRs. |
| Branch protection API | No write from this ticket. Operator GET must still match architecture **GET-required** (**H30-2**, **H30-3**, **H30-5**, **H30-8**, **H30-9**). Observed rows are do-not-touch. Prior `updated_at` is attested; successor close re-GETs. Unauthenticated GET is 401. |
| Root `.woodpecker.yaml` | **Required on the successor PR.** **#30 land bootstrap** quoted in Decision 8 / architecture **Pipeline**. MUST that path. `steps:` + two-item `when:` + digest-pinned alpine + `find` one-liner. No secrets, Coolify, Telegram, BSC, Terra. Do not add `.yml` or `.woodpecker/`. Any later real CI extends this same file. |
| `.gitlab-ci.yml` | Unchanged leftover hosting CI. Not a Forgejo required context. |
| `.gitignore` | Unchanged. `docs/` is already trackable (`ARCHITECTURE.md` on `main`). Do not add a directory ignore of `docs/`. |
| `docs/ARCHITECTURE.md`, `docs/adr/0001-remove-catchall-codeowners.md` | Added/updated on the design branch. Product tip **copies** them **byte-identical** from the independently accepted SHA onto a fork of current `origin/main`. |
| Root `README.md` | **Edit** on the successor PR: add the Decision 3 **Merge gate** paragraph and index ADR 0001 / **H30** in Docs for agents. Keep product tables, tokens, and verify commands. Do not stub. |
| `ledger/`, `operator-voting/`, `frontend/`, `deploy/`, `skills/`, `.gitleaks.toml`, `.githooks/`, product invariant docs, `code/maintainers` team | Unchanged. The team may keep existing; it simply is not planted as official review. Cargo / npm tests stay ungated. |
| Remaining-work issue `{R}` | `fj issue create` before the successor PR; owns S2+WP-pre; successor `Closes #{R}`. Must not close leftover `{L}`. |
| Leftover tracker issue `{L}` | `fj issue create` before merge of the **successor**; owns plant-check `{n}` (named path, close without merge); closed as an issue, not via merge. Closing this issue is not permission to merge `{n}`. Not closed `#30`. |

No runtime contract, schema, LCD, RPC, Legal, wallet, or HTTP API change.

## Affected invariants

| ID | Kind | Rule |
| --- | --- | --- |
| **H30-1** | Non-GET | `test ! -e` is true on `CODEOWNERS`, `docs/CODEOWNERS`, `.gitea/CODEOWNERS`, and `.forgejo/CODEOWNERS`. Absence, including empty or comments-only files, not “file exists but is not requesting reviewers.” File `@code/maintainers` / JSON team `maintainers` are the same leftover plant. Already true on current `origin/main`; successor must not restore. |
| **H30-2** | GET-required | `enable_push=false` on `main`. |
| **H30-3** | GET-required | `enable_status_check=true` and required context `ci/woodpecker/pr/woodpecker`. Merge of `#30` with empty statuses does not waive this. |
| **H30-4** | Non-GET | Merge is SHA-pinned `Do: merge`. Never document or use `force_merge`. Not a protection field; omit from GET matching. |
| **H30-5** | GET-required | `block_on_rejected_reviews` stays true. An explicit REJECT still blocks. |
| **H30-6** | Non-GET | No Coolify/Woodpecker secrets on `pull_request`. This tree does not grow a deploy step from remaining-work `{R}`. Hello `telegram-failure` is forbidden. Coolify / Legal stays #7 / #297. |
| **H30-7** | Non-GET | This tree does not expand CAC merge/deploy/spend/custody policy. |
| **H30-8** | GET-required | `block_on_official_review_requests=false`. Leftover official requests on merged #30 / open #29 are non-blocking. |
| **H30-9** | GET-required | `required_approvals=0`. |

GET-observed flags (`block_on_outdated_branch`, `dismiss_stale_approvals`,
`apply_to_admins`) are listed in architecture only. They have no
close-criterion IDs. Remaining-work `{R}` must not PATCH them.

Product `L*` / `OV-*` / `O8` / `L-EVM*` in ledger / operator-voting /
frontend docs are unchanged. This ADR does not weaken them.

CAC invariants 29 / 67 / #388 stay: skip official-review deadlock; never
`force_merge`; drain does not delete CODEOWNERS. Product land makes the skip
class stop firing **for this repo** once new PRs have no plant.

## Alternatives

| Option | Why not |
| --- | --- |
| Keep file, rely on `block_on_official_review_requests=false` | Requests still plant on every non-WIP PR that changes a file (see #29 and historical #30); drain noise; Renovate/agent PRs look like they need a human stamp; templates can re-teach the old gate. File is already gone on `main`; do not restore it. |
| Replace `.*` with path owners | No second reviewer exists; same 405/422 if official-review is ever turned on; out of scope. |
| Add a second maintainer | Founder ops, not this implement. |
| Dismiss official requests from CAC | Forbidden by #388 as a substitute for policy reversal. |
| Direct-push remaining docs / YAML to `main` | Violates **H30-2**. Remaining land goes through a **new** PR. |
| Empty or comments-only CODEOWNERS | Forgejo still parses it. Absence is the contract. |
| Wait for `code/hello` canary merge before this delete | Sister pattern, not a local iid. hello#15 already merged delete-only; `CODEOWNERS` is absent on hello `main`. Do not wait on it. |
| Cite hello#15 as a landed complete product delete | The landed complete pattern is dex#1309 (pre-existing green WP). hello#15 merged delete-only like `#30`. |
| Leave ADR only on `cac-design-issue-30` | Standing docs never reach `main`; later agents re-add `.* @code/maintainers`. |
| Rewrite ADR/architecture while combining onto the successor | Implement can change **H30** after independent review. Product tip two `docs/` files must be **byte-identical** to the accepted SHA. |
| Treat merged `#30` / `eb352b8` / `1d85943` as land | **H30-1** true, README/ADR/architecture / posting context false; empty statuses fail **H30-3**. Historical incomplete S1. |
| Push remaining work onto closed `pulls/30` | Combine must not operate on a merged PR. New PR from current `origin/main`. |
| Require `Closes #30` on the successor | No-op on a closed issue. File `{R}` and `Closes #{R}`. |
| Close `{R}` only after a later PR proves no new plant | Merge of the successor closes `{R}`; waiting for the follow-up before merge never produces it. S3 lives on `{L}`. |
| Treat `#30` leftover plant, `#29`’s plant, merge of `#30`, an empty-diff follow-up, or a merged `{n}` as S3 | S3 starts after the **successor** is on `main`, not after `#30`. A dedicated **non-WIP** post-merge PR whose **only** path is `docs/h30-plant-check.md`, **closed without merge**, is the evidence. |
| Fire S3 because `#30` merged | Remaining land (S2+WP-pre) is not on `main` yet. Forbidden. |
| Plant-check `{n}` edits `ledger/`, `operator-voting/`, `frontend/`, `deploy/`, or a CODEOWNERS path | Proves a non-empty diff by touching the product tree. Forbidden. Named path is `docs/h30-plant-check.md` only. |
| Merge the plant-check PR `{n}` onto `main` | Lands a throwaway (or a product edit). Hello failure mode “Merging the plant-check PR.” Close `{n}` without merge. Closing the leftover issue is a different contract. |
| Operator attestation instead of the dedicated plant-check PR | False-pass path. Attestation may record GETs of that PR; it is not a substitute. |
| Drop the Woodpecker required context so an empty-CI repo can merge | Violates **H30-3**. Do not `force_merge`. Produce the context via **WP-pre**. Do not treat GitLab as a substitute. Do not treat empty statuses on merged `#30` as a waiver. |
| Wait for a sister YAML-only ticket before remaining land | No such voting issue exists. Blocking land on an unfiled ticket re-boxes implement. Allow the quoted bootstrap on the successor instead. |
| Copy hello’s `.woodpecker.yaml` | Includes `coolify-deploy` and `telegram-failure` `from_secret`. Telegram runs on `status: [failure]` with no event filter (**H30-6**). |
| Copy dex’s `.woodpecker.yaml` | Calls `scripts/ci/gitleaks-scan-tracked.sh`, which does not exist here. |
| Port `.gitlab-ci.yml` rust / frontend / gitleaks into Woodpecker | Expands this chore into product CI. Cargo / npm stay ungated. Later CI extends the same root file. |
| Alpine `tree` binary / `apk add tree` | Stock Alpine has no `tree`. Org stubs use the `find` one-liner. |
| Add `.woodpecker.yml` or `.woodpecker/<other>.yaml` | Directory mode can ignore root YAML and post `ci/woodpecker/pr/<other>`; **H30-3** fails. |
| Treat missing context as silent forge ops / leftover | File owner ≠ posting owner, but land still blocks. Named unblock (cl8y-forgejo#48 or a local dex#1247-shaped leftover) plus retrigger. |
| Fake status POST or `force_merge` | Forbidden. |
| Treat a failing, pending, or hand-posted `ci/woodpecker/pr/woodpecker` as **WP-pre** | Land requires **present and success** from Woodpecker (creator/target `ci.cl8y.com`). |
| Merge `cac-design-issue-30` / use it or `bc15b2d` as the product tip | That ref still has root `CODEOWNERS`; `git diff origin/main <sha> -- CODEOWNERS` is a restore. Transport only. Fork from current `main` and **copy** the two docs files. |
| Push a design SHA as the successor tip | Restores `CODEOWNERS`; reject. |
| Re-require a `CODEOWNERS` delete versus current `origin/main` | File is already absent. A delete hunk means the wrong base. |
| `Fixes` / `Closes` / `Resolves` for leftover `{L}` on `{R}` or the successor | Merge closes the evidence issue; Decision 9 exists to prevent that. |
| Pass-iff matches `@code/maintainers` or only `requested_reviewers` | Misses live team `maintainers` / id 4; historical #30 has empty user list. Use the fail-closed pair. |

## Complexity added / removed

**Removed (S1, already on `main`):** catch-all official-review robot on
every diff from this file; operator dismiss step for new plants.

**Still missing until remaining land:** standing doc (this ADR +
architecture **H30**) plus a README pointer so later agents do not re-add
`.* @code/maintainers` as a merge requirement; leftover checklist `{L}`
that survives merge of `{R}`; a **#30 land bootstrap** Woodpecker file on
the successor so **H30-3** can post (this repo had none; `#30` merge did
not add it). No Coolify, no new services, jobs, flags, or runtime
surfaces. No `.gitignore` exceptions (`docs/` is already trackable).
Cargo / npm stay ungated. GitLab workflows stay but are not the Forgejo
merge gate.

## Migration

1. Protection is already migrated (forge #48 execute on this repo,
   GET `updated_at` 2026-09-21T07:29:30Z). Unauthenticated GET is 401.
   Re-GET at successor merge; do not PATCH.
2. Historical S1 already merged (`#30` / `eb352b8` / `1d85943`).
   Leftover-before-merge-of-`#30` is missed; do not try to satisfy it
   retroactively. File remaining-work `{R}`, then leftover `{L}` **before**
   `Do: merge` of the successor. Successor body `Closes #{R}`, not
   `Closes #30`.
3. Fork from current `origin/main`. Copy the two standing `docs/` files
   **byte-identical** from the independently accepted SHA. Add README
   Merge-gate + docs index and the quoted root `.woodpecker.yaml`. Design
   branch `cac-design-issue-30` is **not** that PR and must not be the
   product tip. Do not push `chore/remove-catchall-codeowners` or closed
   `pulls/30`.
4. Open PRs created while the file existed (#29; merged #30) may still
   show an official team `maintainers` request. Non-blocking under
   **H30-8**. No bulk dismiss required. Disqualified as leftover-complete
   evidence.
5. Do not restore the file from `docs/templates/CODEOWNERS` in cl8y-forgejo;
   that template is owned by #48.

## Observability

Relative reads. Do not log tokens, hosts, RPC keys, LCD URLs, Legal admin
tokens, or protection-script inventories.

**Protection (close criterion 4 / test 3).**
`GET /api/v1/repos/code/voting/branch_protections` — pass iff the
`main` rule equals architecture **GET-required** for `enable_push`,
`enable_status_check`, `status_check_contexts`, `required_approvals`,
`block_on_official_review_requests`, and `block_on_rejected_reviews`. Fail
if any of those five IDs (**H30-2** / **H30-3** / **H30-5** / **H30-8** /
**H30-9**) differ. **H30-3** covers two JSON keys (`enable_status_check`
and `status_check_contexts`). Do **not** treat observed flags or
**H30-4** as GET-match fields. The 2026-09-21T07:29:30Z GET is
attested; unauthenticated GET is 401. Successor merge still re-reads for
drift. A green `find` clone-check or GitLab `test:rust` job does not satisfy
this read. Empty statuses on `eb352b8` / `1d85943` do not satisfy **H30-3**.

**Plant-check (dedicated post-merge PR, leftover-complete).**

Fail-closed pair (jq on the two GETs). Pass iff **all** of:

- `(requested_reviewers // []) | length == 0`
- `(requested_reviewers_teams // []) | length == 0`
- no review with `official == true && state == "REQUEST_REVIEW"`
  (user **or** team)
- PR GET `draft == false`; title does not contain `WIP` (case-insensitive);
  `git diff origin/main...HEAD` is **exactly** the added file
  `docs/h30-plant-check.md` (no `ledger/`, `operator-voting/`,
  `frontend/`, `deploy/`, or CODEOWNERS path)

`maintainers` / `id: 4` is the known-plant example (historical #30), not
the only fail. Do not require `team.organization` on the reviews GET.
Do not match `team.name == "code/maintainers"` or the CODEOWNERS token
`@code/maintainers` (those miss the live plant).

`official` alone means assigned/write-access, not “planted by
CODEOWNERS”; the conjunction with `state == "REQUEST_REVIEW"` is the
plant signal. `REQUEST_REVIEW` is `state`, not a sibling key.

Known-plant sample: historical PR #30 (must fail this predicate; not
leftover evidence). Live user list is empty (`requested_reviewers: []`);
the team plant is in `requested_reviewers_teams`. Checking only teams
would ignore a user plant; checking only `requested_reviewers` would
pass #30.

`GET /api/v1/repos/code/voting/pulls/30` fragment (historical while open):

```json
{
  "draft": false,
  "requested_reviewers": [],
  "requested_reviewers_teams": [
    {
      "id": 4,
      "name": "maintainers",
      "organization": { "id": 4, "name": "code" }
    }
  ]
}
```

`GET /api/v1/repos/code/voting/pulls/30/reviews` fragment:

```json
[
  {
    "id": 217,
    "official": true,
    "state": "REQUEST_REVIEW",
    "team": { "id": 4, "name": "maintainers", "organization": null }
  }
]
```

`{n}` is the dedicated plant-check PR opened after S2+WP-pre (the
successor) is on `main`, **not** merely after `#30` merged. GET both
endpoints immediately after open. If any plant signal is present, fail.
If all clauses hold, wait once 30 seconds and re-GET both before pass;
pass only if the second pair still satisfies all clauses. Record `{n}`
and the two JSON bodies (the pair used for the pass decision) plus the
four-path `test ! -e` check on `main` on leftover tracker `{L}`.
**Close `{n}` without merging it.** After close, `GET .../pulls/{n}`
has `merged == false` and `state == "closed"`. Then close leftover
**issue** `{L}` (issue close, not via merge). Closing `{L}` is not
permission to merge `{n}`. PR #30’s own official request does not pass.
PR #29’s plant does not pass. Merge of `#30` does not pass. “The next
natural PR” does not pass. A `{n}` that edits product code or a
CODEOWNERS path does not pass. A merged `{n}` does not pass.

**CI (land prerequisite).** Woodpecker context `ci/woodpecker/pr/woodpecker`
must be **present and success** on the **successor** product-PR head
(**WP-pre**), created by Woodpecker (creator/target `ci.cl8y.com`), not a
manual Forgejo POST. Pending, failure, error, or a hand-posted status is
not **WP-pre**. Drain comments such as `drain skip: no occupying job…` are
**#429** / CAC, not a remaining-work failure. Empty statuses on `1006f8b`
/ `eb352b8` / `1d85943` are why **WP-pre** remains in this slice. Confirm
Woodpecker project enablement **before** pushing the successor
(Decision 8). If autoland waits on #429, an operator still SHA-pins
`Do: merge` (**H30-4**).

## Failure modes

| Mode | Handling |
| --- | --- |
| File restored on a branch after it left `main` | New non-WIP PRs that change a file plant official review again. Fail **H30-1**. Do not use the design SHA as the product tip. |
| Occupying, drafting, or pushing onto closed `pulls/30` | Forbidden. Remaining land is a new PR from current `origin/main`. |
| `Closes #30` on the successor | No-op. File `{R}` and `Closes #{R}`. |
| Copy left in `docs/CODEOWNERS`, `.gitea/`, or `.forgejo/` (including empty/comments-only) | Forgejo still loads the first existing path and may plant. Land fails **H30-1** (`test ! -e` false); delete those paths too (none exist on current `main`). Standing `docs/adr/` and `docs/ARCHITECTURE.md` are not CODEOWNERS. The throwaway `docs/h30-plant-check.md` is leftover-only and must not land on the successor. |
| Treating merged `#30` as remaining land (S1 without S2 / WP-pre) | **H30-1** true, README/ADR/architecture / posting context absent from `main`. Forbidden. Empty statuses also fail **H30-3**. |
| Firing S3 because `#30` merged | Remaining S2+WP-pre is not on `main`. Open `{n}` only after the successor merges. |
| cl8y-forgejo migrate/apply re-copies a template | Sister-repo race ([cl8y-forgejo#48](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/issues/48) `_ensure_codeowners`). Out of this slice. If a later apply re-adds the file, delete again via PR; never direct-push `main`. |
| Official request leftover on merged #30 or open #29 | Non-blocking (**H30-8**). Optional human dismiss. Not a rollback signal. Not leftover-complete evidence. |
| Treating merge of `#30` as leftover-complete | `#30` closed before S3 and before S2. Use leftover `{L}` after the successor. |
| Operator attestation without the dedicated PR | False-pass. Fail S3; open the dedicated non-WIP `{n}` on `docs/h30-plant-check.md`. Attestation may only record its GETs. |
| Plant-check PR has empty diff | False-pass: Forgejo does not plant on a no-op even if `CODEOWNERS` remains. Fail S3; open a new dedicated PR whose only path is `docs/h30-plant-check.md`. |
| Plant-check `{n}` touches `ledger/`, `operator-voting/`, `frontend/`, `deploy/`, or a CODEOWNERS path | Invalid. Open a new `{n}` whose only path is `docs/h30-plant-check.md`. |
| Merging the plant-check PR | Lands a throwaway docs note (or a product edit if the path constraint was ignored). Hello failure mode of the same name. **Close `{n}` without merge.** After close, `GET .../pulls/{n}` has `merged == false` and `state == "closed"`. Closing the leftover issue is a different contract. |
| WIP/draft follow-up used as “no new plant” | Forgejo skips CODEOWNERS on WIP. Invalid. Use a non-WIP PR into `main`. |
| Pass-iff ignores `requested_reviewers` | User plant would pass; historical #30 has `[]` so teams-only looks green while Tests say fail if users were requested. |
| Pass-iff matches `@code/maintainers` | Misses live team `maintainers` / `id: 4`; use the fail-closed pair. |
| `Fixes` / `Closes` / `Resolves` for leftover `{L}` on `{R}` or the successor | Merge closes the evidence issue; Decision 9 forbids those keywords. |
| Using `cac-design-issue-30` / `bc15b2d` as the product tip | Restores root `CODEOWNERS`; reject. Fork `origin/main` and copy the two docs files. |
| Requiring a `CODEOWNERS` delete versus current `origin/main` | Wrong extra-diff contract. File is already absent. |
| `ci/woodpecker/pr/woodpecker` never posts | **Land blocked** (**WP-pre** / land criterion 3). Do not `force_merge`; do not drop **H30-3**; do not POST a fake status. Diagnose: missing YAML, wrong path (not root `.woodpecker.yaml`), or repo not enabled on Woodpecker. File: `{R}` implement. If YAML is on the successor and nothing posts: named forge unblock (cl8y-forgejo#48 or a local dex#1247-shaped leftover) **plus retrigger**. Empty statuses on merged `#30` do not waive this. |
| Context present but not success, or hand-posted | **WP-pre** incomplete. A failing/pending Woodpecker run or a manual Forgejo POST (creator/target not `ci.cl8y.com`) does not land. Re-run or repair; do not `force_merge`. |
| `.woodpecker.yml` or `.woodpecker/<other>.yaml` added | Wrong or ignored context; **H30-3** fails. |
| GitLab treated as **H30-3** | Those workflows do not post `ci/woodpecker/pr/woodpecker`. Forgejo `has_actions=false`. Keep them; do not drop Woodpecker. |
| Protection silently reverted to official-review true | Merge 405 returns. Out of this repo; re-apply via forge policy, do not `force_merge`. Not proven by a green `find` step. |
| `enable_push` flipped true | **H30-2** regression. Refuse. |
| Re-adding CODEOWNERS “for safety” in a follow-up | Violates **H30-1**. Reviewers must reject unless a new ADR allowlists path owners. |
| CAC autoland waits on #429 | Operator `Do: merge` still closes `{R}`. Do not implement occupying-job cleanup here. |
| `block_on_outdated_branch` blocks merge | Rebase the successor onto current `main`. Do not PATCH the flag off. |
| Implement PATCHes protection or edits CAC / Coolify / ledger / frontend | Out of authority / wrong repo. |
| `git grep '^\.\* @'` as the land check | After a correct absence, that command exits **1** (`set -e` false-fail). A comments-only leftover is a grep pass. Land predicate is `test ! -e` on all four paths. |
| Rewrite of ADR/architecture on the product tip vs the accepted SHA | Land fails the byte-identity gate. Copy the two files; do not edit **H30** while combining. |

## Ordered implementation slices

| Slice | Work | Mergeable? | Depends on |
| --- | --- | --- | --- |
| **S0** | This design on `cac-design-issue-30` (ADR 0001 + architecture **H30**). Transport only. Still carries root `CODEOWNERS` versus current `origin/main`. | No. Do not open or merge as a product PR. Do not use as the product tip. | None in `code/voting`. Woodpecker enablement is a **WP-enable** gate, not an S0 file dep. |
| **S1** | **Historical incomplete.** Root `CODEOWNERS` deleted on `origin/main` via merged `#30` (`eb352b8` / `1d85943`, empty statuses). Other three paths absent. | Already merged. **Not** remaining land. Do not occupy `#30`. Do not re-delete versus current `main`. | — |
| **S2** | **New branch from current `origin/main`:** copy standing docs **byte-identical** from the accepted SHA + README pointer (Decision 3 body). Add the quoted root `.woodpecker.yaml`. No ledger / operator-voting / frontend / deploy / skill / OPS edits. File remaining-work `{R}`. Open leftover `{L}` before successor merge. | Only as the **combined** successor PR with WP-pre. | S0 accepted. S1 already on `main`. |
| **WP-enable** | **Pre-merge gate.** Confirm `code/voting` is an active Woodpecker project **before** pushing the successor (optional pass/fail GET: Forgejo hook → `https://ci.cl8y.com/api/hook` with `push`+`pull_request`, or Woodpecker `active`). File owner ≠ posting owner. If YAML later posts nothing: named unblock (cl8y-forgejo#48 or a local dex#1247-shaped leftover) plus retrigger. Fake status POSTs and `force_merge` forbidden. Empty `#30` statuses do not skip this. | N/A. Blocks successor WP-pre. | Not a local iid. Named forge ops. |
| **WP-pre** | **Land prerequisite (not leftover).** Successor-PR head has a real **success** `ci/woodpecker/pr/woodpecker` from Woodpecker (`ci.cl8y.com`). File owner: `{R}` implement (quoted YAML). Enablement owner: forge ops. Blocks merge of S2. Merge of `#30` does not satisfy this. | N/A as a solo merge. | WP-enable; S2 file on the successor. |
| **Product PR** | **New** PR from current `origin/main` (not `pulls/30`). `git diff origin/main...HEAD` is **only** two `docs/` files **byte-identical** to the accepted SHA + README pointer + quoted root `.woodpecker.yaml`. Body `Closes #{R}` (not `Closes #30`). Leftover `{L}` already open. | Yes, once S2+WP-pre are on the head, rebased, Woodpecker green. | S0 accepted; S2+WP-enable+WP-pre combined. |
| **S3** | Leftover-complete “no new plant” (tests item 4). Dedicated post-merge **non-WIP** plant-check PR `{n}` whose only path is `docs/h30-plant-check.md`. **Close `{n}` without merge.** Operator attestation may record that PR’s GETs; it is not a substitute. Not a close gate for `{R}`. Must not fire merely because `#30` merged. | N/A | **Successor** (S2+WP-pre) merged to `main`. |

The successor ships **S2+WP-pre** only. S1 is already on `main`.

Sister repos (not slices of remaining-work `{R}`, not local `DEPS`): forge
#48 protection+templates+Woodpecker project repair; CAC #429 autoland
occupying job; hello#15 historical delete-only canary (merged); dex#1309
landed complete pattern; dex#1247 enablement shape; voting #29 Renovate;
closed `#30` incomplete S1.

### Combine step (S2+WP-pre onto a new product vehicle)

Do this on a **new** branch from current `origin/main` (example:
`chore/h30-s2-wp-pre`), not on `cac-design-issue-30`, not on
`chore/remove-catchall-codeowners`, and not on closed `pulls/30`.
**WP-enable first:** confirm this repo is a Woodpecker project before the
pushes below.

1. File remaining-work `{R}` (Decision 4 paste). Do not occupy `#30`.
2. `git fetch origin && git checkout -b chore/h30-s2-wp-pre origin/main`.
3. Fetch published design: `origin/cac-design-issue-30` at the
   independently accepted SHA. **Copy**
   `docs/adr/0001-remove-catchall-codeowners.md` and
   `docs/ARCHITECTURE.md` **byte-identical** from that SHA onto the new
   branch. Do not rewrite those files while combining. Never reset the
   branch to a design SHA. Never merge `cac-design-issue-30`. Never merge
   or cherry-pick a tip that re-adds `CODEOWNERS`.
4. Add the README **Merge gate** paragraph as the Decision 3 body (do not
   stub the product README). Index ADR 0001 / **H30** in Docs for agents.
5. Add **exactly** the quoted root `.woodpecker.yaml` (Decision 8 /
   architecture **Pipeline**). No `.yml`, no `.woodpecker/`, no hello/dex
   product steps, no GitLab rust/frontend port.
6. Open leftover issue `{L}` that owns S3 (Decision 9 paste). Required
   **before** merge (tests item 6 / land criterion 6). `{R}` and the
   successor must not contain close keywords for `{L}`. Do not point
   leftover-complete at closed `#30`.
7. Rebase onto current `origin/main` (`block_on_outdated_branch` is
   observed-true; do not PATCH it off). Push the **new** product branch.
   Open a **new** PR targeting `main` whose body contains `Closes #{R}`.
   Do not push onto closed `pulls/30`. Do not write `Closes #30`.
8. Confirm `git diff <accepted-sha> -- docs/adr/0001-remove-catchall-codeowners.md docs/ARCHITECTURE.md`
   is empty, and `git diff origin/main...HEAD` contains **only**: those two
   docs files, README Merge-gate + docs index, quoted root
   `.woodpecker.yaml`. `CODEOWNERS` must not appear in that diff.
9. Ready the PR only when that product diff is the tip and
   `ci/woodpecker/pr/woodpecker` has posted **success**. Then merge
   with SHA-pinned `Do: merge`. If YAML is present and nothing posts:
   named unblock plus retrigger; do not fake a status. Empty statuses on
   merged `#30` do not skip this.

## Tests

In-repo CI cannot GET branch protection. Existing Cargo tests, frontend
`npm test`, Playwright, SPA fallback, and GitLab gitleaks stay ungated
and must not be rewritten for this ticket. The **#30 land bootstrap** does
not run those suites. Any later real Forgejo CI extends the same root
`.woodpecker.yaml` (optional leftover; **not** a close gate for `{R}`). Do
not add a product test solely for file absence.

1. **Absence** (on the successor tip / after remaining land). Scope to the
   four Forgejo CODEOWNERS paths; do not `git grep` the whole tree (this
   ADR quotes `.* @code/maintainers`). Land predicate is four-path
   absence (already true on current `origin/main`; successor must not
   restore):

   ```
   test ! -e CODEOWNERS
   test ! -e docs/CODEOWNERS
   test ! -e .gitea/CODEOWNERS
   test ! -e .forgejo/CODEOWNERS
   ```

   After a correct absence, uninverted `git grep -nE '^\.\* @' -- CODEOWNERS
   docs/CODEOWNERS .gitea/CODEOWNERS .forgejo/CODEOWNERS` exits **1**. If a
   grep stays as a secondary scan, invert it so `set -e` does not
   false-fail:

   ```
   ! git grep -nE '^\.\*\s+@\S+' -- CODEOWNERS docs/CODEOWNERS .gitea/CODEOWNERS .forgejo/CODEOWNERS
   ```

   A comments-only leftover is a grep pass — grep is not the land
   predicate.

2. **PR pipeline (land / WP-pre).** Root `.woodpecker.yaml` on the successor
   head is the Decision 8 / architecture **Pipeline** file (`steps:`,
   two-item `when:`, digest-pinned alpine, `find` one-liner, no secrets).
   Woodpecker context `ci/woodpecker/pr/woodpecker` is **present and
   success** on that head, created by Woodpecker (creator/target
   `ci.cl8y.com`, not a manual Forgejo POST), and is the merge check.
   Cargo / GitLab gitleaks stay ungated. Empty statuses on `eb352b8` /
   `1d85943` do not pass.

3. **Protection (operator read).** Same list as close criterion 4: GET
   `main` still equals **H30-2**, **H30-3**, **H30-5**, **H30-8**, **H30-9**.
   Fail if `enable_push` is true, official-review block is true,
   `required_approvals` is not 0, reject-block is false, or the Woodpecker
   context is missing. Do **not** treat observed flags or **H30-4** as
   GET-match fields. Prior timestamp is attested; this test is
   a close re-GET. Unauthenticated GET is 401.

4. **No new plant** (leftover-complete; **not** a `{R}` close gate; **not**
   triggered by merge of `#30`). After S2+WP-pre are on `main`:

   - Open a **non-WIP** PR **into `main`** whose `git diff origin/main...HEAD`
     is **exactly** the added throwaway `docs/h30-plant-check.md` (one
     line, e.g. `H30 plant-check; do not merge.`). The tip **must not**
     contain `CODEOWNERS`.
   - `{n}` **must not** change `ledger/`, `operator-voting/`, `frontend/`,
     `deploy/`, or any of the four CODEOWNERS paths.
   - Do not request users or teams in the UI.
   - `GET /api/v1/repos/code/voting/pulls/{n}` and
     `GET .../pulls/{n}/reviews` immediately after open, then wait 30s
     and re-GET.
   - Pass = Observability fail-closed pair on both GET pairs.
   - **Close `{n}` without merging it.** After close, `GET .../pulls/{n}`
     has `merged == false` and `state == "closed"`. Then paste the
     record on leftover tracker `{L}` and close that **issue** (issue
     close, not via merge). Those are two different contracts.
   - **Disqualify** merged `pulls/30` and leftover `pulls/29`. A
     draft/WIP, empty-diff, product-path, or **merged** follow-up is not
     proof. Merge of `#30` is not S3 start.
   - Operator attestation may record those GETs; it is not a substitute
     for this PR and does not authorize merging `{n}`.

5. **Reject still blocks (doc-level).** Do not turn off
   `block_on_rejected_reviews` to “make autoland easier.”

6. **Leftover issue `{L}` exists before merge of the successor** (same as
   land criterion 6). The S3-owning follow-up issue is open before
   SHA-pinned `Do: merge` of the successor that `Closes #{R}`. Merging the
   successor closes `{R}`; S3 must not live only there. Close-keyword
   contract: `{R}` / successor text has no `Fixes` / `Closes` / `Resolves`
   for leftover tracker `{L}`. Do not require leftover-before-merge-of-`#30`
   (already missed).

7. **Product untouched + byte-identity.** `git diff origin/main...HEAD` on
   the successor has no `ledger/` / `operator-voting/` / `frontend/` /
   `deploy/` / `skills/` / `.gitlab-ci.yml` / `.gitleaks.toml` /
   `.githooks/` / `docs/LEDGER_INVARIANTS.md` / `docs/OPERATOR_VOTING.md` /
   `docs/FRONTEND.md` / `docs/OPS.md` / `docs/HANDOFF.md` edits, and **no**
   `CODEOWNERS` path. The two standing docs files match the independently
   accepted SHA
   (`git diff <accepted-sha> -- docs/adr/0001-remove-catchall-codeowners.md docs/ARCHITECTURE.md`
   is empty). Allowed extras versus that SHA plus current `origin/main`
   are README Merge-gate + docs index and the quoted root
   `.woodpecker.yaml` only.

## Rollout

- Merge vehicle: **one new** PR from current `origin/main`, after the
  combine step. Diff versus `origin/main` must be **only** ADR 0001 +
  architecture **byte-identical** to the independently accepted SHA +
  README pointer (Decision 3) + root `.woodpecker.yaml` (Decision 8).
  Design-only `cac-design-issue-30` must not be opened or merged as the
  product PR and must not be the product tip. Do not push onto closed
  `pulls/30`. Do not require `Closes #30`. Successor body `Closes #{R}`
  and leftover `{L}` already open.
- Order: protection already live (re-GET at close) → file `{R}` →
  **WP-enable** preflight → fork `origin/main` → copy two docs + README
  + quoted YAML → leftover `{L}` remains open → rebase → Woodpecker
  **success** on the successor head (or named unblock plus retrigger) →
  `Do: merge` → **`{R}` closes**. Then leftover-complete test 4 on a
  dedicated post-merge non-WIP plant-check PR `{n}` whose only path is
  `docs/h30-plant-check.md`. **Close `{n}` without merge.** Operator
  attestation may record that PR’s GETs; it is not leftover-complete.
  Merge of `#30` does not start this leftover sequence.
- Other `code/*` catch-all deletions may copy this pattern; this ADR does
  not merge those repos. Repos that already post the context (dex#1309)
  do not need a new YAML on a delete PR; this tree still does, because
  `#30` merged without posting. Do not copy hello or dex YAML contents.
  Do not treat hello#15 as still open.
- [#297](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/297):
  no deploy, spend, custody, or CAC policy expansion. Landing remaining-work
  `{R}` does not authorize Coolify rebuild, Legal admin, wallet/ledger
  changes, or autonomy edits.

## Rollback

Restore the previous `CODEOWNERS` **via PR**, not direct `main`. That
re-plants official requests. It does **not** by itself re-enable
merge-block (`block_on_official_review_requests`); restoring the 405 gate
is a forge-policy revert, founder-scoped, and is not a `voting`
rollback step.

File-only rollback that leaves this ADR and architecture on `main` (still
saying CODEOWNERS must be absent) is **intended**: standing docs keep
telling later agents not to treat the restored file as a merge gate. Do not
delete the docs as part of restoring the file unless a new ADR reverses
this decision.

Do **not** delete the Woodpecker file as part of a CODEOWNERS rollback
(that re-boxes **H30-3**). WP rollback is a separate PR and is not required
to undo the catch-all. Ledger / operator-voting / frontend / deploy files
stay untouched. Do **not** delete `.gitlab-ci.yml` as part of rollback.

## Integration completion criteria

### Land (S2+WP-pre) — merge of the successor that `Closes #{R}`

All must be true on the merged tip. This is what merging the successor
completes. It does **not** wait for S3. Merge of `#30` does **not**
complete this list.

1. `main` has no CODEOWNERS file at the four Forgejo paths: `test ! -e`
   is true on `CODEOWNERS`, `docs/CODEOWNERS`, `.gitea/CODEOWNERS`,
   `.forgejo/CODEOWNERS` (**H30-1**). Already true on current
   `origin/main`; successor must not restore.
2. Product tip `docs/adr/0001-remove-catchall-codeowners.md` and
   `docs/ARCHITECTURE.md` are **byte-identical** to the independently
   accepted SHA (this SHA or a successor after independent review).
   Allowed extras versus that SHA plus current `origin/main` are README
   Merge-gate + docs index and the quoted root `.woodpecker.yaml` only.
   Architecture-only is not sufficient. Do not rewrite **H30** while
   combining. Do not use `cac-design-issue-30` as the product tip.
3. The successor-PR head has a real **success** `ci/woodpecker/pr/woodpecker`
   status from Woodpecker / `ci.cl8y.com` (**WP-pre** / **H30-3**); no
   deploy secrets ran on that PR event; mergeable under **H30-4**
   (SHA-pinned `Do: merge`). A failing or hand-posted status is not this
   criterion. Empty statuses on `eb352b8` / `1d85943` are not this
   criterion.
4. Protection GET still matches **H30-2**, **H30-3**, **H30-5**, **H30-8**,
   **H30-9** (same list as test 3; operator re-read at merge; not inferred
   from a green `find` step; not **H30-4**; not observed flags; prior
   `updated_at` was attested).
5. No `force_merge`, no direct `main`, no CAC dismiss-as-merge, no
   Coolify/HMAC/`autonomy.rs`/ledger/frontend/deploy edits in
   the successor diff. No push onto closed `pulls/30`. No `Closes #30`
   required.
6. Leftover issue `{L}` exists that owns S3 (because merging the successor
   closes `{R}`). Same as tests item 6. Required before merge of the
   successor. `{R}` has no close keywords for `{L}`. Point leftover-complete
   at `{R}`/`{L}`, not at closed `#30`.

Merged delete-only `#30` is not remaining land. Missing Woodpecker (file
**or** enablement) is not land. A successor that restores `CODEOWNERS` is
not land. A successor that uses the transport ref as the product tip is
not land.

### Leftover-complete (S3) — follow-up issue `{L}`; survives merge of `{R}`

1. A **dedicated** PR `{n}` opened **after** the successor landed (S2+WP-pre
   on `main`), **non-WIP**, whose `git diff origin/main...HEAD` is
   **exactly** the added throwaway `docs/h30-plant-check.md`: Observability
   three fail-closed clauses. `{n}` **must not** change `ledger/`,
   `operator-voting/`, `frontend/`, `deploy/`, or any CODEOWNERS path.
   Record `{n}`, the two JSON bodies used for the pass decision, four-path
   `test ! -e` on `main`, and the close GET (`merged == false`,
   `state == "closed"`). **Close `{n}` without merging it.** Then close
   leftover **issue** `{L}` (issue close, not via merge). Closing `{L}` is
   not permission to merge `{n}`. `#30`’s leftover plant does not count.
   `#29`’s plant does not count. Merge of `#30` does not count and does
   not start S3. “The next natural PR” does not count. An empty-diff, WIP,
   product-path, or **merged** dedicated PR does not count. Operator
   attestation of those GETs is not leftover-complete without this PR.

Forgejo#48 leftovers, CAC#429, and voting #29 (Renovate) may stay open;
they are not land or leftover gates for this tree. Enablement/repair
needed so **WP-pre** can post **is** a land gate (owned above), not a
leftover. A later leftover to extend root `.woodpecker.yaml` with product
CI is optional and is **not** a close gate for `{R}`.

## Authority

[cl8y-agent-control#297](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/297):
this design does not grant deploy, spend, custody, or agent-permission
expansion. Relaxing official-review as a **forge merge gate** is already
executed on this repo by #48 (`updated_at` 2026-09-21T07:29:30Z); this ADR
only records the in-tree file removal (historical S1) and adds standing
docs plus the minimal pipeline so **H30-3** can post. Independent review
of this proposal is a later gate. This document is not architecture
approval. Merge of `#30` is not architecture approval.
