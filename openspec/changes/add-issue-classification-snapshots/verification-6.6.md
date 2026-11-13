# Task 6.6: Project Issue field administration

Subsequent execution review corrections and shared regression results are recorded
in [the 6.5–6.6 review verification](verification-6.5-6.6-review.md).

## Behavior and design

Project settings compose the Issue-owned field editor through an app-level
render slot. Project feature internals do not import the Issue feature. Opening
settings reads the existing Project choice/catalog API; collapsing retains the
mounted draft. Publication uses the existing full-catalog command under separate
Project Administer authority. No permission, database, endpoint or provider
integration was added.

Separate editors administer built-in priority/component/type choices. Omitted
priority/type lists use generated domain defaults; explicitly empty lists stay
empty. Fixed built-in size choices are displayed and remain in Issue editing.
New fields support text/choice/boolean/integer/date using server-derived bounds.
Field IDs are allocated on addition, not render or review, and stay fixed.
Options normally allocate an ID; an explicit optional ID can retain a known
semantic key independently of a display name. Existing identities and field
kinds cannot be removed/converted. Display names and retirement flags are editable.
Ordinary custom properties and optional distinct choice-field semantic bindings
stay separate from built-ins and confer no evidence or execution authority.

The editor performs advisory bounded validation and reviews complete proposed
metadata, kinds, bounds, semantic IDs and retirement. All original server checks
remain authoritative. A stale refresh retains the local draft. Three-way
reconciliation is scoped to built-in list/property/semantic identities. Independent
current changes survive; overlapping changes require an explicit local/current
decision. A resolution is bound to the current catalog version and local draft.
Retaining a local field edit cannot drop concurrently added immutable choice IDs.
Current labels survive reconciliation, and full publication preserves unrelated
catalog metadata. No Issue value, admitted Work/Run or earlier snapshot changes.

The existing confirmed command owns lost-response replay. A conflict closes into
fresh current metadata, explicit reconciliation and another review/new key.
Denied publication retains editable input. Explicit discard resets both catalog
changes and unadded child input buffers; collapsing does not reset either.

## Public-boundary TDD evidence

Tests use the existing routed Project workspace/Issue form and HTTP API boundary,
with API fixtures replacing the server. No private helper is the behavioral test
boundary. Every executable slice below observed the stated missing behavior before
its implementation. Existing capability contracts were reused for qualification.

From `ui`, `pnpm exec vitest run src/features/issues/ProjectIssueFields.test.tsx`
was run after each slice (a `-t` filter selected the new case when appropriate):

1. Project entry/built-ins: Red lacked Issue field settings; Green reviewed and
   published a separate component list while retaining standard priority/type IDs.
2. Ordinary fields: Red lacked the custom field group; Green published bounded
   text through typed controls without an Issue edit.
3. Kinds/semantics: Red lacked the Risk field selector; Green published choice,
   boolean, integer and date fields, with one explicit choice-field binding and
   reviewed option names/bounds.
4. Invalid contracts: Red enabled review for zero text bytes, reversed integer
   bounds and an empty choice set; Green blocked all three with field diagnostics.
5. Stale publication: Red had no reconciliation action after refresh; Green kept
   the local addition and another administrator's new field/current component,
   then reviewed the current version with a new key.
6. Overlap: Red had no explicit scoped conflict selector; Green required the
   operator's decision before adopting the newer version.
7. Discard: Red had no explicit discard action; Green kept collapsed drafts and
   cleared them only on discard, without publication.
8. Choice retention: Red lost a concurrently added choice when keeping a local
   field rename; Green retained both immutable choice IDs in the new review.
9. Semantic ID: Red could not enter an explicit option ID; Green published the
   display name `Неизвестно` with ID `unknown` and a reproducibility binding.
10. Unadded buffers: Red left pending option/field text after explicit discard;
    Green reset the child buffers while retaining the published catalog.

The same suite characterizes compatible rename/retirement and independent
administration denial through the public UI. Role queries distinguish field
retirement from nested option retirement. No request body/key or pending text
is persisted in browser storage.

`node --test ui/scripts/issue-constraints.test.mjs` observed Red because the
catalog administration bounds were absent, then Green using authoritative
OpenAPI field/count/text/integer/catalog limits, kind tags and semantic slots.
Generation preserves the separate optional built-in defaults.

Browser setup initially hit sandbox `EPERM` on local port 4173; running the
local test server with permission restored the environment (not a Red result).
The first completed scenario needed corrected test waits and the actual EN/RU
selector labels; these fixture/locator corrections did not change product
behavior. The final scenario covers Project settings and Issue value editing
through generated HTTP transport in EN/RU at 390px, keyboard modal focus,
concurrent publication, replay after a lost acknowledgement, Project defaults,
all supported kinds, exact catalog version and no implicit Issue mutation.

## Bundle qualification

Before budget adjustment, production build measured 1,594,093 JavaScript bytes,
1,682,646 total static bytes and 59 assets against the prior 1,581,056/1,671,168/57
ceilings. The new settings entry and shared lazy modules account for the two
additional assets. No dependencies were added and the entry/CSS ceilings remain
unchanged. Aggregate JS/static budgets each increase by 24 KiB to
1,605,632/1,695,744 bytes; asset count increases to 59.

`node --test ui/scripts/verify-architecture.test.mjs` observed Red for the
production bundle and new allowed fixture, then Green after the narrow budget
change. Its independent near-ceiling fixture still rejects an extra KiB for
both aggregate ceilings. This retains a tested ceiling rather than removing
bundle enforcement.

## Final verification

From `ui`:

```sh
CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true pnpm api:generate
pnpm typecheck
pnpm test:unit
pnpm exec vitest run src/features/issues/ProjectIssueFields.test.tsx
pnpm build
pnpm lint
pnpm format
pnpm exec playwright test tests/browser/project-issue-fields.spec.ts tests/browser/issue-properties.spec.ts tests/browser/issue-classification.spec.ts tests/browser/issue-execution-snapshots.spec.ts
```

All completed successfully. The full suite passed **45 script tests and 1183
Vitest tests in 137 files**, including **14 Project field editor tests**. A final
focused editor run passed all 14 after removing unused test handles; this changed
no product code. Browser regression passed **9 tests**, including both new
Project field scenarios in EN/RU at 390px, keyboard review focus, settings-page
and Issue-page overflow checks, conflict/replay and existing Properties,
classification and frozen-snapshot journeys. Production build passed TypeScript,
frontend architecture and bundle enforcement. The final lint/format runs passed.
API generation passed and generated schema/authorization inventory stay coherent.

From the repository root:

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest --test v1_contract --test v1_handlers
cargo fmt --all --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

REST contract/handler suites passed **33 + 273 = 306 tests**, including the
existing public test separating Project definition administration from Issue
editing. Rust formatting, workspace architecture (64 packages, 263 edges),
strict OpenSpec validation and whitespace checks passed. No Rust behavior or
persistence schema changed for task 6.6.

Documentation-only edits have no executable behavior for Red/Green; their local
verification links, OpenSpec validation and whitespace were checked. No native
executor, packaged release or real provider effect was performed or claimed;
those remain later end-to-end tasks. No commit was requested. Pre-existing task
6.5 changes are retained, and `github.token` remains untouched and untracked.
