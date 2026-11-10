# Task 2.2 verification

## Scope

Exercise the real accepted-producer → installed classification action → authorized
Issue command → PostgreSQL transaction boundary. Tests pause after the producer
record and completion are committed, use ordinary Issue edit/planning commands
for intervening changes, then resume the action.

The existing transaction now checks all intermediate revisions of each supplied
typed field. Its baseline is the later of the originally reviewed Issue revision
and the last applicable applied assessment of that same canonical field.
Definition publication versions do not change field identity. Only the selected
field value is rebased; attribution retains the original reviewed Issue version.
The existing global assessment ordering fence, source ownership, applicability
checks, rejected management receipts and legacy label ownership remain in place.
No additional writer, mutable assessment store or schema migration was added.

## Observed Red and Green

All Rust commands used `CARGO_INCREMENTAL=0`. PostgreSQL tests used
`OCTACITY_POSTGRES_URL` pointing to a disposable PostgreSQL 18.6 instance.

The pre-existing successful application test passed before and after extracting
shared test setup:

```sh
cargo test --locked -p octacity-server --test factory_configured_flow \
  classification_action_atomically_applies_builtin_and_project_fields_with_provenance \
  -- --ignored --nocapture
```

First behavioral Red:

```sh
cargo test --locked -p octacity-server --test factory_configured_flow \
  classification_rejects_a_manual_priority_change_even_after_the_value_is_restored \
  -- --ignored --nocapture
```

The action resolved `done` instead of returning an Issue conflict after a manual
priority change and restoration. Green followed the intermediate-revision check.
The test was then extended to all five selected targets and renamed
`classification_rejects_manual_edits_even_after_selected_values_are_restored`.

Second behavioral Red:

```sh
cargo test --locked -p octacity-server --test factory_configured_flow \
  classification_allows_a_newer_assessment_of_the_same_reviewed_issue \
  -- --ignored --nocapture
```

Two admitted assessments reviewed the same Issue revision. After the older one
applied, the newer one incorrectly returned `Conflict`. Green followed selection
of the last applied per-field assessment baseline, retaining the manual-change
history check and original reviewed-version attribution.

Final targeted Green:

```sh
cargo test --locked -p octacity-server --test factory_configured_flow \
  issue_outcomes::classification -- --ignored --test-threads=2
```

Nine tests cover:

- Initial atomic application of priority, component, type, size and a selected
  reproducibility property, with the original accepted-record provenance.
- Direct manual conflicts for each of those five targets.
- Change-and-restore conflicts for each target.
- Changed acceptance requirements, including restoration of the original text.
- Source ownership of each target. Trusted ownership is seeded as fixture setup;
  policy reads, application and assertions use the public store/action contracts.
- A delayed older assessment rejected after a newer applicable one, even when
  both propose identical values.
- A newer assessment successfully replacing an earlier applied assessment.
- A manual change-and-restore after an earlier applied assessment, preventing
  the assessment baseline from hiding subsequent manual intent.
- An intervening edit to an unselected property, which is preserved and does
  not block classification.

Every conflict case checks the unchanged current Issue and revision history,
original accepted producer record/completion, unchanged admitted Work, absence
of a successful action result/completion and no additional outbox operation.
Rejection does retain the existing rejected management receipt; it does not
erase the assessment or present it as applied. Successful Run completion can
publish its normal internal lifecycle notification.

## Regression checks

