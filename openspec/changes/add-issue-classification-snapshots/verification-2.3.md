# Task 2.3 verification

## Scope

Manual preparation uses the existing authorized `CreateIssueCommand`,
`EditIssueCommand` and `EditIssuePlanningCommand`, without a producer Work or Run.
The shared Issue revision logic records the actual management actor only for
changed built-in classification fields and semantically bound Project properties.
Ordinary custom properties remain ordinary planning data. Unset values lose their
field attribution; unchanged fields keep their original origin, requirements
version, assessment time and evidence references.

Both store adapters commit values, attribution and the original scoped receipt
through their existing mutation boundary. There is no new writer, schema
migration, scalar value store or execution side effect. A combined requirements
and priority edit attributes the new priority to the new requirements revision;
it does not refresh the remaining fields. Default priority/type choices retain
catalog version zero, while custom properties and components still require
positive published catalog versions. Default choices do not populate Issue values.

The shared planning validator rejects newly supplied verified reproduction claims
through both edit and creation. Supported unresolved values remain available.
Unchanged historical claims retain their original metadata, without becoming new
manual evidence. Evidence applicability/read diagnostics remain task 2.4; REST
classification projections and Console controls remain section 6.

## Observed Red and Green

All Rust commands used `CARGO_INCREMENTAL=0`. PostgreSQL commands selected an
explicit disposable PostgreSQL 18.6 service through `OCTACITY_POSTGRES_URL`.
Authorization/fixture setup failures were corrected before observing behavioral
Red; they are not counted as Red.

Each application test below was first run separately:

```sh
cargo test --locked -p octacity-server-application --lib \
  manual_classification_planning_retains_human_origin_without_a_run -- --nocapture
cargo test --locked -p octacity-server-application --lib \
  manual_classification_priority_override_does_not_refresh_other_stale_fields -- --nocapture
cargo test --locked -p octacity-server-application --lib \
  manual_classification_cannot_claim_verified_reproduction_on_edit_or_creation -- --nocapture
cargo test --locked -p octacity-server-application --lib \
  manual_classification_uses_default_catalog_on_creation_and_edit_without_setup -- --nocapture
```

- Planning Red: five supplied classification fields had zero attributions. Green:
  actual human attribution and exact catalog/revision/time were retained without a
  Run. The final test additionally covers risk, recommended route, clearing values,
  immutable history and exact retry after clearing.
- Priority Red: the manually changed priority had no manual origin. Green: it has
  its own origin while unrelated stale size/severity retain their original age.
  Additional passing checks cover a severity override, an unrelated due-date edit
  and replay returning the original edit without replacing newer values.
- Reproduction Red: a manual `reproduced` value was accepted without independent
  evidence. Green: both `reproduced` and `not_reproduced` fail atomically on edit and
  creation; `needs_input` remains usable without an invented evidence reference.
- Default-catalog Red: an Issue created with priority/type/size had no attribution.
  Green: creation and later edits retain the actual trusted-network attribution
  and exact default catalog without inventing a person or requiring setup.
- A further Red extended the default-catalog test to change requirements and
  priority together: the edit incorrectly failed with `InvalidIssue` because its
  new requirements version exceeded its assessment version. Green: the changed
  field assesses the resulting requirements revision, and retained metadata
  round-trips through ordinary Issue decoding.

```sh
cargo test --locked -p octacity-server-application --lib manual_classification -- --nocapture
```

Final targeted application result: four passed.

```sh
cargo test --locked -p octacity-server-store-postgres --test issues \
  manual_classification_persists_origins_stale_fields_and_original_receipts \
  -- --ignored --nocapture
```

PostgreSQL Red: creation returned zero origins instead of three. Green: creation,
planning and priority edits retain their origins, stale unrelated fields and
immutable revision history. Restarted reads/retries return the original creation
and edit outcomes while preserving the current Issue.

## Regression checks

```sh
cargo test --locked -p octacity-server-application --lib issues
cargo test --locked -p octacity-server-store --lib
cargo test --locked -p octacity-server-store-postgres \
  --test issues --test issue_execution -- --ignored --test-threads=2
cargo test --locked -p octacity-server-store-postgres --test issues \
  -- --ignored --test-threads=2
cargo test --locked -p octacity-server --test factory_configured_flow \
  issue_outcomes -- --ignored --test-threads=2
cargo clippy --locked -p octacity-server-store -p octacity-server-store-postgres \
  -p octacity-server-application --all-targets --all-features -- -D warnings
cargo clippy --locked -p octacity-server-store-postgres \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

The historical migration fixture initially used the current manual writer to
produce records without attribution. It now explicitly restores the historical
JSON shape in revisions and both original receipt payloads before migration,
checks the number of restored receipts and verifies original replay before and
after migration/snapshot restoration. Its focused upgrade/replay test passed.
The new implementation does not backfill attribution into historical records.

UI/browser and full-workspace tests are outside this backend slice. Documentation
and task tracking have no executable behavior to drive with TDD.

Final results: 173 application Issue tests, 119 core store tests, 82 PostgreSQL
Issue tests, 28 PostgreSQL execution tests and 16 configured Flow Issue outcome
tests passed (418 total). The full PostgreSQL Issue suite passed after correcting
the historical fixture. Both Clippy invocations, formatting, architecture (64
packages, 263 dependency edges), strict OpenSpec validation and diff checks passed.
The disposable PostgreSQL container was removed after verification. No commit
was created.
