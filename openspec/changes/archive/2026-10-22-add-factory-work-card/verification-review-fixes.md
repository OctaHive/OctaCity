# Review corrections for tasks 3.1–3.3

## Changes

`ProjectTextSubmission` separates `ProjectTextIntent` from private documents.
`ProjectTextPreparation` retains metadata and the original admission receipt;
it cannot contain an invalid empty-document retirement sentinel. Memory and
PostgreSQL keep the exact private pair separately. The dedicated document read
requires the original Project, security scope and replay key, and returns `None`
after retirement. Completed replay and pending Issue association use metadata.

Migration 0074 moves schema-73 private bodies into `project_text_documents`,
preserving pending and partially deleted pairs and omitting fully retired pairs.
It preserves the original operation, object identities, admission and deadlines.
State updates no longer rewrite private bodies. Project Artifact reads and
protected transfer share one narrow publication query. Cleanup also selects
only the required Run/object/expiration fields, without the admission Flow.

Migration 0075 adds an indexed cleanup due time and generated, indexed original
Run reference. Fresh reservation sets due time to the original private expiry.
Each deletion acknowledgement atomically advances the preparation to its earliest
remaining object due time; partial acknowledgement remains immediately retryable.
Existing recheck times survive migration. The superseded object recheck index is
removed. Hold filtering, Build locking, deletion reservation and the post-lock hold
check retain their original order. The existing worker remains the cleanup owner.

Preparation lease/deadline/attempt limits and the hourly recheck interval have
named constants with unchanged values. Reference documentation now describes
the delivered PostgreSQL/runtime composition and the separated storage contract;
the quickstart handoff uses the same shared types and recovery mechanism.

## TDD and characterization

The initial baseline passed four PostgreSQL request/store scenarios and nine
integration entries (eight behavioral scenarios plus the subprocess fixture).
Seven common application scenarios also passed during the first refactoring step.
Refactoring keeps existing public behavioral tests; new observable contracts use
the existing store and migration/schema interfaces.

| Test | Observed Red | Green |
| --- | --- | --- |
| `preparation_metadata_does_not_expose_private_documents` | Preparation metadata included private task/acceptance documents | Reservation and restarted metadata reads contain neither body |
| `cleanup_schedule_is_indexed_and_tracks_original_expiry_and_rechecks` | Required `cleanup_at_millis` schema interface was absent | Original expiry, ordered due-index eligibility, one-object acknowledgement, pair retirement and hourly recheck pass |

Missing required schema is the second Red; dependency/setup errors are not counted
as Red. `EXPLAIN` checks ordered-index eligibility independently of the tiny
fixture's cost estimates. It is not a production load benchmark.

Additional migration characterization upgrades schema 73 with pending, partially
deleted and retired pairs. Public reads preserve exact bytes, scoped access,
intent and immutable object metadata; retired bodies remain absent. The migrated
cleanup schedule preserves original expiry and already acknowledged rechecks.
Migration is repeated to verify reentrancy. The strict migration table inventory
includes the new private-body table without removing existing assertions.

Builds reuse the existing target with these settings:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
# OCTACITY_POSTGRES_URL names an owned disposable loopback PostgreSQL service.
# Native storage tests also set OCTACITY_MINIO_ENDPOINT,
# OCTACITY_MINIO_ACCESS_KEY and OCTACITY_MINIO_SECRET_KEY for disposable MinIO.
```

Red/Green commands:

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_execution preparation_metadata -- --ignored
cargo test --locked -p octacity-server-store-postgres --test issue_execution cleanup_schedule -- --ignored
```

Qualification commands:

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_execution \
  --test issues --test factory_discovery --test factory_retention \
  --test retention --test migrations -- --ignored
cargo test --locked -p octacity-server-store-postgres --test issue_execution separated_documents -- --ignored
cargo test --locked -p octacity-server --test project_text_intake -- --ignored
cargo test --locked -p octacity-server-store -p octacity-server-application --all-features --lib
cargo test --locked -p octacity-server --lib runtime::factory
cargo clippy --locked -p octacity-server-store-postgres -p octacity-server-application \
  -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
openspec validate add-factory-bugfix-quickstart --strict
git diff --check
```

## Qualification results

- PostgreSQL: 72 scenarios passed (24 discovery, seven Factory retention,
  seven Issue execution/storage, 24 existing Issues, six migrations, four general
  retention scenarios).
- Integration: ten entries passed, including nine behavioral scenarios and the
  subprocess fixture. This includes real process termination, transaction faults,
  cleanup/holds, replay after retirement, native MinIO and presigned download.
- Native Factory runtime composition: 44 scenarios passed.
- Clippy passed for all targets/all features with warnings denied.
- Full application/store libraries, all features: 526 + 81 scenarios passed.
- Architecture passed: 63 packages and 256 dependency edges. Formatting, strict
  validation of both OpenSpec changes, seven local documentation links and
  whitespace checks passed.

The migration characterization was extended with partial acknowledgements after
the PostgreSQL batch; its focused rerun passed with all three original storage states.
Documentation-only edits do not change executable behavior and use formatting,
local-link, whitespace and strict OpenSpec validation instead of TDD.

Scope remains tasks 3.1–3.3; work-card progress remains 17/56. The unrelated
whole-server Delivery shell-host instability documented in task 3.3 was not
requalified by this correction; the relevant Factory runtime selection was run.
No commit, deployment, provider call, model call, image build or image download
was performed. Disk remained approximately 15 GiB free during qualification.
The task-owned PostgreSQL and MinIO containers were removed after qualification.