```sh
cargo test --locked -p octacity-server --test factory_configured_flow \
  issue_outcomes -- --ignored --test-threads=2
cargo test --locked -p octacity-server-store-postgres \
  --test issues --test issue_execution -- --ignored --test-threads=2
cargo clippy --locked -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

- Configured Flow Issue outcomes: 15 passed, including the nine classification tests.
- PostgreSQL Issue execution: 24 passed, including original numeric classification
  receipts, replay and size-label ownership.
- PostgreSQL Issues: 81 passed, including source ownership, planning, immutable
  history, transactional rollback, documents and materials.
- Clippy, formatting and diff checks passed. Architecture: 64 packages,
  263 dependency edges.
- OpenSpec strict validation passed. It retains the existing informational
  reminder that the predecessor delivery-preset spec must be synchronized before
  archiving this change.

The final targeted suite was also rerun after giving the classification fixture
a sufficiently long claim window instead of relying on the unrelated legacy
helper's 100 ms claim. Native PostgreSQL tests ran explicitly despite their
`ignored` default. UI and full-workspace suites were not rerun for this backend
slice. Documentation and task tracking have no executable behavior to drive
with TDD.

## Post-review corrections for tasks 2.1–2.2

The review found three behavior defects and duplicated canonical field identity.
The fixes retain the existing writer, receipt format and transaction boundaries:

- Typed priority uses the active Project choice, including an ID named `unknown`.
  Omission/null represents an absent typed assessment; legacy numeric parsing is
  unchanged.
- Console menu options retain their typed priority and distinct `choice:` versus
  `legacy:` menu identities. Selecting Project choice `"2"` preserves its label
  and string type instead of presenting legacy numeric `2` as `High`.
- Legacy priority history compares the complete canonical JSON value. An unchanged
  Project choice baseline no longer becomes SQL NULL and conflicts after a date
  or other unrelated planning edit.
- `IssueClassificationFieldReference::identity()` owns canonical field identity.
  Metadata duplicate validation, attribution replacement and PostgreSQL receipt
  lookup reuse it without numeric identity tags or removing version keys from JSON.

Behavioral Red/Green commands, with the same Cargo/PostgreSQL environment above:

```sh
cd ui
pnpm exec vitest run src/features/issues/IssuePriority.test.tsx
```

Red: the Project choice `"2"` displayed `High`. Green: all eight priority editor
tests passed, including original numeric values and exact string preservation.

```sh
cargo test --locked -p octacity-server --test factory_configured_flow \
  classification_applies_a_project_priority_named_unknown -- --ignored --nocapture
cargo test --locked -p octacity-server-store-postgres --test issue_execution \
  legacy_classification_preserves_unrelated_edits_with_a_project_priority_baseline \
  -- --ignored --nocapture
```

Red: the accepted `unknown` assessment resolved successfully but left priority
empty. Green: it applies the published choice and all five field attributions.
Red: an unrelated due-date edit caused an Issue conflict with an unchanged `high`
baseline. Green: the explicit legacy numeric result applies, preserving planning,
historical priority and admitted Work scheduling.

Before the identity refactor, existing classification tests passed. Added passing
characterization tests reject duplicate locations across catalog versions while
allowing distinct property IDs, and reapply priority under catalog version 2 with
exactly one updated attribution and unchanged historical revision. The same tests
pass after consolidation; this is behavior-preserving refactoring, not a new
behavior requiring a failing test.

UI type checking additionally exposed defaults becoming required in generated
TypeScript. An artifact test in the standard Issue constraint suite first failed
because generated `priorities` was required. API generation now preserves wire
optionality (`defaultNonNullable: false`), while required `labels`/`properties`
remain required. After `CARGO_INCREMENTAL=0 pnpm run api:generate`, all five
constraint/generation tests and the existing typed Issue API contract passed.

Final regression commands:

```sh
cargo test --locked -p octacity-server-store --lib
cargo test --locked -p octacity-server-store-postgres \
  --test issues --test issue_execution -- --ignored --test-threads=2
cargo test --locked -p octacity-server --test factory_configured_flow \
  issue_outcomes -- --ignored --test-threads=2
cargo clippy --locked -p octacity-server-store -p octacity-server-store-postgres \
  -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check

