# Task 4.1 verification

## Scope and public boundary

`automatic_issue_snapshot_waits_for_classification_while_another_item_progresses`
drives the real Source worker with a verified process-provider fixture and an
isolated migrated PostgreSQL database. It creates and links canonical Issues
through ordinary store commands, polls two native candidates, edits classification
through the public planning command and polls again. No live GitHub credentials
or provider service are used.

The first candidate has no size; the second has `small`. Both have canonical
requirements and acceptance that differ from provider text. The first poll must
create only the ready candidate's Run, leaving the other preparation waiting
without an admission intent. After setting the pending Issue's size to `medium`,
the next poll must admit it through the same preparation. Assertions inspect
literal Incoming values, canonical Issue identities, absent predecessor metadata,
retained snapshot version/definition/attribution and its exact Incoming digest.

## Observed Red/Green

```sh
# OCTACITY_POSTGRES_URL selects an owned disposable PostgreSQL service.
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib automatic_issue_snapshot_waits_for_classification \
  -- --ignored --nocapture
```

**Red:** the fully initialized worker produced zero Runs instead of one:
`the ready Issue must progress while the first Issue lacks classification`.
The previous Source path expected a predecessor for Incoming and could not
consume canonical Issue fields. A test-fixture private `flow()` call was corrected
before this run; that compilation failure is not counted as Red.

**Green:** the test passes with one ready Run on the first poll and two Runs after
classification. The original expected payloads remain unchanged. The final run
also checks the frozen Source journal data and digest; it passes in 5.93 seconds.
The new reader is explicitly installed in the fixture, as in both production
Source-worker compositions.

## Implementation boundaries

The independent Source Issue reader resolves the exact scoped native link and
current authorized canonical Issue. It calls the existing manual-execution
snapshot builder for fields, definitions, attribution, readiness and any declared
admission classification policy. Canonical task/acceptance documents reuse the
existing mapping and immutable Source byte publication.

The private snapshot freezes atomically with the original preparation policy in
the existing Source JSONB journal. Its target remains the original input's pinned
Configuration/Repository, not a replacement definition. Common snapshot admission
validation checks Incoming and scheduling values; the Source owner additionally
checks its exact task/acceptance bytes. Before initial admission, the worker
rechecks canonical input. PostgreSQL reuses the manual snapshot head fences inside
the ordinary Source admission transaction and verifies the native link before
associating Work with that exact Issue revision. Cleanup clears the new private
field alongside existing Source snapshot/policy bodies.

Missing required classification is a candidate-local input wait, so it does not
abort the polling page. Existing predecessor waiting and non-snapshot provider
mapping remain available. No new table, migration, byte store, producer lookup,
provider write or import policy is added. Discussion/document readers are not
installed for automatic snapshots in this slice: an explicit automatic material
policy is task 4.4. Expanded duplicate/restart and diagnostic matrices remain
tasks 4.2–4.3; this report does not claim their qualification.

## Regression commands and outcomes

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::admission
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issues
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib management::tests::project_text_intake_tests
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-store --lib
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib runtime::sources::tests::worker

# Disposable PostgreSQL and MinIO are selected with OCTACITY_POSTGRES_URL and
# OCTACITY_MINIO_ENDPOINT/ACCESS_KEY/SECRET_KEY for the native suites.
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib runtime::sources::tests::worker \
  -- --ignored --test-threads=2
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --test project_text_intake -- --ignored --test-threads=2
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-store-postgres --test factory_discovery \
  source_publication_is_atomic_owned -- --ignored --nocapture

CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-application -p octacity-server-store \
  -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

The initial `factory::sources` application filter selected zero tests because
Source behavioral tests live under `factory::admission`; the corrected command
above is the actual regression qualification.

Passed disjoint suites: **153 admission**, **211 Issue**, **27 shared intake**,
**128 store**, **97 ordinary Source worker**, **49 native Source worker**,
**34 native manual intake** and **one PostgreSQL Source publication contract**:
**800 tests**. The focused Red/Green test is included in the 49 native worker
tests and is not counted again. All native suites pass with their ignored tests
explicitly enabled. Clippy with warnings denied, formatting, architecture
(64 packages, 263 edges), strict OpenSpec validation, 37 local documentation
links and diff whitespace checks pass. Native linking retains the existing macOS
compact-unwind size warning. Documentation edits have no executable behavior and
are validated by link/spec/diff checks rather than a separate TDD cycle.

Only the owned disposable PostgreSQL/MinIO test containers are stopped after
qualification. No commit is created for this task.
