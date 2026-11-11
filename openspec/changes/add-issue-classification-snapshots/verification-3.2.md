# Task 3.2 — manual Issue snapshot admission

## Scope

A manually classified Issue can reserve and publish a snapshot consumer without
selecting a predecessor Run. The authorized Issue adapter resolves the exact
reviewed revision and the input contract's Project catalog. It selects canonical
values under consumer aliases, retaining their original attribution, assessment
applicability, definitions and requirements digests. No producer acceptance or
transcript is manufactured.

The request and Project text preparation retain the selection digest. The
existing private `ProjectTextDocuments` body retains the selected data, under the
same preparation, security scope and expiry as the requirements input. The input
owner still resolves and freezes code before publishing task/acceptance bytes,
and admits one ordinary Work/Run. Its immutable Incoming data binds the selection
reference and projected values to the Work digest, resolved subject, destination
Run and pinned definition. The snapshot does not become a Project catalog document
or an implicit task/acceptance input.

Legacy encodings omit the new optional metadata. Explicit predecessor inputs keep
their existing path and validations. Ordinary manual text submission cannot supply
a canonical Issue snapshot; only the authorized Issue adapter enables that path.
Required unavailable material/evidence selections fail closed and optional ones
retain explicit omissions. Body readers, independent evidence consumption,
concurrency qualification, full restart/replay qualification, per-step access,
admission-classification policy and ordinary-property qualification remain in the
separately planned tasks 3.3–3.9.

## Behavioral TDD

All test commands use `CARGO_INCREMENTAL=0`.

1. `cargo test --locked -p octacity-server-application --lib manual_issue_snapshot_request_freezes_its_selection`
   observed Red (`missing_incoming_result`), then Green: a request retains its
   selection digest, creates no Run and exact transport replay returns the same
   reservation.
2. `cargo test --locked -p octacity-server-application --lib manual_issue_snapshot_publishes_frozen_inputs`
   observed Red (`missing_incoming_result`), then Green: ordinary publication
   retains the original task/acceptance identities, selected title/size/priority,
   catalog and manual attribution, Issue/applicability versions and resolved base.
   The default catalog omits its standard choice arrays in JSON; the test reads
   that documented encoding through `PublishedIssueProperties` before asserting
   the independently specified `blocker` choice.
3. `cargo test --locked -p octacity-server-application --lib manual_issue_snapshot_checks_code_dependent`
   observed Red: an assessment of another base was admitted. Green checks its
   original subject against the frozen resolved base before publication, with a
   field-specific blocker and no admitted Run or visible input publication.

4. `cargo test --locked -p octacity-server-application --lib manual_issue_snapshot_rejects_a_value`
   observed Red: a choice introduced in catalog v1 was accepted by a v0 input
   merely because the consumer allowed strings. Green validates the actual
   canonical value against the pinned Project type/choices, with a field-specific
   incompatible-value diagnostic and no reservation mutation.

5. `cargo test --locked -p octacity-server-application --lib manual_issue_snapshot_does_not_treat`
   observed Red: an optional component with an empty pinned choice list accepted
   a value from a newer catalog. Green validates supplied values with required
   presence, so the optional-empty publication shape permits omission without
   becoming an arbitrary-text vocabulary.

The first four snapshot tests passed before extracting the shared selection-digest and
Incoming construction helpers. The regression run checks those refactorings.

## Final qualification