cd ui
pnpm exec vitest run src/features/issues src/api/issuePlanningContracts.test.ts
node --test scripts/issue-constraints.test.mjs
pnpm run typecheck
pnpm run lint
pnpm exec prettier --check scripts/generate-api.mjs scripts/issue-constraints.test.mjs \
  src/features/issues/IssueEditor.tsx src/features/issues/IssuePriority.test.tsx \
  src/features/issues/issuePriority.ts
```

Results: 119 core tests, 25 PostgreSQL execution tests, 81 PostgreSQL Issue tests,
16 Flow Issue outcome tests, 211 Console/API tests and five constraint/generation
tests passed. Clippy, Rust/UI formatting, type checking, lint, architecture and
OpenSpec validation passed, with the same predecessor-spec informational reminder.
Documentation-only changes were checked with the existing validation/diff checks.
The disposable PostgreSQL container was removed after verification. No commit
was created and unrelated pending changes were retained.

## Mixed-format follow-up review

The next review identified two remaining interactions between legacy numeric and
typed Project priority. Both fixes use the existing canonical identity and Issue
transaction; neither changes historical receipt encoding:

- Numeric and typed priority actions now use the same canonical receipt lookup.
  The legacy path no longer ignores a previously applied typed assessment when
  choosing its selected-field baseline.
- A legacy action that changes priority removes the earlier priority attribution
  from the new Issue revision. Its original receipt remains the legacy provenance
  source. Unrelated field origins and historical revisions remain intact. A
  numeric no-op does not clear attribution or create a revision for that purpose.
- Removed the legacy-priority boolean from `field_references()`. It now derives
  only the fields actually supplied by its typed update. Existing successful
  requests already excluded the mixed state represented by that flag.

Tests use the public PostgreSQL Issue classification/query ports and the existing
reviewed execution/admission fixtures. All commands below used
`CARGO_INCREMENTAL=0`; database tests used `OCTACITY_POSTGRES_URL` selecting a new
disposable PostgreSQL 18.6 service.

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_execution \
  newer_legacy_priority_supersedes_a_typed_assessment_of_the_same_reviewed_issue \
  -- --ignored --nocapture
```

Red: two Runs reviewed the same revision; after the older typed Run applied
`high`, the newer numeric Run incorrectly returned `Conflict`. Green: the numeric
result applies one revision, preserving planning, applicability and historical
values. An additional delayed typed result from the older Run still conflicts,
even with a later acceptance timestamp.

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_execution \
  legacy_priority_change_removes_stale_typed_attribution_without_rewriting_history \
  -- --ignored --nocapture
```

Red: numeric `80` was written while priority retained the earlier typed Run's
origin/catalog pin. Green: that attribution is absent from the new revision;
the independently attributed size, planning, evidence/dependencies and original
typed revision stay intact. After restarting the store, retries of both original
commands return their exact original outcomes and do not replace the current
Issue or alter the newer Run's admitted Work.

The two tests passed before removing the unused `field_references` argument.
After that behavior-preserving simplification, all three mixed-format tests
passed. The third test changes priority manually and restores `high` after the
typed assessment: the pending numeric action still rejects without changing the
Issue/history, Work or outbox. Fixture review/admission timestamps now respect
the reviewed Issue's update time.

```sh
cargo test --locked -p octacity-server-store --lib
cargo test --locked -p octacity-server-store-postgres \
  --test issues --test issue_execution -- --ignored --test-threads=2
cargo test --locked -p octacity-server --test factory_configured_flow \
  issue_outcomes -- --ignored --test-threads=2
cargo clippy --locked -p octacity-server-store -p octacity-server-store-postgres \
  -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Results: 119 core, 28 PostgreSQL execution, 81 PostgreSQL Issue and 16 Flow Issue
outcome tests passed (244 total). Clippy, formatting, architecture (64 packages,
263 dependency edges), diff checks and strict OpenSpec validation passed, with
the existing predecessor-spec informational reminder. UI suites were not rerun
for this backend-only follow-up. The disposable service was removed; no commit
was created and existing unrelated changes were preserved.
