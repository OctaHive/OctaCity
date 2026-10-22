# Task 7.2 verification — durable action projection and recovery

## Scope and authority

The backend-neutral `FactoryActionProjectionStore` persists the safe descriptions
introduced in 7.1. The pure descriptor now lives in the store contract and remains
re-exported by the application; there is one eligibility implementation for direct
inspection, memory and PostgreSQL. No provider, Issue status, latest-execution
selection, new control family or execution claim is introduced. Manual Issue
creation and unlinked Work remain independent.

Migration 0091 adds a per-Run source-version checkpoint, bounded safe action rows
and a visible-Run backfill index. Source Run versions provide same-transaction
logical invalidation: a source commit immediately disqualifies the previous
checkpoint without writing projection tables from execution/admission/control
transactions. There are no invalidation triggers or additional outbox owners.

The application worker selects bounded missing/stale source versions. Publication
locks/rechecks the authoritative Run, rejects stale versions and older observation
times, and replaces the checkpoint and rows in one transaction. Committed versions
are durable backfill checkpoints. An incomplete observation returns no actions or
continuation; covered emptiness remains distinguishable from unfinished backfill.
Expiry is checked before ordering/limits/continuation. Reads do not repair state.

The composition root always schedules maintenance through the existing worker
loop and retention-worker polling/batch policy. Its optional read-model task is
isolated from the fail-fast execution supervisor and readiness. Shutdown owns and
drains its inner task. A stopped worker cannot cancel execution siblings; after
restart, missing/stale checkpoints are selected again. Project/global/Issue queue
HTTP and Console composition remains task 7.3.

## Actual test-first cycles

Tests use public retained-snapshot, projection-store and application-worker
contracts; runtime supervision is exercised through the existing worker harness.
Disposable PostgreSQL tests require `OCTACITY_POSTGRES_URL` explicitly set.

1. Memory persistence and unfinished coverage:
   `cargo test --locked -p octacity-server-application --lib action_projection_persists`.
   **Red:** required projection types/store methods were absent (compile-time
   missing API). **Green:** one test passed after adding the common contract and
   memory implementation. It verifies exact target/contract/version, covered
   versus uncovered results, checkpoint reuse and unchanged execution authority.
2. PostgreSQL publication/recovery:
   `cargo test --locked -p octacity-server-store-postgres --test factory_discovery action_projection_recovers -- --ignored`.
   **Red:** `PostgresStore` lacked the projection port. **Green:** one test passed
   after migration/adapter implementation. The final stronger regression also
   passed with `pg_terminate_backend(pg_backend_pid())` injected between header
   and action-row publication, not just a raised transaction error.
3. Bounded worker/restart:
   `cargo test --locked -p octacity-server-application --lib action_projection_worker_resumes`.
   **Red:** `FactoryActionProjectionWorker` was absent. **Green:** one test passed;
   two Runs are materialized across independently constructed workers with a
   one-Run pass bound, completed work is not repeated, invalid limits are rejected,
   and neither Run advances.
4. Stopped/crashed worker isolation:
   `cargo test --locked -p octacity-server --lib stopped_action_read_model`.
   **Red:** the read-model isolation boundary was absent. **Green:** one test
   passed for both unexpected normal exit and panic, while execution siblings
   remain alive and requested shutdown drains supervision successfully.
5. Migration regression:
   `cargo test --locked -p octacity-server-store-postgres --test migrations migrates_an_empty_database_and_is_reentrant -- --ignored`.
   **Red:** the existing Factory schema invariant rejected two cascading foreign
   keys. **Green:** all seven migration tests passed after using the repository's
   existing non-cascading retention convention.

Existing control tests passed before moving the pure descriptor and after that
move (10 tests each); existing PostgreSQL configured-node discovery passed before
and after extracting the shared admission fixture (2 tests each).

## Final behavioral regressions

| Command | Observed outcome |
| --- | --- |
| `cargo test --locked -p octacity-server-application --lib factory_flow_control_tests` | 13 passed |
| `cargo test --locked -p octacity-server-application --lib factory_node_failure_tests` | 5 passed |
| `cargo test --locked -p octacity-server-store --lib` | 81 passed |
| `cargo test --locked -p octacity-server-store-postgres --test factory_discovery -- --ignored` | 28 passed |
| `cargo test --locked -p octacity-server-store-postgres --test migrations -- --ignored` | 7 passed |
| `cargo test --locked -p octacity-server --lib runtime::workers::tests` | 7 passed |
| `cargo test --locked -p octacity-server --test factory_configured_flow human_contract::actions` | 9 passed |

The PostgreSQL recovery scenario starts with schema 90 and an existing pending
HumanGate, applies migration twice, and observes unchanged execution authority.
It verifies rollback after connection termination, restart, lost-acknowledgement
replay without duplicate rows, exact frozen targets, inclusive deadline/expiry,
stale-version and backwards-observation fencing. With both index tables locked
exclusively, the ordinary response command still succeeds under a two-second
statement timeout. Its source commit invalidates the previous index; the recorded
response retires the unanswered action before owner consumption, and exact command
replay remains available.

Memory coverage additionally verifies future-observation rejection, exclusive
continuation, forged page limits, cancellation and superseded-cycle retirement
before continuation generation. Existing nested/parallel execution and admitted
retry-policy tests remain passing. The full unrelated workspace/browser suites
were not rerun for this persistence-only slice.

`cargo fmt --all -- --check`, `git diff --check`,
`python3 tools/check_architecture.py` (63 packages, 256 edges), and
`openspec validate add-factory-work-card --strict` passed.

The macOS server test linker emitted a large `__eh_frame` compact
unwind warning; linking and all worker tests succeeded.

The first all-target Clippy run exposed unresolved test-module references when
7.1's shared HumanGate tests were included by the server integration harness.
The fixture now shares its local request-context helper and the server harness
imports the existing failure/Issue fixtures under their expected names. No fixture
implementation was copied and no production behavior changed. The nine action
contracts also passed in that independently compiled server harness.

Strict Clippy passed for all targets/features of the four affected crates:

```sh
cargo clippy --locked -p octacity-server-store -p octacity-server-application \
  -p octacity-server-store-postgres -p octacity-server --all-targets --all-features -- -D warnings
```

Builds used a task-owned temporary target with two jobs, incremental compilation
disabled and dev/test debug information disabled. PostgreSQL used the already
available pinned `postgres:18.6-bookworm` image in an explicitly disposable
container with tmpfs storage; no shared cache, pre-existing build directory or
persistent database was cleaned. Free host space was monitored throughout and
never fell below approximately 35 GiB.

The task-owned target (approximately 4.2 GiB), wrapper scripts and disposable
PostgreSQL container were removed after verification. Approximately 39 GiB was
free afterwards. No commit or push was made. Task 7.2 is complete; task 7.3 remains
unimplemented.
