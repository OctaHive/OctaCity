# Task 2.12 verification — Console Properties and related tasks

## Scope

The current task card now reads and edits optional organizational assignee,
Project labels, due date and configured text/choice/boolean/integer/date values.
Type, component and size are ordinary installed definitions, with no provider or
execution semantics. The existing right-side/narrow Properties surface and shared
SelectMenu/confirmation/query-state components are reused; no dependency or new
application framework was added. Test-only card/router setup was extracted under
`ui/tests/fixtures/`, preserving the existing requirements/card tests.

Unassigned remains usable without a directory, unavailable retained responsibility
is identified without leaking its ID, and source-owned controls explain their
ownership. Exact retained catalogs resolve saved names. Retired selections can
remain or be removed, but cannot be newly selected. Changed values follow current
bounds; unchanged values survive tighter definitions. Selected-label counts derive
from OpenAPI rather than a duplicated constant.

Parent selection reuses authorized bounded Project task discovery, excludes self
and retains a human label across pages. Parent and direct-child links have their
own loading/error/stale states and bounded child pagination. Visibility/cycle
checks remain server authority. These links grant no rights, roll-up or execution.

Drafts and confirmed attempts stay in mounted memory. Task/catalog/directory/policy
changes block a new confirmation until explicit reload/discard and fresh review.
Indeterminate retries retain the original body, positive Issue ETag and key even
after refresh. Planning uses only its dedicated PATCH; it does not edit content,
move workflow/status, change execution inputs or invoke Factory/provider APIs.
English/Russian browser fixtures preserve historical selection, keyboard focus,
progressive technical IDs and narrow layout, including open shared menus.

Only UI, generated-bound tooling, reference documentation and this task's
verification were changed for 2.12. Existing uncommitted 2.9–2.11/backend and
Docker-context work was preserved. No commit was created.

## TDD evidence

Commands were run from the repository using pinned pnpm 12.8.1 and the bundled
Node 24 runtime. `pnpm` below abbreviates that executable. Existing tests passed
before extracting shared fixtures: 26 IssuesView/API scenarios; the 25 existing
card scenarios passed again after extraction.

The primary behavioral boundary was the real App/router/query-client card with
injected typed APIs, followed by Playwright against actual generated HTTP calls.
Each behavior below was specified and observed failing before its implementation.

| Command | Observed Red | Observed Green |
| --- | --- | --- |
| `pnpm --dir ui exec vitest run src/features/issues/IssueProperties.test.tsx` | No Unassigned/optional Properties presentation | Manual/unconfigured card presents optional values and directory explanation |
| Same command, planning edit slice | No Edit properties action | Explicit reviewed planning-only PATCH with exact task ETag, catalog version and key |
| Same command, typed values slice | Project Task type control absent | Configured kinds, false vs absent, explicit clearing and UTF-8 byte bounds |
| Same command, retirement slice | Retirement explanation absent | Retained selections preserved; retired new selections excluded; missing-directory assignment can be cleared |
| Same command, stale/ownership slice | Ownership explanation absent; stale confirmation still enabled | Managed controls explained/disabled; changed definitions require explicit fresh review |
| Same command, retained-definition slice | Original property name absent | Saved version 6 resolves its own catalog while editing current version 7 |
| Same command, relationship slice | Visible parent link absent | Human parent/child navigation and bounded replacement pages |
| Same command, parent edit slice | Parent selector absent | Authorized paged selection excludes self and reviews the selected human label |
| Same command, retained read/review slice | Saved-catalog refresh failure invisible; unavailable assignee called Unassigned in review | Readable stale data retained, new confirmation disabled, honest assignment and exact version review |
| Same command, numeric retention slice | Native form rejected an unchanged retained number above the tightened maximum | Unchanged number retained; changed out-of-range number blocked; valid change saved |
| Same command, requirements-only ownership slice | Empty planning ownership claim appeared for source-owned Title | Only planning ownership produces a planning ownership explanation |
| `pnpm --dir ui exec node --test scripts/issue-constraints.test.mjs` | Selected-label maximum undefined instead of the supplied schema value 2 | Generated selected-label bound follows OpenAPI |
| `pnpm --dir ui exec playwright test tests/browser/issue-properties.spec.ts` | Opening the editor left focus on its trigger | Initial input focus, modal keyboard loop, Escape/close restoration and HTTP body/header assertions in English/desktop and Russian/390px |
| Same browser command, checkbox layout slice | Checkbox occupied the full label width | Intrinsic checkbox with a usable full label target, no horizontal overflow |
| `pnpm --dir ui build` and shared-menu component scenarios | Architecture rejected native selects and production-classified test support | Shared product controls and test-only fixture location; behavior preserved |
| `pnpm --dir ui exec node --test scripts/verify-architecture.test.mjs` | Measured Properties workspace exceeded previous total JS/static ceilings | Measured workspace admitted; additional 5,000 bytes still rejected |

Additional regression cases cover task, directory and ownership changes separately,
frozen retry after task/catalog refresh, unavailable/forbidden planning and child
reads without disabling requirements editing, inaccessible parent names/IDs,
navigation discarding drafts, and no persistence/provider/Factory commands.
Selector, fixture, TypeScript and formatting corrections were not counted as
behavior Red. No unavailable environment was used as Red evidence.

## Final checks

```sh
pnpm --dir ui api:generate
# Generated planning contracts and label counts from the current server OpenAPI.
pnpm --dir ui test:unit
# 31 Node tests and 522 Vitest tests passed (78 files).
pnpm --dir ui exec playwright test tests/browser/issues.spec.ts \
  tests/browser/issue-properties.spec.ts tests/browser/accessibility.spec.ts
# 22 passed, including 2 new Properties HTTP/keyboard/localization scenarios.
pnpm --dir ui lint
pnpm --dir ui format
pnpm --dir ui build
# TypeScript, production build, frontend architecture and bundle budgets passed.
python3 tools/check_architecture.py
# 63 packages / 256 dependency edges passed.
openspec validate add-factory-work-card --strict
git diff --check
```

The measured build contains 1,017,267 JavaScript bytes, 85,392 CSS bytes,
1,103,205 total static bytes and 49 files. The optional feature adds approximately
20 KiB of JavaScript compared with the prior task workspace. Total JS and static
ceilings each increased by 20 KiB, to 1,019,904 and 1,105,920 respectively, only
after the measured-fixture Red/Green check. Entry JS (460,800), CSS (86,016) and
asset count (49) ceilings remain unchanged. Actual entry JS is 430,767 bytes.
The existing guard still rejects a further 5,000-byte addition.

OpenAPI generation reused the existing Rust target with
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`. No new backend
behavior required Rust/PostgreSQL regression or container image builds in this
UI task; the prior 2.11 report retains that verification. Full workspace and full
browser suites were not rerun. Space was checked before/during builds: initially
6.9 GiB, up to 7.6 GiB during work and 5.4 GiB at the last check. User target
artifacts, containers, volumes and credentials were not removed. Documentation-only
updates were checked with OpenSpec and diff validation; executable TDD does not
apply to those prose edits.

## Review follow-up

The three subsequent review findings are fixed and covered by additional public
card regressions. See [review-fix verification](verification-2.12-review-fixes.md)
for Red/Green evidence and the updated final checks.
