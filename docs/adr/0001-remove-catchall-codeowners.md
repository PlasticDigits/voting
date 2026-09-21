# ADR 0001: Remove catch-all CODEOWNERS

## Status

Proposed ([#30](https://git.cl8y.com/code/voting/issues/30)). Not
accepted by this design pass. Keywords in the issue body are not
architecture approval. Ordinary design is not a founder card. There is no
separate standing issue `#30`; the issues URL and the product PR share one
number (`html_url` → `pulls/30`).

Overview (product tree, merge gate, pipeline path/layout):
[`ARCHITECTURE.md`](../ARCHITECTURE.md). Do not copy that table here. Local
invariant IDs are **H30-*** (this ticket). `code/hello` architecture **H15**
and sister-repo **H3** / **H5** / **MG** tables are other trees’ copies of
the same flags, not this repo’s issue number.

Design branch `cac-design-issue-30` is transport only; it is not the product
PR. Do not open a design-only PR.

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

**Land** is S1+S2+**WP-pre** on PR
[#30](https://git.cl8y.com/code/voting/pulls/30) (default vehicle: push
the combined diff onto that existing PR). Merging that PR **closes `#30`**.
A successor is allowed only if its body contains `Closes #30` **and** the
S3 leftover issue already exists. Merging a successor does **not** close
`#30` without that footer.
**Leftover-complete** (S3: dedicated post-merge plant-check PR with a
**non-empty**, **non-WIP** diff) is tracked on a follow-up issue / leftover
checklist that survives that merge. S3 is not a close gate for `#30`.
Operator attestation may **record** the plant-check GETs of that dedicated
PR; it is not a substitute for the PR.

This is a product-tree copy of the pattern named by
[cl8y-forgejo#48](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/issues/48).
The canary is `code/hello` [#15](https://git.cl8y.com/code/hello/issues/15)
(still open; `CODEOWNERS` still on hello `main`). The **landed product
pattern** is
[code/cl8y-dex-terraclassic#1309](https://git.cl8y.com/code/cl8y-dex-terraclassic/issues/1309)
(CODEOWNERS-only delete, pre-existing green WP). This repo has **no**
in-tree Woodpecker pipeline and empty commit statuses on `main` / PR #29 /
PR #30, so land must produce the posting context here (**WP-pre**).
Protection on `code/voting` `main` already matches the intended flags
(GET `updated_at` 2026-09-21T07:29:30Z; **H30-8** / **H30-9** / **H30-2** /
**H30-3** / **H30-5**; unauthenticated GET is 401). Close still re-GETs.
#30 does not re-roll protection, does not implement CAC autoland, and does
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
Drain must not delete `CODEOWNERS` as a workaround; this designed PR is the
allowed product-tree path.

This tree has no root `.woodpecker.yaml`. Do not add `.woodpecker.yml` or
`.woodpecker/` (directory mode can ignore root YAML and post
`ci/woodpecker/pr/<other>`). There is **no** voting sister ticket that
will add one before S1 (open numbers are only #30 this delete and #29
Renovate). Hello#15 and dex#1309 could merge a CODEOWNERS-only delete
because those trees already posted `ci/woodpecker/pr/woodpecker`. This
tree cannot: land criterion 3 / **H30-3** requires that context, and this
slice must not `force_merge` or drop **H30-3**. Therefore the deletion PR
**may and must** add the quoted **#30 land bootstrap** (architecture
**Pipeline**). Missing WP YAML **or** missing Woodpecker project enablement
is a land blocker for `#30`, not leftover.

[`.gitlab-ci.yml`](../../.gitlab-ci.yml) already runs gitleaks / cargo /
frontend unit / SPA fallback on GitLab. Forgejo `has_actions=false`; those
jobs do **not** post the required context. They stay leftover hosting CI.
Do not delete them in #30. Do not port `test:rust` / `test:frontend` /
`lint:gitleaks` into Woodpecker on this PR.

[cl8y-forgejo ADR 0003](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/src/branch/main/docs/adr/0003-community-forge-and-woodpecker.md)
item 8 (“CODEOWNERS plus protected `main`”) is amended on the forge repo for
the **official-review** half only. This tree’s README is product deploy docs
and does not currently claim CODEOWNERS is the trusted-PR gate; S2 still
adds an explicit **H30** pointer so later agents do not re-add
`.* @code/maintainers`. The issue body links cl8y-forgejo
[`docs/INVARIANTS.md`](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/src/branch/main/docs/INVARIANTS.md);
that file is not in this tree. Do not invent a local copy.

Live proof that the file still plants requests (this pass):

| PR | Role | Plant |
| --- | --- | --- |
| [#30](https://git.cl8y.com/code/voting/pulls/30) | This delete (open, `draft: false`, **1 file** / 6 deletions, `mergeable: true`) | `requested_reviewers` = `[]`; `requested_reviewers_teams` = `maintainers` (id 4, org `code`); `official: true` `REQUEST_REVIEW` id **217** at 2026-09-21T07:43:10Z. Head `eb352b8` deletes root `CODEOWNERS` and nothing else. Evidence of the catch-all (Forgejo still loads `CODEOWNERS` from `main`). **Disqualified** as “no new plant” / leftover-complete. hello#15 trap: 1 file, `mergeable: true`. |
| [#29](https://git.cl8y.com/code/voting/pulls/29) | Renovate onboarding (open, 1 file) | `requested_reviewers` = `[]`; `requested_reviewers_teams` = `maintainers`; `official: true` `REQUEST_REVIEW` id **163** at 2026-09-20T23:56:23Z |

Those leftovers are **disqualified** as “no new plant” evidence. Protection
GET on `main` has official-review **block** off (**H30-8**), so leftover
requests must not be treated as merge blockers.

`#30` as it sits cannot land: S1-only, empty Woodpecker statuses, no
standing docs / README pointer. Convert #30 to **draft** before pushing a
combined head: that lock is against a later **delete-only** tip that
somehow gets a green context (hello#15), not a substitute for **WP-pre**.

Drain comments on #30 (`no occupying job…`; queued `design_author` without a
Hetzner VM) are
[cl8y-agent-control#429](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/429)
/ CAC, not a #30 failure. They are not permission to merge it.

This tree has **no** vendor `CODEOWNERS`. Scope absence checks to the four
Forgejo search paths.

## Non-goals

- Forgejo protection JSON / `apply_repo_policy.py` / migrate
  `_ensure_codeowners` / templates
  ([cl8y-forgejo#48](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/issues/48)).
- CAC autoland predicates, occupying jobs, or `DrainSkip::OfficialReview`
  cleanup ([cl8y-agent-control#429](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/429),
  leftover of #388).
- Dismissing reviewers from the controller (forbidden substitute in #388),
  including the leftover on #30 / #29.
- Path-specific CODEOWNERS, a second maintainer, or `required_approvals: 1`.
- Weakening **H30-3**. The quoted root `.woodpecker.yaml` (**#30 land
  bootstrap**, architecture **Pipeline**) **is allowed** on the deletion PR
  so **H30-3** can post; see **WP-pre**. MUST path: root `.woodpecker.yaml`.
  Do not add `.woodpecker.yml` or `.woodpecker/`. Do not `force_merge`. Do
  not post a fake commit status. Do not add Coolify, Telegram, BSC RPC,
  Terra LCD, Legal admin, or bind deploy secrets to `pull_request`. Do not
  copy hello’s `coolify-deploy` / `telegram-failure`. Do not copy dex’s
  `scripts/ci/gitleaks-scan-tracked.sh`. Do not port `.gitlab-ci.yml`
  `test:rust` / `test:frontend` / `lint:gitleaks` onto this PR.
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
- Opening `cac-design-issue-30` as a PR, or merging that transport ref to
  `main`.

## Decision

1. **Delete** root `CODEOWNERS`. Do not leave an empty or comments-only file
   (Forgejo still parses it).
2. **Do not add** `docs/CODEOWNERS`, `.gitea/CODEOWNERS`, or
   `.forgejo/CODEOWNERS`. After land, `test -f` fails on all four paths.
   None of those paths may contain a reviewer rule for any pattern (not
   only `.*`).
3. **Keep** the merge gate in [`ARCHITECTURE.md`](../ARCHITECTURE.md)
   **H30**. On the product PR, **add** exactly this paragraph to root
   `README.md` after the **Local git hooks** section. Keep existing product
   tables, token registry, and Docs-for-agents index; add ADR 0001 +
   architecture **H30** to that table. Do not claim maintainers review every
   change. Architecture-only is **not** sufficient.

```markdown
## Merge gate

Merge to `main` is a pull request, Woodpecker context
`ci/woodpecker/pr/woodpecker`, and SHA-pinned `Do: merge`
([architecture **H30**](docs/ARCHITECTURE.md)). Official CODEOWNERS review
is not a merge gate. Do not re-add catch-all `CODEOWNERS`
(`.* @code/maintainers`); see
[ADR 0001](docs/adr/0001-remove-catchall-codeowners.md).
```

4. **One product PR** whose diff is: delete `CODEOWNERS` + ADR 0001 +
   architecture **H30** + README pointer + the **WP-pre** pipeline file.
   Default vehicle: push that combined diff onto
   [#30](https://git.cl8y.com/code/voting/pulls/30).
   `cac-design-issue-30` is transport; do not merge it to `main` and do not
   open it as the product PR. If a successor is required, its body **must**
   contain `Closes #30`, and the S3 leftover issue **must** already exist.
5. **Leave** already-planted official requests on open PRs (including #30
   and #29). They are non-blocking under **H30-8** / **H30-9**. Do not
   dismiss them from CAC. Human dismiss is optional leftover, not AC. They
   do not count as “no new plant.”
6. **Do not** PATCH branch protection from this repository.
7. **Split land from leftover-complete.** PR `#30` (or a `Closes #30`
   successor) is S1+S2+**WP-pre** only. S3 lives on a follow-up issue
   opened before that merge (plus the leftover checklist below). Require a
   **dedicated** post-merge plant-check PR that **changes at least one
   file**, is **non-WIP**, and is opened **after** the delete is on `main`.
   Do not accept `#30`’s leftover plant, `#29`’s plant, or “the next
   natural PR.” A no-op dedicated PR is not evidence: `.*` matches every
   path, and Forgejo does not plant on a no-op even if `CODEOWNERS` is
   still on `main` (dex#1309 no-op warning). A draft/WIP follow-up is not
   evidence (Forgejo skips CODEOWNERS on WIP). Operator attestation may
   **record** the GETs of that dedicated PR; it is not a substitute
   for opening it.
8. **WP-pre (land prerequisite, owned).** The deletion-PR head SHA must
   show a real Woodpecker **success** status `ci/woodpecker/pr/woodpecker`
   (creator/target `ci.cl8y.com`, not a manual Forgejo POST) before
   SHA-pinned `Do: merge`. Pending, failure, or a hand-posted status is not
   **WP-pre**.

**File owner:** implement of `#30` — no sister voting ticket exists to
add YAML first, so put **exactly** this file at root `.woodpecker.yaml` on
the deletion PR (same bytes as architecture **Pipeline**):

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
active Woodpecker project **before** combining onto `#30` (Forgejo repo id
**43** on `ci.cl8y.com`; webhook to `https://ci.cl8y.com/api/hook` for
`push` + `pull_request`). File owner ≠ posting owner (forge ops). Empty
statuses with no YAML are ambiguous: missing file **or** repo not
registered. dex#1247’s 44/44 `code/*` enablement on 2026-09-12 is **not**
this preflight.

If the YAML is on the PR and nothing posts: **land blocked**. Do not treat
a missing context as leftover, “out of this slice,” or silent sister-ops.
Block on a **named** forge unblock
([cl8y-forgejo#48](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/issues/48)
or a local leftover shaped like
[dex#1247](https://git.cl8y.com/code/cl8y-dex-terraclassic/issues/1247))
**plus retrigger**. Do not drop **H30-3**. Do not `force_merge`. Do not POST
a fake status.

9. **Leftover tracker** — `fj issue create`, not a PR. Open it **before**
   `Do: merge` of #30. Leftover-complete closes that **issue** (issue
   close after the record is pasted), not via merge. **#30 must not
   contain close keywords for that number.** Issue #30 and PR #30 bodies,
   titles, and later comments must not use `Fixes #<tracker>`,
   `Closes #<tracker>`, `Resolves #<tracker>`, or equivalent Forgejo close
   keywords pointing at the leftover tracker (merging #30 would swallow
   the evidence issue).

Paste:

```text
fj issue create --body-file leftover-tracker.md --no-template \
  "chore: leftover plant-check after catch-all CODEOWNERS delete"
```

`leftover-tracker.md` body (fields to fill later; `{n}` is the
dedicated plant-check PR):

```markdown
Post-merge evidence for removing catch-all CODEOWNERS.

Merging PR #30 closes issue #30; this tracker survives.

Do not merge a PR whose body uses Fixes / Closes / Resolves for this
issue number. Leftover-complete is an issue close after the record
below is pasted, not a merge.

## Record

- plant-check PR `{n}`:
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
| `CODEOWNERS` (root) | Remove file. |
| `docs/CODEOWNERS`, `.gitea/CODEOWNERS`, `.forgejo/CODEOWNERS` | Must remain absent (no empty file). |
| Forgejo PR review interface | After land, a **dedicated** non-WIP plant-check PR against `main` that **changes at least one file** must not get an official CODEOWNERS team request. `requested_reviewers_teams` from this file becomes empty for those new PRs. |
| Branch protection API | No write from this ticket. Operator GET must still match architecture **GET-required** (**H30-2**, **H30-3**, **H30-5**, **H30-8**, **H30-9**). Observed rows are do-not-touch. Prior `updated_at` is attested; close re-GETs. Unauthenticated GET is 401. |
| Root `.woodpecker.yaml` | **Required on the deletion PR.** **#30 land bootstrap** quoted in Decision 8 / architecture **Pipeline**. MUST that path. `steps:` + two-item `when:` + digest-pinned alpine + `find` one-liner. No secrets, Coolify, Telegram, BSC, Terra. Do not add `.yml` or `.woodpecker/`. Any later real CI extends this same file. |
| `.gitlab-ci.yml` | Unchanged leftover hosting CI. Not a Forgejo required context. |
| `.gitignore` | Unchanged. `docs/` is already trackable (`ARCHITECTURE.md` on `main`). Do not add a directory ignore of `docs/`. |
| `docs/ARCHITECTURE.md`, `docs/adr/0001-remove-catchall-codeowners.md` | Added/updated on the design branch; land with S1+S2. |
| Root `README.md` | **Edit** on the deletion PR: add the Decision 3 **Merge gate** paragraph and index ADR 0001 / **H30** in Docs for agents. Keep product tables, tokens, and verify commands. Do not stub. |
| `ledger/`, `operator-voting/`, `frontend/`, `deploy/`, `skills/`, `.gitleaks.toml`, `.githooks/`, product invariant docs, `code/maintainers` team | Unchanged. The team may keep existing; it simply is not planted as official review. Cargo / npm tests stay ungated. |
| Leftover tracker issue | `fj issue create` before merge of #30; owns plant-check; closed as an issue, not via merge. |

No runtime contract, schema, LCD, RPC, Legal, wallet, or HTTP API change.

## Affected invariants

| ID | Kind | Rule |
| --- | --- | --- |
| **H30-1** | Non-GET | `test -f` fails on `CODEOWNERS`, `docs/CODEOWNERS`, `.gitea/CODEOWNERS`, and `.forgejo/CODEOWNERS`. Absence, not “file exists but is not requesting reviewers.” File `@code/maintainers` / JSON team `maintainers` are the same leftover plant. |
| **H30-2** | GET-required | `enable_push=false` on `main`. |
| **H30-3** | GET-required | `enable_status_check=true` and required context `ci/woodpecker/pr/woodpecker`. |
| **H30-4** | Non-GET | Merge is SHA-pinned `Do: merge`. Never document or use `force_merge`. Not a protection field; omit from GET matching. |
| **H30-5** | GET-required | `block_on_rejected_reviews` stays true. An explicit REJECT still blocks. |
| **H30-6** | Non-GET | No Coolify/Woodpecker secrets on `pull_request`. This tree does not grow a deploy step from #30. Hello `telegram-failure` is forbidden. Coolify / Legal stays #7 / #297. |
| **H30-7** | Non-GET | This tree does not expand CAC merge/deploy/spend/custody policy. |
| **H30-8** | GET-required | `block_on_official_review_requests=false`. Leftover official requests on #30/#29 are non-blocking. |
| **H30-9** | GET-required | `required_approvals=0`. |

GET-observed flags (`block_on_outdated_branch`, `dismiss_stale_approvals`,
`apply_to_admins`) are listed in architecture only. They have no
close-criterion IDs. #30 must not PATCH them.

Product `L*` / `OV-*` / `O8` / `L-EVM*` in ledger / operator-voting /
frontend docs are unchanged. This ADR does not weaken them.

CAC invariants 29 / 67 / #388 stay: skip official-review deadlock; never
`force_merge`; drain does not delete CODEOWNERS. Product land makes the skip
class stop firing **for this repo** once new PRs have no plant.

## Alternatives

| Option | Why not |
| --- | --- |
| Keep file, rely on `block_on_official_review_requests=false` | Requests still plant on every non-WIP PR that changes a file (see #29 and #30); drain noise; Renovate/agent PRs look like they need a human stamp; templates can re-teach the old gate. |
| Replace `.*` with path owners | No second reviewer exists; same 405/422 if official-review is ever turned on; out of scope. |
| Add a second maintainer | Founder ops, not this implement. |
| Dismiss official requests from CAC | Forbidden by #388 as a substitute for policy reversal. |
| Direct-push the delete to `main` | Violates **H30-2**. File deletes go through a PR (already [#30](https://git.cl8y.com/code/voting/pulls/30)). |
| Empty or comments-only CODEOWNERS | Forgejo still parses it. Absence is the contract. |
| Wait for `code/hello` canary merge before this delete | Sister pattern, not a local iid. This repo’s protection is already rolled; the file still plants. hello#15 is still open. |
| Cite hello#15 as a landed product delete | The landed pattern is dex#1309. |
| Leave ADR only on `cac-design-issue-30` | Standing docs never reach `main`; later agents re-add `.* @code/maintainers`. |
| Merge live #30 as delete-only (S1 without S2 / WP-pre) | **H30-1** maybe true, README/ADR/architecture / posting context false; later agents have no “do not re-add” contract. hello#15 is the delete-only trap (1 file, mergeable). Empty statuses already fail **H30-3**. |
| Close #30 only after a later PR proves no new plant | Merge of #30 closes the tracker; waiting for the follow-up before merge never produces it. |
| Treat `#30` leftover plant, `#29`’s plant, or an empty-diff follow-up as S3 | Merge closes `#30` before leftover-complete. A dedicated **non-empty non-WIP** post-merge PR is the evidence. |
| Operator attestation instead of the dedicated plant-check PR | False-pass path. Attestation may record GETs of that PR; it is not a substitute. |
| Merge a successor without `Closes #30` | `#30` stays open. Body must contain `Closes #30` and the S3 leftover issue must already exist. |
| Drop the Woodpecker required context so an empty-CI repo can merge | Violates **H30-3**. Do not `force_merge`. Produce the context via **WP-pre**. Do not treat GitLab as a substitute. |
| Wait for a sister YAML-only ticket before S1 | No such voting issue exists. Blocking land on an unfiled ticket re-boxes implement. Allow the quoted bootstrap on the deletion PR instead. |
| Copy hello’s `.woodpecker.yaml` | Includes `coolify-deploy` and `telegram-failure` `from_secret`. Telegram runs on `status: [failure]` with no event filter (**H30-6**). |
| Copy dex’s `.woodpecker.yaml` | Calls `scripts/ci/gitleaks-scan-tracked.sh`, which does not exist here. |
| Port `.gitlab-ci.yml` rust / frontend / gitleaks into Woodpecker | Expands this chore into product CI. Cargo / npm stay ungated. Later CI extends the same root file. |
| Alpine `tree` binary / `apk add tree` | Stock Alpine has no `tree`. Org stubs use the `find` one-liner. |
| Add `.woodpecker.yml` or `.woodpecker/<other>.yaml` | Directory mode can ignore root YAML and post `ci/woodpecker/pr/<other>`; **H30-3** fails. |
| Treat missing context as silent forge ops / leftover | File owner ≠ posting owner, but land still blocks. Named unblock (cl8y-forgejo#48 or a local dex#1247-shaped leftover) plus retrigger. |
| Fake status POST or `force_merge` | Forbidden. |
| Treat a failing, pending, or hand-posted `ci/woodpecker/pr/woodpecker` as **WP-pre** | Land requires **present and success** from Woodpecker (creator/target `ci.cl8y.com`). |
| Merge `cac-design-issue-30` to `main` | That ref still has root `CODEOWNERS`; it is not a valid **H30-1** tip. Transport only. |
| Push a design SHA as the PR #30 tip | Design-only tip still plants reviews; reject. |
| Merge `eb352b8` | Incomplete land (delete-only); reject. |
| `Fixes` / `Closes` / `Resolves` for the leftover tracker on #30 | Merge closes the evidence issue; Decision 9 exists to prevent that. |
| Pass-iff matches `@code/maintainers` or only `requested_reviewers` | Misses live team `maintainers` / id 4; live #30 has empty user list. Use the fail-closed pair. |

## Complexity added / removed

**Removed:** catch-all official-review robot on every diff; operator dismiss
step; false “CODEOWNERS is the trusted-PR gate” story if later docs grow one.

**Added:** a small standing doc (this ADR + architecture **H30**) plus a
README pointer so later agents do not re-add `.* @code/maintainers` as a
merge requirement; a leftover checklist that survives merge of `#30`; a
**#30 land bootstrap** Woodpecker file on the deletion PR so **H30-3** can
post (this repo had none). No Coolify, no new services, jobs, flags, or
runtime surfaces. No `.gitignore` exceptions (`docs/` is already
trackable). Cargo / npm stay ungated. GitLab workflows stay but are
not the Forgejo merge gate.

## Migration

1. Protection is already migrated (forge #48 execute on this repo,
   GET `updated_at` 2026-09-21T07:29:30Z). Unauthenticated GET is 401.
   Re-GET at merge; do not PATCH.
2. Before merging the deletion PR, open a follow-up **leftover** issue that
   owns S3 (dedicated **non-empty non-WIP** plant-check PR). Merging
   [#30](https://git.cl8y.com/code/voting/pulls/30) closes that
   number; S3 must not live only there. If a successor is used, that
   leftover issue must already exist **and** the successor body must
   contain `Closes #30`.
3. Combine onto the existing product head, then merge **one** PR (see
   slices). Design branch `cac-design-issue-30` is **not** that PR. Today
   `chore/remove-catchall-codeowners` is delete-only @ `eb352b8`; a
   delete-only merge is not land (hello#15).
4. Open PRs created while the file existed (#30 and #29) may still show an
   official team `maintainers` request. Non-blocking under **H30-8**. No
   bulk dismiss required to close #30. Disqualified as leftover-complete
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
if any of those five IDs differ. Do **not** treat observed flags or
**H30-4** as GET-match fields. The 2026-09-21T07:29:30Z GET is
attested; unauthenticated GET is 401. Merge still re-reads for
drift. A green `find` clone-check or GitLab `test:rust` job does not satisfy
this read.

**Plant-check (dedicated post-merge PR, leftover-complete).**

Fail-closed pair (jq on the two GETs). Pass iff **all** of:

- `(requested_reviewers // []) | length == 0`
- `(requested_reviewers_teams // []) | length == 0`
- no review with `official == true && state == "REQUEST_REVIEW"`
  (user **or** team)
- PR GET `draft == false`; title does not contain `WIP` (case-insensitive);
  at least one changed file vs `main`

`maintainers` / `id: 4` is the known-plant example (live #30), not
the only fail. Do not require `team.organization` on the reviews GET.
Do not match `team.name == "code/maintainers"` or the CODEOWNERS token
`@code/maintainers` (those miss the live plant).

`official` alone means assigned/write-access, not “planted by
CODEOWNERS”; the conjunction with `state == "REQUEST_REVIEW"` is the
plant signal. `REQUEST_REVIEW` is `state`, not a sibling key.

Known-plant sample: PR #30 (must fail this predicate; not leftover
evidence). Live user list is empty (`requested_reviewers: []`); the
team plant is in `requested_reviewers_teams`. Checking only teams
would ignore a user plant; checking only `requested_reviewers` would
pass #30.

`GET /api/v1/repos/code/voting/pulls/30` fragment:

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

`{n}` is the dedicated plant-check PR opened after the delete is on
`main`. GET both endpoints immediately after open. If any plant signal
is present, fail. If all clauses hold, wait once 30 seconds and re-GET
both before pass; pass only if the second pair still satisfies all
clauses. Record `{n}` and the two JSON bodies (the pair used for the
pass decision) plus the four-path check on `main` on the leftover
tracker, then close that **issue** without merge. PR #30’s own official
request does not pass. PR #29’s plant does not pass. “The next
natural PR” does not pass.

**CI (land prerequisite).** Woodpecker context `ci/woodpecker/pr/woodpecker`
must be **present and success** on the **combined** product-PR head
(**WP-pre**), created by Woodpecker (creator/target `ci.cl8y.com`), not a
manual Forgejo POST. Pending, failure, error, or a hand-posted status is
not **WP-pre**. Drain comments such as `drain skip: no occupying job…` are
**#429** / CAC, not a #30 failure. Empty statuses on `1006f8b` / `eb352b8`
are why **WP-pre** is in this slice. Confirm Woodpecker project enablement
**before** combining (Decision 8). If autoland waits on #429, an operator
still SHA-pins `Do: merge` (**H30-4**).

## Failure modes

| Mode | Handling |
| --- | --- |
| File deleted on a branch but still on `main` | New non-WIP PRs that change a file keep planting official review until the product PR merges. Expected until land. Occupying #30 already demonstrates this. |
| `chore/remove-catchall-codeowners` stays delete-only @ `eb352b8` | `#30` must not merge. Convert to draft; push S2 + WP-pre onto that head. |
| Live #30 stays `draft: false` after a **delete-only** head is pushed | hello#15 trap: 1 file, `mergeable: true`. Convert to draft **before** pushing a combined head so S1-without-S2 cannot close `#30` if a context later posts. |
| Copy left in `docs/`, `.gitea/`, or `.forgejo/` (including empty/comments-only) | Forgejo still loads the first existing path and may plant. Land fails **H30-1**; delete those paths too (none exist on current `main`). Standing `docs/adr/` and `docs/ARCHITECTURE.md` are not CODEOWNERS. |
| S1 (delete-only) merges without S2 / WP-pre | **H30-1** true, README/ADR/architecture / posting context absent from `main`. Forbidden. Empty statuses currently also fail **H30-3**. |
| Successor merges without `Closes #30` | `#30` stays open. Forbidden as the land vehicle. |
| cl8y-forgejo migrate/apply re-copies a template | Sister-repo race ([cl8y-forgejo#48](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/issues/48) `_ensure_codeowners`). Out of this slice. If a later apply re-adds the file, delete again via PR; never direct-push `main`. |
| Official request leftover on #30 or #29 | Non-blocking (**H30-8**). Optional human dismiss. Not a rollback signal. Not leftover-complete evidence. |
| Treating merge of `#30` as leftover-complete | Merge closes `#30` before S3. Use the leftover issue / checklist. |
| Operator attestation without the dedicated PR | False-pass. Fail S3; open the dedicated non-empty non-WIP PR. Attestation may only record its GETs. |
| Plant-check PR has empty diff | False-pass: Forgejo does not plant on a no-op even if `CODEOWNERS` remains. Fail S3; open a new dedicated PR that changes at least one file. |
| WIP/draft follow-up used as “no new plant” | Forgejo skips CODEOWNERS on WIP. Invalid. Use a non-WIP PR into `main`. |
| Pass-iff ignores `requested_reviewers` | User plant would pass; live #30 has `[]` so teams-only looks green while Tests say fail if users were requested. |
| Pass-iff matches `@code/maintainers` | Misses live team `maintainers` / `id: 4`; use the fail-closed pair. |
| `Fixes` / `Closes` / `Resolves` for the leftover tracker on #30 | Merge closes the evidence issue; Decision 9 forbids those keywords. |
| Merging `eb352b8` | Land without ADR/README/WP-pre; reject. |
| Pushing a design SHA as the PR tip | Design-only tip still plants reviews; reject. |
| `ci/woodpecker/pr/woodpecker` never posts | **Land blocked** (**WP-pre** / land criterion 3). Do not `force_merge`; do not drop **H30-3**; do not POST a fake status. Diagnose: missing YAML, wrong path (not root `.woodpecker.yaml`), or repo not enabled on Woodpecker. File: `#30` implement. If YAML is on the PR and nothing posts: named forge unblock (cl8y-forgejo#48 or a local dex#1247-shaped leftover) **plus retrigger**. |
| Context present but not success, or hand-posted | **WP-pre** incomplete. A failing/pending Woodpecker run or a manual Forgejo POST (creator/target not `ci.cl8y.com`) does not land. Re-run or repair; do not `force_merge`. |
| `.woodpecker.yml` or `.woodpecker/<other>.yaml` added | Wrong or ignored context; **H30-3** fails. |
| GitLab treated as **H30-3** | Those workflows do not post `ci/woodpecker/pr/woodpecker`. Forgejo `has_actions=false`. Keep them; do not drop Woodpecker. |
| Protection silently reverted to official-review true | Merge 405 returns. Out of this repo; re-apply via forge policy, do not `force_merge`. Not proven by a green `find` step. |
| `enable_push` flipped true | **H30-2** regression. Refuse. |
| Re-adding CODEOWNERS “for safety” in a follow-up | Violates **H30-1**. Reviewers must reject unless a new ADR allowlists path owners. |
| CAC autoland waits on #429 | Operator `Do: merge` still closes #30. Do not implement occupying-job cleanup here. |
| `block_on_outdated_branch` blocks merge | Rebase the product head onto current `main`. Do not PATCH the flag off. |
| Implement PATCHes protection or edits CAC / Coolify / ledger / frontend | Out of authority / wrong repo. |
| `git grep '^\.\* @'` as the land check | After a correct delete, that command exits **1** (`set -e` false-fail). A comments-only leftover is a grep pass. Land predicate is `test ! -e` on all four paths. |

## Ordered implementation slices

| Slice | Work | Mergeable? | Depends on |
| --- | --- | --- | --- |
| **S0** | This design on `cac-design-issue-30` (ADR 0001 + architecture **H30**). Transport only. | No. Do not open or merge as a product PR. | None in `code/voting`. Woodpecker enablement is a **WP-enable / combine** gate, not an S0 file dep. |
| **S1** | Delete root `CODEOWNERS` on a branch that differs from `main`. Confirm the other three paths are absent. Live head `eb352b8` on `chore/remove-catchall-codeowners` is that delete. | **Draft-only** until S2+WP-pre are on the same branch. Not a landable slice. Convert live #30 to `draft` **before** pushing a combined head (lock against a later delete-only tip). | S0 accepted. |
| **S2** | Same branch as S1: standing docs + README pointer (Decision 3 body). Add the quoted root `.woodpecker.yaml`. No ledger / operator-voting / frontend / deploy / skill / OPS edits. Open the leftover issue that will own S3. | Only as the **combined** product PR with S1+WP-pre. | S1 on the same head. |
| **WP-enable** | **Pre-merge gate.** Confirm `code/voting` is an active Woodpecker project **before** combining onto `#30`. File owner ≠ posting owner. If YAML later posts nothing: named unblock (cl8y-forgejo#48 or a local dex#1247-shaped leftover) plus retrigger. Fake status POSTs and `force_merge` forbidden. | N/A. Blocks combine / WP-pre. | Not a local iid. Named forge ops. |
| **WP-pre** | **Land prerequisite (not leftover).** Deletion-PR head has a real **success** `ci/woodpecker/pr/woodpecker` from Woodpecker (`ci.cl8y.com`). File owner: `#30` implement (quoted YAML). Enablement owner: forge ops. Blocks merge of S1+S2. | N/A as a solo merge. | WP-enable; S2 file on the PR. |
| **Product PR** | **One** PR: default [#30](https://git.cl8y.com/code/voting/pulls/30) (`chore/remove-catchall-codeowners`) whose `git diff origin/main...HEAD` is delete + ADR 0001 + architecture + README pointer + root `.woodpecker.yaml`. Merging this PR **closes #30**. Successor only with `Closes #30` in the body **and** S3 leftover issue already open. | Yes, once S1+S2+WP-pre are on the head, rebased, Woodpecker green. | S0 accepted; S1+S2+WP-enable+WP-pre combined. |
| **S3** | Leftover-complete “no new plant” (tests item 4). Dedicated post-merge **non-WIP non-empty** plant-check PR. Operator attestation may record that PR’s GETs; it is not a substitute. Not a close gate for #30. | N/A | Product PR merged to `main`. |

PR `#30` (or `Closes #30` successor) ships **S1+S2+WP-pre** only.

Sister repos (not slices of #30, not local `DEPS`): forge #48
protection+templates+Woodpecker project repair; CAC #429 autoland occupying
job; hello#15 open canary; dex#1309 landed pattern; dex#1247 enablement
shape; voting #29 Renovate.

### Combine step (S1+S2+WP-pre onto the product vehicle)

Do this on `chore/remove-catchall-codeowners`, not on `cac-design-issue-30`.
**WP-enable first:** confirm this repo is a Woodpecker project before the
pushes below.

1. Mark [#30](https://git.cl8y.com/code/voting/pulls/30) **draft** so a
   later **delete-only** head cannot land (hello#15: 1 file,
   `mergeable: true`). Live `#30` is already that trap; draft is the lock.
2. Fetch published design: `origin/cac-design-issue-30` (this ADR,
   `docs/ARCHITECTURE.md`).
3. Copy or cherry-pick those S0 paths onto `chore/remove-catchall-codeowners`
   (keep the CODEOWNERS delete from `eb352b8`). Never reset the branch to a
   design SHA. Never merge `eb352b8`.
4. Add the README **Merge gate** paragraph as the Decision 3 body (do not
   stub the product README). Index ADR 0001 / **H30** in Docs for agents.
5. Add **exactly** the quoted root `.woodpecker.yaml` (Decision 8 /
   architecture **Pipeline**). No `.yml`, no `.woodpecker/`, no hello/dex
   product steps, no GitLab rust/frontend port.
6. Open the leftover issue that owns S3 (Decision 9 paste). Required
   **before** merge (tests item 6 / land criterion 6). If a successor is
   used instead of `#30`, that issue must already exist and the successor
   body must contain `Closes #30`. #30 must not contain close keywords for
   that leftover number.
7. Rebase onto current `origin/main` (`block_on_outdated_branch` is
   observed-true; do not PATCH it off). Push the product branch.
8. Ready the PR only when `git diff origin/main...HEAD` is the product
   diff and `ci/woodpecker/pr/woodpecker` has posted **success**. Then merge
   with SHA-pinned `Do: merge`. If YAML is present and nothing posts:
   named unblock plus retrigger; do not fake a status.

## Tests

In-repo CI cannot GET branch protection. Existing Cargo tests, frontend
`npm test`, Playwright, SPA fallback, and GitLab gitleaks stay ungated
and must not be rewritten for this ticket. The **#30 land bootstrap** does
not run those suites. Any later real Forgejo CI extends the same root
`.woodpecker.yaml`. Do not add a product test solely for file absence.

1. **Absence** (on the product tip / after land). Scope to the four Forgejo
   CODEOWNERS paths; do not `git grep` the whole tree (this ADR quotes
   `.* @code/maintainers`). Land predicate is four-path absence:

   ```
   test ! -e CODEOWNERS
   test ! -e docs/CODEOWNERS
   test ! -e .gitea/CODEOWNERS
   test ! -e .forgejo/CODEOWNERS
   ```

   After a correct delete, uninverted `git grep -nE '^\.\* @' -- CODEOWNERS
   docs/CODEOWNERS .gitea/CODEOWNERS .forgejo/CODEOWNERS` exits **1**. If a
   grep stays as a secondary scan, invert it so `set -e` does not
   false-fail:

   ```
   ! git grep -nE '^\.\*\s+@\S+' -- CODEOWNERS docs/CODEOWNERS .gitea/CODEOWNERS .forgejo/CODEOWNERS
   ```

   A comments-only leftover is a grep pass — grep is not the land
   predicate.

2. **PR pipeline (land / WP-pre).** Root `.woodpecker.yaml` on the combined
   head is the Decision 8 / architecture **Pipeline** file (`steps:`,
   two-item `when:`, digest-pinned alpine, `find` one-liner, no secrets).
   Woodpecker context `ci/woodpecker/pr/woodpecker` is **present and
   success** on that head, created by Woodpecker (creator/target
   `ci.cl8y.com`, not a manual Forgejo POST), and is the merge check.
   Cargo / GitLab gitleaks stay ungated.

3. **Protection (operator read).** Same list as close criterion 4: GET
   `main` still equals **H30-2**, **H30-3**, **H30-5**, **H30-8**, **H30-9**.
   Fail if `enable_push` is true, official-review block is true,
   `required_approvals` is not 0, reject-block is false, or the Woodpecker
   context is missing. Do **not** treat observed flags or **H30-4** as
   GET-match fields. Prior timestamp is attested; this test is
   a close re-GET. Unauthenticated GET is 401.

4. **No new plant** (leftover-complete; **not** a #30 close gate). After
   S1+S2+WP-pre are on `main`:

   - Open a **non-WIP** PR **into `main`** from a tip that does **not**
     contain `CODEOWNERS` and that **changes at least one file**.
   - Do not request users or teams in the UI.
   - `GET /api/v1/repos/code/voting/pulls/{n}` and
     `GET .../pulls/{n}/reviews` immediately after open, then wait 30s
     and re-GET.
   - Pass = Observability fail-closed pair on both GET pairs.
   - **Disqualify** leftover `pulls/30` and leftover `pulls/29`. A
     draft/WIP or empty-diff follow-up is not proof.
   - Operator attestation may record those GETs; it is not a substitute
     for this PR.

5. **Reject still blocks (doc-level).** Do not turn off
   `block_on_rejected_reviews` to “make autoland easier.”

6. **Leftover issue exists before merge** (same as land criterion 6). The
   S3-owning follow-up issue is open before SHA-pinned `Do: merge` of `#30`
   or a `Closes #30` successor. Merging `#30` closes that number; S3 must
   not live only there. Close-keyword contract: PR #30 / issue #30 text
   has no `Fixes` / `Closes` / `Resolves` for the leftover tracker iid.

7. **Product untouched.** `git diff origin/main...HEAD` on the implement PR
   has no `ledger/` / `operator-voting/` / `frontend/` / `deploy/` /
   `skills/` / `.gitlab-ci.yml` / `.gitleaks.toml` / `.githooks/` /
   `docs/LEDGER_INVARIANTS.md` / `docs/OPERATOR_VOTING.md` /
   `docs/FRONTEND.md` / `docs/OPS.md` / `docs/HANDOFF.md` edits.

## Rollout

- Merge vehicle: **one** PR, existing
  [#30](https://git.cl8y.com/code/voting/pulls/30), after the combine
  step. Diff must be delete + ADR 0001 + architecture + README pointer
  (Decision 3) + root `.woodpecker.yaml` (Decision 8). Design-only
  `cac-design-issue-30` must not be opened or merged as the product PR.
  Successor only with `Closes #30` in the body and the S3 leftover issue
  already open.
- Order: protection already live (re-GET at close) → **WP-enable**
  preflight → convert #30 to draft → combine S1+S2+WP-pre on
  `chore/remove-catchall-codeowners` → leftover issue remains open →
  rebase → Woodpecker **success** on the combined head (or named unblock
  plus retrigger) → `Do: merge` → **#30 closes**. Then leftover-complete
  test 4 on a dedicated post-merge non-WIP non-empty plant-check PR.
  Operator attestation may record that PR’s GETs; it is not
  leftover-complete.
- Other `code/*` catch-all deletions may copy this pattern; this ADR does
  not merge those repos. Repos that already post the context (hello#15,
  dex#1309) do not need a new YAML on the delete PR; this tree does. Do
  not copy hello or dex YAML contents.
- [#297](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/297):
  no deploy, spend, custody, or CAC policy expansion. Landing #30 does not
  authorize Coolify rebuild, Legal admin, wallet/ledger changes, or
  autonomy edits.

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

### Land (S1+S2+WP-pre) — merge of PR `#30` or `Closes #30` successor

All must be true on the merged tip. This is what merging `#30` completes. It
does **not** wait for S3.

1. `main` has no CODEOWNERS file at the four Forgejo paths: `test -f` fails
   on `CODEOWNERS`, `docs/CODEOWNERS`, `.gitea/CODEOWNERS`,
   `.forgejo/CODEOWNERS` (**H30-1**).
2. Standing docs on `main`: ADR 0001 + architecture **H30**, **and** root
   README pointer (Decision 3 body; product tables kept). Architecture-only
   is not sufficient.
3. The deletion-PR head has a real **success** `ci/woodpecker/pr/woodpecker`
   status from Woodpecker / `ci.cl8y.com` (**WP-pre** / **H30-3**); no
   deploy secrets ran on that PR event; mergeable under **H30-4**
   (SHA-pinned `Do: merge`). A failing or hand-posted status is not this
   criterion.
4. Protection GET still matches **H30-2**, **H30-3**, **H30-5**, **H30-8**,
   **H30-9** (same list as test 3; operator re-read at merge; not inferred
   from a green `find` step; not **H30-4**; not observed flags; prior
   `updated_at` was attested).
5. No `force_merge`, no direct `main`, no CAC dismiss-as-merge, no
   Coolify/HMAC/`autonomy.rs`/ledger/frontend/deploy edits in
   the #30 diff.
6. A leftover issue exists that owns S3 (because merging `#30` closes that
   number). Same as tests item 6. Required before merge, including on a
   successor. #30 has no close keywords for that leftover number.

A delete-only `#30` is not land. Missing Woodpecker (file **or** enablement)
is not land. A successor without `Closes #30` is not land of `#30`.

### Leftover-complete (S3) — follow-up issue; survives merge of `#30`

1. A **dedicated** PR opened **after** the delete landed, **non-WIP**, with
   a **non-empty diff** (at least one file changed vs `main`): Observability
   three fail-closed clauses. Record `{n}`, the two JSON bodies used for
   the pass decision, and four-path `test ! -e` on `main`, then close the
   leftover **issue** (issue close, not via merge). `#30`’s leftover plant
   does not count. `#29`’s plant does not count. “The next natural PR”
   does not count. An empty-diff or WIP dedicated PR does not count.
   Operator attestation of those GETs is not leftover-complete without
   this PR.

Forgejo#48 leftovers, CAC#429, and voting #29 (Renovate) may stay open;
they are not land or leftover gates for this tree. Enablement/repair
needed so **WP-pre** can post **is** a land gate (owned above), not a
leftover.

## Authority

[cl8y-agent-control#297](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/297):
this design does not grant deploy, spend, custody, or agent-permission
expansion. Relaxing official-review as a **forge merge gate** is already
executed on this repo by #48 (`updated_at` 2026-09-21T07:29:30Z); this ADR
only removes the in-tree file that plants requests and adds the minimal
pipeline so **H30-3** can post. Independent review of this proposal is a
later gate. This document is not architecture approval.
