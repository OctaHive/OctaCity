# Task 3.4: reviewed execution HTTP contracts

## Scope and boundaries

The generated REST inventory now exposes three optional execution operations:

- `POST /api/v1/projects/{project_id}/issues/{issue_id}/executions`.
- `GET /api/v1/projects/{project_id}/issues/{issue_id}/executions/{request_id}`.
- `GET /api/v1/projects/{project_id}/issues/{issue_id}/executions`.

The POST translates the reviewed body and Issue `If-Match` into the existing
reservation command, then invokes `IssueExecutionPublisher`. That publisher still
uses the single shared `ProjectTextIntake`; HTTP owns no byte publication,
source-specific adapter, replacement Work/Run, recovery worker or compensation
transaction. Manual Issue handlers retain their independent composition.

The closed body selects an exact configuration/repository, source policy,
versioned mapping and ordinary Work classification. Classification participates
in reservation replay before publication and must match the admitted Work before
association. Its optional retained representation preserves legacy JSON/digests;
new HTTP commands require it. Migration 77 permits JSON escaping of the full
existing ordinary metadata allowance within a bounded metadata-only document.

Responses distinguish 201/200 durably associated admission from 202 retained
pending work. All successful POST responses include the exact status `Location`.
The native publication wait is five seconds; shared original leases/deadlines
remain unchanged. Timeout, unavailable dependency and another publication owner
cause a current authorized status read, avoiding a false pending result after an
unknown acknowledgement. Other authorization/input failures retain their safe
errors. GETs never resume publication and use `Cache-Control: no-store` without
an Issue ETag. Existing Run/control and Artifact admission contracts remain valid.

History uses the shared 200-entry Issue history bound and chronological
`(requested_at, request_id)` keys. Migration 76 allocates reservation sequences
under the existing Issue lock and adds scoped watermark/order indexes. Counters
are local to each Issue, so another Issue cannot influence continuation metadata. The first
page fixes membership and the observed Issue revision; later pages keep that
boundary while observing current associations. `request-watermark-v1` is explicit
about this distinction. Canonical opaque cursors bind Project, Issue, security
partition, normalized current visibility and observation version. They contain no
requirements bodies or plaintext security partition. Visibility is checked before
store reads, ordering or continuation metadata; no totals are inferred.

The reference HTTP contract is in `docs/reference/issues.md`. The existing UI
API generator consumes the published OpenAPI inventory without a second schema
or custom execution client. Console behavior remains task 3.5.

## Observed Red/Green slices

Each behavior was tested through an existing public application/store or HTTP
boundary before its implementation. Missing newly required APIs were valid Reds;
fixture/import compilation corrections were not counted as behavioral failures.

| Slice | Observed Red | Observed Green |
| --- | --- | --- |
| Exact status GET | Missing execution management composition API | Pending immutable metadata, no publication, matching closed OpenAPI schema |
| Classification reservation | Reviewed classification API absent | Changed classification with the same key conflicts before preparation |
| Classification publication | Changed review was accepted and admitted | Conflict before any VCS resolution or byte write |
| Reviewed POST | Publication composition API absent | 201, exact request Location and existing Run URL |
| Dependency unavailable | 503 instead of retained 202 | Pending metadata and exact resume Location |
| Bounded HTTP wait | 503 instead of retained 202 | Pending response after the configured wait |
| Application history | History query API absent | Bounded membership, fixed watermark, current visibility and scoped cursor rejection, no mutation evidence |
| PostgreSQL history | Required history port implementation absent | Original bounded chronological membership after store restart and a later request |
| HTTP history | 405 instead of 200 | Bounded closed metadata page, declared observation and invalid parameter/cursor rejection |
| Native composition | Real HTTP history returned 503 | PostgreSQL/S3 request resumes after host restart and an intervening Issue edit; exact original Run survives replay |
| Publication ownership/unknown outcome | Busy publication returned 409; already linked unknown outcome returned 202 | Busy request returns pending; current durable association returns admitted |
| Generated status Location | OpenAPI Location schema was absent | 200/201/202 document the exact status URL and required review headers |
| Reviewed association | Another classification was accepted for the reviewed request | Association rejects it without partial evidence/linkage |
| Full valid classification | PostgreSQL rejected a valid ordinary classification as unavailable | Bounded metadata migration retains and replays the complete reviewed intent |
| Watermark isolation | Both memory and PostgreSQL returned 4 after two visible and two other-Issue requests | Both return the Issue-local boundary 2 and continue exactly its own membership |

Focused commands, using the build settings below:

