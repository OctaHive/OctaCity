# Task 6.4 review corrections

## Corrected behavior

1. The snapshot form recognizes an editable binding through its exact Incoming
   reference, empty source path, matching selected field shapes and generated
   mapping. One matching source alone no longer establishes ownership. Renamed
   targets, extra constant fields, custom conditional fallbacks and nested source
   paths remain unchanged and block incompatible automatic editing/removal.
   Generated presence guards remain editable after the server sorts schema
   fields: the recognizer recovers guard order from the all-present branch and
   verifies the complete mapping rather than treating field order as semantics.
2. Snapshot and typed classification authoring reuse equivalent registered data
   schemas. Shared registration deduplicates exact references and retires only
   replaced inputs that no remaining draft contract references. Unrelated saved
   schemas and contracts still used by outcomes, inputs, profiles or other draft
   declarations remain registered. Cleanup happens after root/entry input
   synchronization, so the previous root pointer cannot retain a dead contract.
   Reachability includes exact transitive pinned subflows. If an exact child
   definition is unavailable, cleanup retains candidate schemas conservatively.
3. Both classification editors share the same exact producer check: node,
   outcome and full schema reference. Typed compilation also fences writer
   choices, matching the legacy guard. Producer, outcome or writer changes while
   normalization is pending discard the captured action instead of installing it.
4. Publication input review composes Incoming source paths with mapping paths.
   Empty mapping pointers include the appropriate Incoming members of the whole
   source map; they do not hide the consumer. Selecting a nested Incoming field
   does not expand it to sibling fields or materials.
   Presence-only conditions are shown separately from forwarded field values.

## Follow-up review corrections

The second review's three cases are covered at the same public editor/review
boundaries:

- Schema cleanup traverses exact pinned child definitions, including nested
  closures supplied by the existing explorer. It does not confuse another
  version with the selected publication. Unknown closure reachability preserves
  schemas; known closures retain their internal inputs/results while unused
  replaced root contracts can still be retired.
- Publication review records `if_present` condition paths as presence reads.
  Conditions that do not forward the raw value are labelled separately; a value
  already forwarded covers the corresponding presence dependency. Incoming
  source paths compose with these condition paths just as with value pointers.
- Unavailable selections remain visible by input alias and source identity.
  Explicit removal clears the selection and its per-step projections, after
  which the author can normalize and review the current catalog version. Missing
  fields are never silently discarded.

Related round-trip tests also caught two prerequisites for reliable recovery:
source identity must ignore JSON object property order, and opening a published
Flow must retain `issue_snapshot` in its draft. Both corrections have independent
Red/Green evidence. Source identity uses the discriminant plus property ID/slot;
published input metadata and bindings are preserved exactly during loading.

| Follow-up cycle | Observed Red | Observed Green |
| --- | --- | --- |
| Pinned closure retention | Direct/nested/missing/wrong-version child cases lost a live shared schema | Four cases retain it; complete closures still retire an unused root-only contract |
| Presence review | Three presence-only/mixed mappings omitted their source dependency | Whole and nested sources show explicit presence checks; mixed inputs distinguish values and checks |
| Unavailable field recovery | Retired property and removed semantic binding were invisible | Both show their source and can be explicitly removed without affecting the component selection |
| Source identity | Reordered JSON keys made an available property appear unselected | The property remains selected, available and applicable |
| Published Flow loading | The snapshot-removal control and draft metadata were absent | Opening/validating retains the exact snapshot and its existing input binding |

```sh
cd ui
pnpm exec vitest run src/features/factory/FlowSchemaRetention.test.tsx
pnpm exec vitest run src/features/factory/IssueInputReview.test.tsx \
  -t 'distinguishes snapshot presence'
pnpm exec vitest run src/features/factory/IssueSnapshotEditing.test.tsx \
  -t 'unavailable snapshot selection'
pnpm exec vitest run src/features/factory/IssueSnapshotEditing.test.tsx \
  -t 'JSON property order'
pnpm exec vitest run src/features/factory/FactoryAuthoring.test.tsx \
  -t 'retains a published Issue snapshot'
```

Each command failed behaviorally before its fix. The local log pairs are
`/tmp/octacity-64-fix2-{subflow,presence,unavailable,source-identity,reopen}-{red,green}.log`.
Focused Green counts were **11**, **43**, **19**, **29**, and **1** respectively;
these runs also include overlapping existing regressions where selected.

Four additional EN/RU 390 px browser scenarios qualify keyboard removal of a
retired field and review of a published presence-only snapshot. Their HTTP
validation fixture preserves the snapshot metadata returned by the actual API.

### Renamed consumer with an open snapshot form

A subsequent public-editor regression covers one and two existing consumers.
Opening snapshot settings, renaming a step elsewhere in the graph and applying
the old form previously erased that step's input; an orphan projection key could
also make a snapshot with no actual consumers appear applicable.

