# Task 4.2 verification

## Scope and public boundaries

Two new native tests exercise the real Source worker and migrated PostgreSQL,
reusing task 4.1's canonical Issue/Flow fixture and the verified process-provider
fixture. Input/Configuration publications, Issue edits and all assertions use
public store/worker boundaries. No test mutates database internals or bypasses
admission to fabricate a receipt. Restart constructs a new PostgreSQL store and
worker over the retained database; the byte publisher models an independent
durable service. No live provider or credentials are required.

`automatic_issue_snapshot_duplicates_and_restart_keep_one_run_after_classification_label_and_target_edits`:

- Publishes a literal required-label predicate and admits a linked Issue with
  `size=small` and `priority=high`.
- Publishes `size=large` and `priority=low` through canonical Issue commands, then
  repeats the native observation, removes its required label and re-adds it with
  changed native update markers. Every pass uses a freshly constructed store and
  worker and checks that only one Run exists.
- Publishes a replacement Configuration with a new snapshot contract and a new
  input version under the same stable input ID. The next restarted pass still
  retains the original Work/Run, complete Incoming, scheduling values and bytes.
- Reads current Issue values independently and verifies that they differ from
  the admitted literal `small/high` payload. The Issue retains exactly its
  original native Work association and requirements revision.

`automatic_issue_snapshot_restart_after_lost_byte_ack_retains_preparation_and_admits_once`:

- Pauses the external byte publisher after both original objects have been
  written but before the second acknowledgement reaches the application.
- Observes a frozen snapshot, reserved exact objects, no published manifest,
  no admission intent and no Run; aborts that worker and advances trusted time
  beyond its claim lifetime.
- Restarts against the same database. The same preparation/input/snapshot and
  Artifact identities are recovered, then one Work/Run admits the original
  canonical literal payload and sealed digest.
- Repeats restart/rediscovery twice, retaining the same receipt and one Run,
  without any further byte writes after admission.

## Observed test-first outcomes

```sh
# OCTACITY_POSTGRES_URL selects the owned disposable PostgreSQL service.
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib automatic_issue_snapshot_duplicates_and_restart \
  -- --ignored --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib automatic_issue_snapshot_restart_after_lost_byte_ack \
  -- --ignored --nocapture
```

Both tests passed on their first executable run against task 4.1's implementation
(5.01 seconds and 4.77 seconds respectively). This is regression qualification of
existing behavior, not a runtime behavior change. There is no independent
missing-behavior **Red** for task 4.2, and none is claimed. Task 4.1's separate
behavioral Red remains recorded in its own report. No implementation is altered
to manufacture a failure, and no new identity/deduplication layer, migration or
production code is needed. Final assertions additionally qualify the canonical
association and current-versus-frozen values.

## Regression commands

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib runtime::sources::tests::worker

# Disposable PostgreSQL is selected through OCTACITY_POSTGRES_URL.
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib runtime::sources::tests::worker \
  -- --ignored --test-threads=2
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib runtime::sources::tests::worker::issue_snapshots \
  -- --ignored --test-threads=2

CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Passed **97 ordinary Source worker** and **51 native Source worker** tests:
**148 disjoint tests**. The focused snapshot run passes **three tests**, including
task 4.1's wait/progress scenario and both new recovery tests; these are included
in the native total, not counted twice. The full native run completes in 222.07
seconds and the final focused run in 9.96 seconds. Clippy with warnings denied,
formatting, architecture (64 packages, 263 dependency edges), strict OpenSpec,
38 local documentation links and whitespace checks pass. Native linking retains
the existing macOS compact-unwind size warning. Only the owned disposable
PostgreSQL test container is stopped afterward.

Task 4.3's expanded stale/missing/evidence/subject diagnostics and task 4.4's
automatic material policy remain separate. This slice adds tests and reference
documentation only, preserving all uncommitted task 4.1 changes. No commit is
created. Documentation has no executable behavior and is checked through local
links, OpenSpec validation and whitespace checks.
