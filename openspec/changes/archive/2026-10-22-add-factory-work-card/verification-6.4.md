# Task 6.4 — bounded timeline and result observations

## Scope

`IssueTimelineStore` derives metadata from the existing Issue revisions, explicit
status history, execution requests, discussion metadata and scoped Source read
receipts. No migration, parallel event journal, execution owner or provider port
is added. Manual task management and discussion remain independent of Factory.

Memory captures the local boundary under the existing Issue lock. PostgreSQL
uses a read-only repeatable-read transaction and bounded metadata queries; body
columns and status reasons are not selected. Shared ordering is original time,
source kind (revision/status/execution/discussion/source), then canonical stable
identity lexically. Original Issue version, committed request watermark and
lifetime discussion sequence exclude later publications, including backdated
execution requests. Current Issue version remains separately visible.

Source has an independent scoped fingerprint of native-link correlation,
append-only observation membership, pull generation and mapping version. Changes
mark only that section stale; an optional-section savepoint allows local metadata
to survive unavailable Source storage. Originally unavailable Source coverage
stays unavailable for the traversal even after recovery. The receipt expiry is
provenance, not body retention or provider freshness.

`ListIssueResultsQuery` composes the existing exact Issue/Work association and
independently authorized Factory diagnostic reader. It binds original local
membership, selected Work/Run/collection and original Run version. Version checks
before and after reading prevent mixed Run observations. Changed Runs yield
`stale`; retained associations with unavailable/hidden technical content yield
`unavailable`; readable bounded collections remain `partial`, including empty or
final pages. Missing evidence cannot become invented acceptance. Original
candidate/evidence/Decision subjects and nullable legacy provenance are retained.

Both GET routes are no-store, use closed parameters, shared canonical opaque
cursors (2,048 bytes), current visibility/security binding and independent limits:
timeline 50/default, 200/maximum; results 50/default, 100/maximum. OpenAPI registers
typed visibility for both paginated operations and supplies generated Console
reads. Production runtime installs timeline separately from optional results;
neither GET invokes publication, provider access or execution. Console rendering,
real detailed event instrumentation, journal expansion, Checks/Artifacts and
retention policy remain tasks 6.5–6.8, 6.14 and 8.2 respectively.

## TDD evidence

All Rust commands below use `--offline` and
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_BUILD_JOBS=2`. PostgreSQL tests additionally set the explicitly disposable
`OCTACITY_POSTGRES_URL` and run with `-- --ignored`.

| Command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-application issue_timeline_merges` | Timeline query/store APIs absent | Equal timestamps retain kind/identity order through bounded pages; private bodies excluded and no mutations |
| `cargo test -p octacity-server-application issue_timeline_freezes_execution` | Execution-only timeline returned zero rows instead of an original request | Captured membership excludes later requests and revisions; original boundary survives while current Issue version advances |
| `cargo test -p octacity-server-application issue_timeline_binds_source` | Source-only timeline returned no receipt | Scoped safe receipt appears; pull-generation change marks only Source stale and a fresh traversal captures a new fence |
| `cargo test -p octacity-server-store-postgres --test issues timeline_metadata_matches -- --ignored` | PostgreSQL timeline read API absent | Memory parity for tied metadata, exclusive continuation and restart |
| `cargo test -p octacity-server-store-postgres --test issues timeline_keeps_local -- --ignored` | Independent unavailable-coverage contract absent | Renamed optional Source table cannot abort local pages; unavailable original coverage survives recovery until a fresh traversal |
| `cargo test -p octacity-server-application issue_results_bind` | Result observation API absent | Independent partial coverage, nonduplicated pages, unchanged original provenance after Issue edits, stale continuation after a real Run control, empty evidence without acceptance |
| `cargo test -p octacity-server-store-postgres --test issues timeline_rejects -- --ignored` | Future local membership was accepted | Both adapters classify forged revision/request/discussion bounds and invalid page sizes as invalid input |
| `cargo test -p octacity-server-api-rest --test v1_handlers issue_timeline_http` | Manual timeline GET returned 404 | No-Factory metadata read, no-store, bounded continuation/filter and closed invalid-input handling |
| `cargo test -p octacity-server-api-rest --test v1_handlers issue_activity_openapi` | Activity schemas absent | Closed bounded schemas, exact response/no-store/cursor contracts and no acceptance field |
| `cargo test -p octacity-server-store-postgres --test issues timeline_rejects -- --ignored` (unavailable boundary case) | Memory marked an originally unavailable Source section as changed | Unavailable and changed observations remain distinct in both adapters |

Additional characterization checks cover independent comment membership, current
hidden-Issue denial before metadata reads, cursor reuse across Issue/Project,
security scope, normalized visibility and filters, invalid/oversized cursors,
exact selected Work/collection, denied Factory/Project access and unchanged manual
state. PostgreSQL also qualifies Source partition/policy parity after restart and
backdated execution membership through its real SQL path. Production HTTP
qualification resumes the same timeline cursor after full ServerRuntime restart
with object storage deliberately unreachable and verifies installed result reads
return not-found for an unassociated Work rather than dispatching anything.

Compiler/fixture repairs are not counted as behavioral Red: using numeric version
accessors and valid application-failure variants; populating real budget history
before testing pagination; choosing a policy that accepts the test's verified
security context; moving visibility implementations before the test module to
satisfy Clippy. Documentation and formatting have no executable behavior and are
validated by specification, formatting and diff checks.

## Regression verification

| Command | Result |
| --- | --- |
| `cargo test --offline -p octacity-server-application --lib issue_` | 131 passed |
| `cargo test --offline -p octacity-server-application --lib management_security` | 21 passed |
| `cargo test --offline -p octacity-server-store --lib` | 81 passed |
| `cargo test --offline -p octacity-server-store-postgres --test issues -- --ignored` | 54 passed |
| `cargo test --offline -p octacity-server-store-postgres --test issue_execution --test issue_workflows --test migrations -- --ignored` | Execution 20, workflows 8 and migrations 7 passed |
| `cargo test --offline -p octacity-server-api-rest --lib --test v1_handlers` | 12 library and 144 HTTP tests passed |
| `cargo test --offline -p octacity-server --test issue_discussion -- --ignored` | Restart/body/timeline integration test passed |
| `cargo clippy --offline -p octacity-server-api-rest -p octacity-server-store-postgres -p octacity-server-application -p octacity-server-store -p octacity-server --all-features --all-targets -- -D warnings` | Passed after moving visibility implementations; final REST schema recheck also passed |
| `pnpm --dir ui run api:generate`, `format`, `lint`, `typecheck`, `test:unit`, `build` | Pinned Node 24.21.0/pnpm 12.8.1; 33 script checks, 580 tests/84 files, build and architecture/bundle budgets passed |
| `cargo fmt --all -- --check` | Passed |
| `python3 tools/check_architecture.py` | Passed: 63 packages, 256 dependency edges |
| `openspec validate add-factory-work-card --strict` | Passed |
| `git diff --check` | Passed |

Generated API/build output remains ignored; no dependencies, global toolchains,
lockfiles or unrelated code changed. Rust builds use two jobs without incremental
compilation/debug symbols. Free space remained above 9 GiB. The existing
PostgreSQL image runs with a bounded disposable tmpfs data directory. The temporary
pinned Node distribution was SHA-256 verified against its official manifest.
The disposable PostgreSQL container and temporary Node distribution were removed
after checks. Final free space after cleanup: approximately 9.5 GiB. Task 6.4 is complete: 36 of 56
tasks checked. No commit or push was created.
