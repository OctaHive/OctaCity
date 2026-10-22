# Task 5.3: classified Reporter conflicts and reviewed policy resolution

## Boundaries and behavior

Existing Reporter Inspect/Apply, ordered managed fields, worker claims and the
append-only outbox remain the only delivery path. Migration 0087 adds a nullable
closed rejection reason to that journal. Legacy rows and their identity encoding
remain unchanged when no classified reason is present. Conflict and unsupported
writes stop automatic retry; transient failure exhausts its original finite
attempt/window policy. Current managed connection grants/disablement are checked
before process calls. New managed publications freeze their original connection,
metadata version and any capability-configuration alias in private mapping/intent
material. Dispatch requires the original managed authority rather than borrowing
an identical legacy profile. Public mapping/choice responses contain no new
connection handle. Local status, requirements, status history and accepted
Factory results remain independent of technical delivery.

A resolution is an authorized current-version policy command: current Issue
revision, current mapping pin and exact terminal receipt for the original native
link/security partition. It reuses ordinary mapping qualification and publication,
committing a new mapping and desired intent together. The new intent retains its
`resolves` lineage; the old operation is never reopened or retargeted. Both inputs
remain independently readable even when they share an Issue revision. Scoped
replay retains one original effect, competing/stale policy reviews conflict and
unsupported effects fail before any publication. Normal mapping edits create no
new projection. Manual Issue creation/editing has no new capability requirement.

The optional REST command uses the existing authorization inventory, bounded
mapping schema and precondition/idempotency helpers. Its body is `mapping` plus
an exact `review`; no credentials, endpoint, arbitrary instruction or force flag
is accepted. The command publishes no provider I/O. Detailed read/Console feedback
is still task 5.4.

## TDD evidence

All Rust commands below use `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_BUILD_JOBS=2` and `--offline`. Ignored database
scenarios use `OCTACITY_POSTGRES_URL` pointing to the owned disposable
`octacity-issue-push-53` PostgreSQL 18.6 service, limited to 512 MiB RAM and
384 MiB tmpfs. Import/fixture errors and implementation compile repairs are
excluded from Red evidence.

| Command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test --offline -p octacity-server --all-features --lib issue_reporter_conflict_is -- --ignored` | Required retained failure type/field were absent | A real conditional process conflict remains Conflict after store reconstruction, no automatic retry or local rollback, unrelated human state preserved |
| Same command, filter `issue_reporter_unsupported_is` | Unsupported/Exhausted outcome contracts were absent | Unsupported stops after one inspection; transient outage makes exactly three bounded attempts, then remains exhausted while local metadata stays retained |
| Same command, filter `issue_reporter_revoked_or_disabled` | Required durable forbidden outcome was absent | Revoked grants and disabled connections stop subsequent process calls; original local result remains unchanged |
| Same command, filter `issue_reporter_resolution_publishes` | Reviewed resolution store API and outcome were absent | One new policy/intent commits and delivers through Reporter; unsupported/competing requests reject, restart/replay preserves one effect and original Issue/status history/failed receipt |
| Same command, filter `issue_reporter_resolution_rechecks` | Authorized resolution command was absent | Revoked writer authority rejects before mapping/effect acceptance; restored grant permits exactly one reviewed policy command and replay performs no provider I/O |
| Same authority filter, managed-origin qualification | A newly accepted managed policy was delivered by an otherwise identical legacy writer without any current-grant resolver | The unqualified writer is rejected Forbidden before process I/O; a new explicit repair delivers through the shared original-connection resolver and preserves both old terminal receipts |
| `cargo test --offline -p octacity-server-api-rest --test v1_handlers issue_source_http_resolution` | Optional HTTP resolution composition API was absent | Exact version/receipt are forwarded; missing If-Match rejects before handler, successful response is bounded/no-store, and force input is rejected |

## Regression verification

All final affected suites passed after the managed-authority correction:

| Command | Result |
| --- | --- |
| `cargo test --offline -p octacity-server --all-features --lib runtime::work_source_tests::reporting::issues -- --ignored --test-threads=2` | 14 passed |
| `cargo test --offline -p octacity-server-store-postgres --test issues --test migrations -- --ignored --test-threads=2` | 42 Issue and 7 migration tests passed |
| `cargo test --offline -p octacity-server-application --lib issue_` | 107 passed |
| `cargo test --offline -p octacity-server-application --lib source_` | 143 passed |
| `cargo test --offline -p octacity-server-store --lib` | 81 passed |
| `cargo test --offline -p octacity-server-api-rest --lib --test v1_handlers` | 12 library and 136 HTTP tests passed, including the authorization operation inventory |
| `cargo test --offline -p octacity-server --all-features --lib runtime::issue_tests -- --ignored --test-threads=2` | 2 manual/no-provider runtime journeys passed |
| `cargo clippy --offline -p octacity-server -p octacity-server-application -p octacity-server-store-postgres --all-features --all-targets -- -D warnings` | Passed |

The earlier shared Reporter regression passed all 55 tests, including existing
Work delivery, unknown-response recovery, ordering and fencing. PostgreSQL
discovery (27), retention (7), Factory action application (21), current Source read
grant (1) and retained-replay grant (1) regressions also passed. The migration
regression first rejected the new JSON failure column as insufficiently typed
and bounded; adding explicit JSON type and byte limits made all 7 migration
tests pass, including forward-failure/previous-binary rollback. The REST operation
inventory fixture was extended for the new route; its initial inventory mismatch
is not counted as behavior Red evidence.

Console checks passed: `pnpm --dir ui run api:generate`, `format` (Prettier),
`lint`, `typecheck`, and `test:unit` (33 Node checks and 570 Vitest tests across
83 files). Regeneration after the private authority change produced exactly the
same generated API file hashes as the verified client. No Console behavior is
added in this task. Process-peer tests exercise existing protocol contracts;
these results do not claim a live external-provider deployment test.

`cargo fmt --all -- --check`, `python3 tools/check_architecture.py`,
`openspec validate add-factory-work-card --strict` and `git diff --check` passed
(architecture: 63 packages and 256 dependency edges). Documentation-only edits use specification/format/diff
checks without executable TDD. No provider SDK, synthetic execution, second
outbox, dependency or new worker was introduced. No commit is created by this
implementation task. Rust builds keep incremental compilation and debug symbols
disabled to bound disk use; the disposable PostgreSQL service was removed after
verification, with 10 GiB available on the workspace volume. The macOS linker reports its existing large `__eh_frame` warning;
linking, behavioral tests and Clippy succeed.
