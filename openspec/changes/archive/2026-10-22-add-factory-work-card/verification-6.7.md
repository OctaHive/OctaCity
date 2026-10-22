# Task 6.7 — exact execution journal reads

## Scope and public boundaries

The journal composes existing Issue execution projections, version-fenced 6.4
Factory diagnostic pages, exact Build/Attempt/Job lookup and the existing Job
event reader. No persistence, migration, executor, provider integration or input
publication was added. Manual Issue creation remains independently composed.
Console journal rendering remains task 6.8.

Public tests exercise authorized management queries, HTTP/OpenAPI responses and
the real ServerRuntime with a disposable PostgreSQL database. Existing exact-Job
and timeline behavioral tests passed before extracting shared cursor scope logic.
All Rust commands below use `--locked --offline`, one owned temporary target,
`CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0` and
`CARGO_PROFILE_TEST_DEBUG=0`.

## Observed Red → Green

Application command prefix:
`cargo test --locked --offline -p octacity-server-application --lib`.

| Test filter | Observed Red | Observed Green |
| --- | --- | --- |
| `issue_execution_journal_reads_selected` | Exact selected journal query API absent | Preparing and admitted original request read without writes |
| `issue_execution_journal_events_reconnect` | Safe event query/composition API absent | Exact command/file rows, original sequences/times, private bodies excluded, bounded reconnect without replay |
| `issue_execution_journal_marks_retained_gaps` | Missing sequence interval omitted | Leading and fully expired tail ranges explicit; resumed cursor does not repeat them |
| `issue_execution_journal_keeps_measured_zero` | Actual lifecycle sample projected as Not reported | Typed lifecycle/resource samples preserved, nullable missing accounting distinct from zero; prose token claims absent |
| `issue_execution_journal_preserves_progress` | Event-store outage discarded the page | Original Job progress and cursor retained with unavailable coverage; no invented gap |
| `issue_execution_journal_binds_continuation` | Job-scoped event grant rejected by Issue-specific scope translation | Typed Job grant works; different security partition, visibility and foreign Work continuation rejected |
| `issue_execution_journal_detail_never` | Exact body query API absent | One independently authorized original sequence; retention cannot substitute the next row |
| `issue_execution_journal_events_reconnect` (live append extension) | No stale Job-snapshot marker | Live event advance marks stale metadata and preserves continuation |
| Same filter (stale reconnect extension) | A previously issued cursor was rejected while the Job snapshot lagged | Existing retained stream cursor validates reconnect without replay or a second read |
| Same filter (live body extension) | The emitted live row could not expand until Job metadata caught up | Exact independently authorized body comes from the retained event reader while metadata is stale |

REST command prefix:
`cargo test --locked --offline -p octacity-server-api-rest --test v1_handlers`.

| Test filter | Observed Red | Observed Green |
| --- | --- | --- |
| `issue_journal_http_publishes` | Journal composition/public selection API absent | Three exact GET contracts, generated closed schemas/bounds, safe metadata, original links and separate no-store body |
| `job_event_parameters_and_page` | Existing common event reader omitted cache-control | Existing response body retained; HTTP and OpenAPI declare no-store |
| `issue_journal_http_invalid_sequence` | Malformed/overflow sequence returned a non-JSON extractor error | Common bounded JSON `invalid_request` response |

Initial malformed event-fixture envelopes and missing test imports were repaired;
they are not behavioral Red evidence. Before sharing the scope hash,
`issue_timeline` passed all four existing tests. New authorization
characterizations additionally deny each Issue/Run/Project/Build/Job/event owner
before event-store lookup and verify current event rights on both HTTP reads.

## Qualification limits

Metadata contains only the shared validated public operation DTO, actual Agent
lifecycle/resource samples or explicit Not reported. Raw bodies remain behind
independent exact-event authorization. Original model/profile and enrolled
worker facts remain distinct. Common completion/stop/budget records expose
settled accounting separately from reservation. Missing model token/cache/cost
breakdowns cannot be inferred from operation progress, arbitrary payloads, prose
or an empty page. Resource sample milliseconds/bytes are not model usage.

The Job stream cursor is scoped to the original association, Build-execution,
Job, security partition and normalized typed visibility. Collection coverage
retains the existing Run version fence. These observations are independent and
never imply a complete result or successful acceptance.


Runtime command:
`cargo test --locked --offline -p octacity-server --test issue_execution_journal -- --ignored`
with `OCTACITY_POSTGRES_URL` pointing only to the owned disposable PostgreSQL 18.6
container. The public Store port reserves a pending request after manual HTTP
Issue creation; no Factory or provider is configured and object storage is
unreachable. Before runtime composition, the journal returned **503 instead of
200**. After installing the existing authorized services, it returned **200**
with no-store, the original pending request and no association, identically
after restart. Issue content/version stayed unchanged. This is a read/restart
qualification, not a new admission or live provider qualification.


## Regression and repository checks

- `cargo test --locked --offline -p octacity-server-application --lib issue_`:
  **139 passed**, including all eight journal behavioral tests, manual Issue,
  workflow, Source and execution regressions after the stale-reader fixes.
- `cargo test --locked --offline -p octacity-server-application --test job_event_long_poll`:
  **6 passed**, including restart, replay, lost notification and typed visibility.
- `cargo test --locked --offline -p octacity-server-api-rest --lib --test v1_handlers`:
  **12 library + 147 HTTP tests passed**. The common matrix covers all 160
  management operations, including authorization before application dispatch;
  response fixtures validate against generated closed schemas and route drift.
- `cargo test --locked --offline -p octacity-server --test issue_execution_journal --test issue_discussion -- --ignored`:
  **2 PostgreSQL/runtime tests passed**, including restart and independent manual
  creation/discussion with no configured Factory/provider and unreachable object
  storage. The disposable PostgreSQL service uses a bounded 512 MiB tmpfs.
- `python3 tools/check_architecture.py`: **63 packages / 256 edges**, no violations.
- `python3 -m unittest tools.tests.test_check_architecture`: **32 passed**.
- Exported current OpenAPI with
  `cargo run --quiet --locked --offline -p octacity-server-api-rest --example export_openapi`.
  With pinned Node **24.21.0** and pnpm **12.8.1**, regenerated the ignored Console
  client using `OCTACITY_OPENAPI_SCHEMA_FILE=<export> pnpm api:generate`.
  `pnpm typecheck` and `pnpm build` passed, including architecture/bundle budgets.
  No new dependency or Console UI behavior was added.
- `pnpm format`, `pnpm lint` and `pnpm test:unit`: **33 script + 600 component
  tests passed**. Existing UI behavior remains covered; journal UI is task 6.8.
- `cargo clippy --locked --offline -p octacity-server-application -p octacity-server-api-rest -p octacity-server --all-features --all-targets -- -D warnings`: passed after the stale-reader fixes.
- `cargo fmt --all -- --check`, `git diff --check` and
  `openspec validate add-factory-work-card --strict`: passed.

Documentation-only updates describe the implemented contracts; they have no
executable behavior to drive through TDD. Behavioral production changes use the
Red/Green cases above. No commit or push is part of this task.


## Disk and cleanup

Disk space was checked throughout builds, reaching approximately **10 GiB** free
at the lowest observed point. Compilation used one owned temporary target with
two jobs, no incremental artifacts and no debug information. After checks, only
that target, the owned temporary Node runtime and the disposable PostgreSQL
container were removed. User caches, source files and unrelated build directories
were preserved. Final observed free space: approximately **14 GiB**.

Task 6.7 is checked complete; the change is **39/56** tasks complete. Task 6.8
Console journal journeys remain unchecked.
