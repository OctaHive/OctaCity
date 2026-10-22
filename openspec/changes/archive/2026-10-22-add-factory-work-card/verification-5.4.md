# Task 5.4: synchronization reads and Console feedback

## Scope and shared boundaries

Provider-neutral retained reads compose original Issue desired-state publication
with the existing Reporter journal. No second outbox, worker, native state machine,
provider SDK or dispatch-on-GET path is introduced. Manual tasks and ordinary
Issue services have no new provider prerequisite. A separately installed query
uses Issue visibility, native security partition, the common history bound and
opaque cursor codec. Live pagination declares its observation contract and makes
no aggregate synchronization or execution-acceptance claim.

Safe items expose original revision, mapping, native correlation, workflow/status,
finite desired effects, exact optional accepted pipeline provenance, resolution
lineage and independently latest Reporter receipt/state/reason/time. Private
grants, replay keys, security scope, bodies and provider diagnostics are excluded.
The existing Source freshness record retains qualified native status/managed
labels with their exact receipt and original mapping. Legacy missing values stay
unavailable. The migration adjusts that record's finite typed JSON bound for
supported escaped values without another observation journal.

Console uses generated contracts, common query notices and the existing mapping
editor/confirmed-command mechanism. It keeps local business metadata, historical
execution and technical delivery distinct. Resolution reviews exact receipt and
current policy; stale/failed refresh disables a new confirmation while retaining
visible data. No force control or automatic provider refresh is added. Unsubmitted
values and command keys remain in component memory. Detailed execution/result
panels remain the separate section 6 tasks rather than fabricated acceptance UI.

## TDD evidence

Rust commands use `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_BUILD_JOBS=2` and `--offline`. Ignored PostgreSQL
tests use an explicitly owned disposable PostgreSQL 18.6 container
`octacity-issue-sync-54`, with 512 MiB memory and 384 MiB tmpfs. Compilation repairs,
fixture authorization mistakes and React observer-flush timing are excluded from
Red evidence.

| Public boundary and command/filter | Red | Green |
| --- | --- | --- |
| `cargo test --offline -p octacity-server --all-features --lib issue_synchronization_reads_original -- --ignored` | Required retained query API absent | Original pending effect reads safely without provider call, Issue mutation or execution |
| `cargo test --offline -p octacity-server-application --lib issue_source_status_pull_uses` | Serialized native values absent | Exact accepted native status/mapping retained; echo adds no Issue revision or outgoing action |
| `cargo test --offline -p octacity-server-api-rest --test v1_handlers issue_source_http_projections` | Optional HTTP query composition API absent | Bounded/no-store read, typed safe provenance schema and invalid cursor/limit/force rejection |
| `pnpm --dir ui exec vitest run src/features/issues/IssueSynchronization.test.tsx` | Outgoing synchronization region/pending feedback absent | Desired effects and independent delivery feedback visible without blocking requirements editing |
| `pnpm --dir ui exec vitest run src/features/issues/sourceApi.test.ts` | Projection transport method absent | Generated read/resolve paths, bounded page and exact version/idempotency/receipt forwarding |
| Console synchronization test, resolution scenario | Explicit resolution review unavailable | Existing mapping editor submits the exact failed receipt and reviewed policy through resolution, without ordinary mapping publication |
| Console synchronization test, native-values scenario | Last reconciled native values/direction unavailable | Values, original provenance and field direction visible without provider I/O |
| `cargo test --offline -p octacity-server-store-postgres --test issues synchronization_retains_supported -- --ignored` | Supported escaped managed labels exceeded the prior freshness-document bound | Same values/receipt/mapping survive reconstruction, unrelated labels excluded and echo leaves revision/history intact |
| `cargo test --offline -p octacity-server-application --lib issue_source_status_mapping_cannot_reinterpret` | A cached pre-publication read was labeled with new-policy native values | Only observations after that policy publication can qualify those retained values |

## Regression and repository checks

All final affected suites passed:

| Command | Result |
| --- | --- |
| `cargo test --offline -p octacity-server --all-features --lib runtime::work_source_tests::reporting -- --include-ignored --test-threads=2` | 57 passed, including scoped pagination, original mapping/receipt lineage, revoked/disabled access, unknown-response recovery and fencing |
| `cargo test --offline -p octacity-server-store-postgres --test issues --test issue_execution --test migrations -- --ignored --test-threads=2` | 43 Issue, 19 execution and 7 migration tests passed |
| `cargo test --offline -p octacity-server-application --lib issue_` | 107 passed, including configured accepted Issue action/projection provenance and guarded source echoes |
| `cargo test --offline -p octacity-server-application --lib source_` | 143 passed |
| `cargo test --offline -p octacity-server-store --lib` | 81 passed |
| `cargo test --offline -p octacity-server-api-rest --lib --test v1_handlers` | 12 library and 137 HTTP tests passed, including the complete authorization/route/OpenAPI inventories |
| `cargo test --offline -p octacity-server --all-features --lib runtime::issue_tests -- --ignored --test-threads=2` | 2 manual/no-provider runtime journeys passed |
| `cargo clippy --offline -p octacity-server -p octacity-server-application -p octacity-server-store-postgres --all-features --all-targets -- -D warnings` | Passed |
| `pnpm --dir ui run api:generate`, `format`, `lint`, `typecheck` | Passed |
| `pnpm --dir ui run test:unit` | 33 Node checks and 580 Vitest tests across 84 files passed; all 33 Node checks passed again after the measured budget update |
| `pnpm --dir ui exec playwright test tests/browser/issues.spec.ts tests/browser/issue-executions.spec.ts tests/browser/issue-sources.spec.ts` | 8 passed, including English/Russian feedback, manual creation, historical selection, keyboard and 390 px layout |
| `pnpm --dir ui run build` | Passed with architecture and bundle checks |

Console characterization covers pending/claimed/delivered and each finite
terminal reason, stale visible receipts and disabled fresh confirmation after a
refresh failure. A real PostgreSQL/Reporter conflict followed by explicit repair
retains both original receipts with one unchanged Issue revision; pagination
does not substitute the current mapping. Another security partition cannot read
the original history or reuse its cursor; changed visibility denies before
projection. Existing accepted-action and Reporter recovery tests preserve the
original completed action/Flow while delivery remains independently pending or
fails. This is retained-contract/process-peer qualification, not a live external
provider deployment or the future detailed result-panel implementation.

The first complete Console build exposed insufficient aggregate asset headroom.
An isolated HEAD build passed and comparison measured approximately 10 kB of
additional JavaScript for synchronization rendering and the two locale catalogs,
with no new dependencies. The new measured-output scenario was observed Red via
`node --test --test-name-pattern='admits measured synchronization feedback' ui/scripts/verify-architecture.test.mjs`.
Both aggregate budgets were increased by 8 KiB, while entry JavaScript, CSS and
asset-count ceilings stayed unchanged. The scenario was consolidated into the
existing measured-task budget test to avoid duplicate guards; the complete Node
suite passed with both accepted measured output and rejection of another 8 kB
unplanned addition. Final built JavaScript is 1,078,852 bytes and total static
output 1,164,951 bytes, below the explicit updated bounds. No architecture check
was disabled.

Formatting/fixture compilation, authorization-inventory expected-call maintenance
and asynchronous React observer flushing were corrected without counting them as
behavior Red. `cargo fmt --all -- --check`, `python3 tools/check_architecture.py`
(63 packages, 256 edges), OpenSpec strict validation and `git diff --check` are
the final repository checks. Documentation uses specification/diff checks rather
than executable TDD. The macOS linker emits its existing large `__eh_frame`
warning; linking, behavioral suites and Clippy pass. The owned PostgreSQL container
and isolated HEAD bundle-comparison checkout were removed. Final workspace free
space is 10 GiB; builds used no incremental cache or debug symbols. Task 5.4 is
complete, with 32 of 56 change tasks checked. No commit is created.