```sh
cargo test --locked -p octacity-server-factory -p octacity-server-store \
  -p octacity-server-application --lib
cargo test --locked -p octacity-server-application --lib issues
cargo test --locked -p octacity-server-store-postgres --test issue_execution -- --ignored
cargo test --locked -p octacity-server --test project_text_intake -- --ignored --test-threads=1
cargo clippy --locked -p octacity-server-factory -p octacity-server-application \
  -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

The full library run passed **1,581 tests** (1,103 application, 351 Factory,
127 store). After the final empty-choice correction, all **180 Issue application
tests**, including all five new snapshot cases, passed. The final correction is
isolated to snapshot value qualification; authoring and ordinary incoming sources
retain their previous shape rules.

Native regression passed **30 Issue execution tests** and **17 Project text
intake tests** against disposable PostgreSQL 16 and MinIO, including HTTP restart,
unknown publication/admission responses, association repair, retention and the
explicit predecessor classification path. An initial native run lacked MinIO
settings; the final run used the literal HTTP harness credentials and sequential
execution. A time-bounded classification test that returned `Store(Unavailable)`
in the initial parallel run passed in the sequential native runs. Environment
setup failures are not counted as behavioral Red. No production behavior was
changed to accommodate them. The disposable containers and their volumes were
removed after qualification.

Final Clippy passed with warnings denied for all targets/features of the four
selected packages. Formatting, architecture (64 packages, 263 edges), strict
OpenSpec and diff checks passed. This task adds no migration, REST schema or UI;
backend snapshot-specific restart/race/access qualifications remain in their
separately planned slices and are not claimed by these legacy native regressions.

No commit was requested for task 3.2.

## Post-review corrections

The four review findings were addressed without advancing tasks 3.3–3.9:

- A selected classification property's consumer binding must match the binding
  in its original assessment catalog. Rebinding a property from reproduction to
  risk cannot supply an unverified `reproduced` claim to an older consumer.
  Compatible publications, including display-name changes, remain usable.
- Canonical value absence is handled before assessment applicability. Required
  absence reports `issue_snapshot_missing_value`; optional absence is retained
  as `Missing`, independently of stale-assessment diagnostics.
- `factory::issue_snapshot::resolve` is shared by authoring and execution and
  returns the authorized, validated catalog. Selection no longer reloads that
  catalog or calls an authoring-specific validation workflow.
- The snapshot model owns code-subject compatibility. The common text intake
  invokes that rule after source freezing and keeps publication/admission ownership.

### Behavioral TDD and refactoring baseline

Commands below use `CARGO_INCREMENTAL=0`.

```sh
cargo test --locked -p octacity-server-application --lib snapshot_rejects_reproduction_reinterpreted
cargo test --locked -p octacity-server-application --lib 'snapshots::presence'
cargo test --locked -p octacity-server-application --lib 'execution::snapshots'
```

The semantic-rebinding test observed Red: the reviewed request was accepted with
no evidence. Green rejects it with `issue_snapshot_semantic_binding_changed`
before reserving a request or creating Work/Run. Both presence tests observed
Red: missing required size returned `issue_snapshot_inapplicable`, and missing
optional component was retained as `Inapplicable`. Green reports and retains the
independently specified missing states through request and publication boundaries.

Five existing snapshot tests passed before changes. Before the shared-resolver
and subject-policy refactor, all nine snapshot tests and 78 authoring tests
passed. The ninth snapshot case characterizes an allowed compatible catalog
rename, including its original consumer and assessment definition versions.
Fixture setup/compiler errors were corrected before observing behavioral Red.

### Final correction qualification

```sh
cargo test --locked -p octacity-server-application --lib issues
cargo test --locked -p octacity-server-application --lib factory::authoring
cargo test --locked -p octacity-server-application --lib management::tests::project_text_intake_tests
cargo test --locked -p octacity-server-store --lib
cargo clippy --locked -p octacity-server-factory -p octacity-server-application \
  -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

**416 tests passed**: 184 Issue application tests (including all nine snapshot
cases), 78 authoring tests, 27 common text-intake tests and 127 store tests.
Final Clippy, formatting, architecture and strict OpenSpec checks passed.
The corrections change no storage format, transport schema or database writer.
No commit was requested.

## Compact selected definitions

The additional review identified whole-catalog duplication across assessment
versions. A legal 32-publication history could inflate 375 bytes of selected
values to roughly 1.7 MiB and fail the existing 1 MiB private-input bound.

New reservations now retain `selected_v1` definition fragments. Each fragment
contains its original Project/version/publication time, the SHA-256 of the
complete original publication, and only referenced built-in choice lists or
property declarations. Property fragments preserve the original kind, bounds,
choice vocabulary and semantic role. Stable property IDs are deduplicated within
each publication. Unselected labels, components, types and properties are omitted;
a fragment is explicitly distinguished from a complete `PublishedIssueProperties`.
Readiness still uses the authorized complete publications before compaction.
The input bound is unchanged and the existing preparation/admission owner remains
responsible for all input publication.

### Red/Green and compatibility

Commands use `CARGO_INCREMENTAL=0`.

```sh
cargo test --locked -p octacity-server-application --lib snapshot_admits_small_selected_values
cargo test --locked -p octacity-server-store --lib legacy_full_catalog_snapshot
cargo test --locked -p octacity-server-application --lib snapshot_publishes_a_retained_full_catalog_request
```

The 32-publication execution test observed Red through authorized Issue execution:
reservation failed with `InvalidProjectTextInput`. Green reserves and publishes
Work/Run with all 32 selected values and a snapshot below 64 KiB. Assertions verify
the first original field/choice/semantic role, its complete-publication digest,
and the absence of unrelated dictionaries.

A literal full-catalog wire fixture with an independently pinned SHA-256 passed
before and after the model update, preserving original bytes and digest. Both
compact and legacy definition forms are readable. A second regression test then
observed Red for a pre-upgrade request whose input preparation had not begun.
Green preserves that request's original digest and publishes/replays its original
input identity. Recovery selects the legacy representation only when its bytes
match the retained receipt exactly, after the same readiness checks; fresh
reservations always select the compact form.

### Final compaction qualification

```sh
cargo test --locked -p octacity-server-application --lib issues
cargo test --locked -p octacity-server-application --lib factory::authoring
cargo test --locked -p octacity-server-application --lib management::tests::project_text_intake_tests
cargo test --locked -p octacity-server-store --lib
cargo clippy --locked -p octacity-server-factory -p octacity-server-application \
  -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

**419 tests passed**: 186 Issue tests (including all 11 snapshot cases), 78
Factory authoring tests, 27 shared text-intake tests and 128 store tests.
Clippy, formatting, architecture and strict OpenSpec checks passed. This change
adds a private versioned input representation with legacy decoding, without a
new persistence owner, database migration or transport-schema change. Separately
planned tasks 3.3–3.9 remain pending. No commit was requested.
