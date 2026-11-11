# Task 3.4 verification

## Scope

This slice qualifies the snapshot recovery behavior already provided by the
shared Project text intake, Issue execution publisher and task 3.3 admission
fence. No production code, persistence format or public API changed. The existing
process-death driver is shared by legacy and snapshot tests rather than copied.
The snapshot fixture now accepts explicit built-in string/choice projections;
the replay tests select title, priority and size.

The tests use authorized Issue execution commands and public Issue/catalog,
preparation, Run and retention boundaries against disposable PostgreSQL. SQL
locks inject process interruption; assertions read retained outcomes through
public store contracts. An external byte port simulates a lost acknowledgement.
They do not mock the application publisher or replace the admission transaction.

## Test-first qualification

The new tests were written and run against the existing implementation. They
passed before extracting the shared crash driver and splitting the fixtures.
There was no missing-behavior Red or production correction in this task, and
none is claimed. These are characterization and regression tests for behavior
implemented with Red/Green in [3.2](verification-3.2.md) and
[3.3](verification-3.3.md), followed by test refactoring with unchanged behavior.

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server --test project_text_intake \
  snapshot_byte_ack_recovery -- --ignored --nocapture
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server --test project_text_intake \
  snapshot_process_restart -- --ignored --nocapture
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server --test project_text_intake \
  issue_snapshot_races::replay -- --ignored --nocapture --test-threads=1
```

Observed results: the first byte-ack test passed; the five-stage process-death
test passed; all four replay tests then passed together. Environment failures
are not counted as Red.

| Test | Qualified behavior |
| --- | --- |
| Lost byte acknowledgement | Retry after store/publisher reconstruction reuses the private snapshot, original object identities and resolved base. A later Issue/catalog edit leaves subsequent request/publication replay and the admitted Run unchanged. |
| Process death at five boundaries | Interruption after the first byte, both bytes, byte publication, admission or Issue association recovers the same logical request. A committed admission keeps its original Work/Run; pre-admission recovery creates only one durable Work/Run. |
| Pending snapshot after later edits | Restart returns a conflict when requirements/classification/catalog changed before admission, preserving the earlier private snapshot, source pin, intent, objects and deadline without creating a Run. |
| Private input cleanup after admission | Interrupted association and completed association both recover after body deletion and Issue/catalog edits. Original Incoming values and snapshot digest remain; private bodies are not restored, no new bytes/base resolution occur, and changed scheduling input is rejected. |

The four tests cover nine scenarios: one lost byte acknowledgement, five process
interruption points, one stale pending recovery, and two admitted cleanup cases.
They distinguish edits before admission from edits after commit: the former
require fresh review under task 3.3, while the latter preserve the original
execution. The original priority `high`, size `small`, title, requirements and
definition publication stay frozen after the Issue moves to priority `low`, size
`large`, different requirements and a newer catalog that retires `high`.

Assertions verify the exact retained documents/provenance, snapshot digest,
Incoming values, requirements version, object identities, base revision,
preparation deadline, Work/Run identities and current Issue remaining unchanged
by replay. Public Run discovery verifies that recovery does not create another
processing cycle. A resolver that rejects a second resolution and byte write
counts qualify reuse of the original input across restart.

## Regression qualification

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server --test project_text_intake \
  -- --ignored --test-threads=1
```

Passed: **28 native tests**, including all new snapshot cases and the original
five-stage legacy recovery test after extracting the common driver. Disposable
PostgreSQL 16 and MinIO were configured through `OCTACITY_POSTGRES_URL`,
`OCTACITY_MINIO_ENDPOINT`, `OCTACITY_MINIO_ACCESS_KEY` and
`OCTACITY_MINIO_SECRET_KEY`.

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-application --lib issues
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-store --lib
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-application --lib \
  management::tests::project_text_intake_tests
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-application --lib factory::authoring
CARGO_INCREMENTAL=0 cargo clippy --locked \
  -p octacity-server-factory -p octacity-server-application \
  -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Passed: **186 Issue/application**, **128 store**, **27 shared intake** and
**78 authoring tests** in disjoint selections, plus the **28 native tests**:
**447 behavioral tests**. Clippy, formatting, architecture (64 packages,
263 dependency edges), strict OpenSpec validation and diff whitespace checks
passed. Disposable services were removed after qualification.

The first library/static runs could not fetch dependencies missing from the
local Cargo cache under the sandbox network restrictions. They stopped before
running tests. Restoring cache access and fetching only locked dependencies
allowed all commands above to pass without a lockfile or dependency change.

Snapshot-specific per-step access, admission/scheduling policy, ordinary property
qualification and material-body consumption remain tasks 3.5–3.9.