The form now retains its original immutable pipeline-draft baseline. A pipeline
edit invalidates the local form and disables Apply with an EN/RU explanation.
No normalization or graph mutation is emitted from that stale form. Cancel and
reopen restores the already configured bindings under the current names. The
consumer check also iterates actual current steps rather than stale state keys.
This avoids introducing a second mutable step-identity/rename protocol.

```sh
cd ui
pnpm exec vitest run src/features/factory/IssueSnapshotEditing.test.tsx \
  -t 'renaming a step with the form open'
pnpm exec playwright test tests/browser/factory-canvas.spec.ts \
  -g 'renamed snapshot consumer'
```

The unit Red observed both stale forms incorrectly enabling Apply; Green passed
both cases and the surrounding **26 editor/schema-retention tests**. Assertions
verify no normalization/mutation while stale, unchanged existing bindings, and
successful re-application after reopening. Local logs:
`/tmp/octacity-64-rename-{red,green}.log`. The two EN/RU 390 px keyboard/browser
journeys passed in `/tmp/octacity-64-rename-browser-focus.log` and verify the final
validation request still contains the renamed step's Incoming binding and schema.

These changes stay within the existing Console authoring and normalized-schema
API contracts. They add no writer, endpoint, storage table, permission grant or
vocabulary. Rust implementation and API contracts are unchanged by this fix.
The existing snapshot-removal test fixture was corrected from a boolean result
schema to the object-shaped component contract required by snapshot publication;
its preservation assertions were retained.

## Observed Red/Green

Tests assert behavior through rendered editors, publication review and emitted
Flow drafts. HTTP mocks remain at the existing authoring/normalization boundary;
tests do not import the new private ownership/catalog helpers.

| Cycle | Observed Red | Observed Green |
| --- | --- | --- |
| Advanced binding preservation | Four advanced-input cases enabled Apply | All four block Apply, show the existing conflict explanation and emit no mutation/normalization request |
| Snapshot schema lifecycle | Unchanged Apply grew a two-schema catalog to three | Unchanged Apply retains two; material edits retain exactly three including a saved result and the live subset |
| Classification schema lifecycle | Reapplying the same mask grew four schemas to five | Reapply and mask changes retain four and preserve assessment/receipt/saved-result contracts |
| Async classification inputs | Producer, outcome and writer changes each emitted one stale action | All three discard it after normalization completes |
| Publication projections | Both whole-map and nested-source consumers were misreported | Both list only the consumed component, without sibling fields/materials |
| Optional guard ordering | A server-sorted subset reopened as unselected/unsupported | Both optional selections remain checked and Apply retains the existing subset contract |

```sh
cd ui
pnpm exec vitest run src/features/factory/IssueSnapshotEditing.test.tsx \
  -t 'advanced snapshot'
pnpm exec vitest run src/features/factory/IssueSnapshotEditing.test.tsx \
  -t 'catalog bounded'
pnpm exec vitest run src/features/factory/IssueSnapshotEditing.test.tsx \
  -t 'typed classification input schemas'
pnpm exec vitest run src/features/factory/IssueClassificationEditing.test.tsx
pnpm exec vitest run src/features/factory/IssueInputReview.test.tsx
pnpm exec vitest run src/features/factory/IssueSnapshotEditing.test.tsx \
  -t 'canonicalizes schema'
```

Each command was run before its corresponding implementation and produced the
behavioral failure above. Local evidence pairs are
`/tmp/octacity-64-fix-{bindings,schemas,typed-schemas,stale,review,binding-order}-{red,green}.log`.
The mistaken initial Graph-step label was corrected before observing the
classification lifecycle Red; setup failures are not counted as Red.

Focused regressions passed after each slice: ownership **7**, snapshot lifecycle
**15**, typed lifecycle/graph **50**, async guard/graph **47**, publication review
**40**, and optional-order/editor **17** tests. Counts overlap across runs.

## Final verification

All final checks passed:

- **44 script tests and 1,152 Vitest tests** across 135 files.
- **24 Playwright scenarios**, including EN/RU keyboard/narrow snapshot authoring,
  existing delivery input selection and Issue classification.
- ESLint, Prettier and production build with TypeScript/frontend bundle budgets.
- Repository architecture: **64 packages and 263 dependency edges**.
- Strict OpenSpec validation and diff whitespace checks.

```sh
cd ui
pnpm test:unit
pnpm exec playwright test tests/browser/factory-canvas.spec.ts \
  tests/browser/factory-ticket-delivery.spec.ts tests/browser/factory-triage-results.spec.ts \
  tests/browser/issue-classification.spec.ts
pnpm lint
pnpm format
pnpm build
cd ..
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

The production build includes TypeScript and frontend architecture/bundle-budget
checks. Browser tests use the existing mocked HTTP fixture boundary; no native
provider/stand rollout is claimed. Documentation-only edits need no executable
TDD cycle.