```sh
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_execution_http_status
cargo test --locked -p octacity-server-application --lib issue_execution_review_freezes_classification
cargo test --locked -p octacity-server-application --lib project_text_intake_issue_publication_recovers
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_execution_http_start
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_execution_http_retains_pending
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_execution_http_returns_pending
cargo test --locked -p octacity-server-application --lib issue_execution_history
cargo test --locked -p octacity-server-store-postgres --test issue_execution execution_history -- --ignored
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_execution_http_history
cargo test --locked -p octacity-server --test project_text_intake issue_execution_http -- --ignored
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_execution_http
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_execution_http_generated_contract
cargo test --locked -p octacity-server-application --lib issue_execution_association_rejects_work_with_another_reviewed_classification
cargo test --locked -p octacity-server-store-postgres --test issue_execution execution_review_accepts_full_valid -- --ignored
cargo test --locked -p octacity-server-store-postgres --test issue_execution execution_watermark_is_issue_local -- --include-ignored
```

The native HTTP scenario first uses an unavailable object endpoint, shuts down
the server, edits the Issue, then starts a fresh host against disposable MinIO.
Reads preserve pending state; the exact original POST resumes old reviewed bytes,
links its original request and returns the same Run on another replay. A distinct
key with the old version conflicts; reviewing the new version creates another
Run with changed task digest. The old Run URL still returns 200 and exactly two
Runs are retained. The pre-existing native shared-input suite covers interruption
and recovery across byte publication, ordinary admission and association.

Additional characterization checks preserve manual create/edit without execution
rights, separately protect execution, reject unreviewed/open command bodies and
exercise every registered route's allowed/denied dispatch inventory. Existing
application admission tests independently deny Project admission, target,
repository and Issue authority before reservation.

## Qualification commands

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
# PostgreSQL and MinIO variables refer only to owned disposable loopback services.
cargo test --locked -p octacity-server-api-rest
cargo test --locked -p octacity-server-store -p octacity-server-application --all-features --lib
cargo test --locked -p octacity-server-application --all-features --lib issue
cargo test --locked -p octacity-server-store-postgres --test issue_execution --test issues --test migrations -- --include-ignored --test-threads=2
cargo test --locked -p octacity-server --test project_text_intake -- --ignored --test-threads=2
cargo test --locked -p octacity-server --lib runtime::issue_tests -- --ignored
cargo clippy --locked -p octacity-server-api-rest -p octacity-server-application -p octacity-server-store-postgres -p octacity-server --all-targets --all-features -- -D warnings
cargo run --quiet --locked -p octacity-server-api-rest --example export_openapi > /tmp/octacity-3.4-openapi.json
# From ui/:
OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-3.4-openapi.json pnpm api:generate
pnpm typecheck
node --test scripts/factory-authoring-schemas.test.mjs scripts/issue-workflow-constraints.test.mjs scripts/issue-constraints.test.mjs
# From repository root:
cargo fmt --all --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
openspec validate add-factory-bugfix-quickstart --strict
git diff --check
```

## Qualification results

| Qualification | Result |
| --- | --- |
| REST library, DTO/route and contract suites | 12 + 17 + 25 + 115 = 169 passed |
| Full application/store libraries, all features | 529 application + 81 store passed |
| Issue application regression after final watermark isolation | 47 passed |
| Execution persistence conformance after final watermark isolation | 11 passed: 10 PostgreSQL + 1 memory |
| PostgreSQL Issue regression | 24 passed |
| PostgreSQL migration/upgrade/rollback regression | 6 passed |
| Native shared publication/recovery/HTTP/S3 suite after final watermark isolation | 11 passed, including the subprocess fixture |
| Native manual Issue journey without Factory/providers | 1 passed |
| Generated client and UI contract checks | API generation, TypeScript typecheck and 8 generator tests passed |
| Clippy, four affected packages, all features/targets | Passed with `-D warnings`, repeated after watermark isolation |
| Formatting and whitespace | `cargo fmt --all --check` and `git diff --check` passed |
| Architecture | 63 packages, 256 dependency edges passed |
| OpenSpec | Work-card and quickstart strict validation passed |

The full library run began before the final Issue-local watermark correction;
the final source was then rechecked through all 47 Issue application tests,
backend conformance, PostgreSQL migration/regression, the full native suite and
Clippy. The final counter change does not affect Factory execution logic.

Documentation has no executable behavior; its validation uses the schema,
reference checks and strict OpenSpec validation.

## Resource use and remaining scope

Builds reuse the existing target with incremental/debug data disabled and two
compiler jobs. Free disk was checked around compilation (approximately 14–15 GiB).
Disposable services use bounded tmpfs storage and cached images; no image pulls,
release bundles or unrelated cache deletion were used. Both task-owned containers
were removed after qualification; approximately 14 GiB remained free.

Only task 3.4 is in scope. Console interactions, discussion implementation and
detailed execution/result journals remain their existing later tasks. This work
creates no commit without a new explicit request.
