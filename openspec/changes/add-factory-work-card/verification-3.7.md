# Task 3.7: durable Issue-status command recovery

## Scope and owners

PostgreSQL now implements accepted pipeline moves through the existing
`IssueStore::transition_issue` and `IssueRevision::transitioned` policy. Manual
command digests and receipt encoding are preserved. The existing transactional
publisher owns revision, history, audit, outbox and replay publication; there is
no second transition handler, outbox, execution journal or recovery worker.

Migration 78 adds applicability to the existing Work association and backfills
from the original reviewed requirements revision. It never blesses the latest
head or invents authority for legacy Artifact associations. The Issue lock
serializes competing moves; the same transaction advances only the winning
Work's marker. Sequential moves from its own lineage remain possible.

The existing scoped idempotency record seals original guard, selection,
provenance and move time before business publication. A transient failure leaves
that original command pending; definitive business rejection is retained without
status/history/audit/outbox mutation. Ordinary mutations retain their previous
pending semantics. A read-only recovery port returns the same sealed metadata
after failure, success or rejection, scoped to Project, Issue and security
partition. Changed reuse conflicts; successful replay returns its historical
receipt after later edits. Memory and PostgreSQL preserve the original move time.

`RetainedIssueStatusCommand` stores safe facts/references, never evidence-authority
flags or raw evidence. Its shared qualification path checks the exact Work/Run
and reuses canonical journal evidence validation for both first execution and
recovery. A candidate without selected evidence cannot become accepted provenance.
Migration 79 extends the bounded history envelope to 64 KiB so the supported
256-record inventory fits; it does not admit evidence bodies or unbounded history.

`FactoryIssueStatusAction` executes through the existing trusted-action adapter
and registry, installed explicitly by `FactoryRuntimeResources::issue_status_executor`.
The host supplies accepted attribution, current policy and clock. Binding
parameters select an exact workflow/version/edge and declared outcome; projected
input carries only optional candidate/evidence references. The common adapter
supplies the exact persisted attempt and stable operation identity. Only records
explicitly selected by that frozen input may support evidence. The receipt
schema/budget is checked before publication, current visibility/rights and
execution windows are rechecked, and the common owner retains the node result.
Cancellation does not publish a pending status or undo a committed one.

No provider, Reporter, paid model or replacement Work/Run is required. Ordinary
manual Issues remain independent. Flow/action REST and Console authoring remain
task 3.8; this slice adds no public pipeline-status mutation endpoint.

## Observed Red/Green

Baselines: 24 PostgreSQL Issue tests, 14 application accepted-action tests,
20 common Flow-action tests and the native shared Issue-publication scenario
passed before the relevant shared changes. Fixture/schema/import corrections
and build failures unrelated to the new behavior were excluded from Red.

All Cargo commands used the resource settings below. Application filters use
`cargo test --locked -p octacity-server-application --lib`; PostgreSQL filters
use `cargo test --locked -p octacity-server-store-postgres --test issue_execution`
with `-- --ignored`. Native filters use
`cargo test --locked -p octacity-server --test project_text_intake` with `-- --ignored`.

| Filter / slice | Observed Red | Observed Green |
| --- | --- | --- |
| `pipeline_status_missing_association` | Unsupported/unavailable instead of a safe Issue conflict | No association, move, history or guessed Issue |
| `pipeline_status_manual_move` | Freshly retargeted old Work incorrectly moved the current Issue after a manual move | Original applicability rejects it after restart |
| `pipeline_status_rejected` | A rejected command could rebind the same operation to the new head | Original failed command/condition stays retained |
| `pipeline_status_commit_failure` | Original-command recovery API absent | Atomic rollback, scoped original metadata, original-time retry and one receipt |
| `pipeline_status_native` | Installed local Issue action API absent | Lost acknowledgement after business commit recovers through the common node owner after later edits; revoked rights block acknowledgement |
| `pipeline_status_native` — native composition | Runtime installation method absent | Existing registry installs the exact local plugin without contacting an unavailable object endpoint or a Reporter |
| `issue_pipeline_action_pending_retry` | Memory replaced original time 55 with retry time 100 | Memory preserves original time, matching PostgreSQL |
| `pipeline_status_history_retains` | Valid full reference inventory violated the former 16 KiB history constraint | Bounded historical provenance with 256 digests remains readable |
| `issue_pipeline_action_restore_rejects` | Candidate-only metadata restored without selected evidence | Shared initial/recovery qualification rejects unqualified provenance |

Additional public-boundary regressions verify sequential moves, historical
receipt/provenance replay after requirements edits, changed attempt rejection,
parallel Work with one winning move and an unadvanced losing marker, repeatable
upgrade from schema 77, and preservation of applicability for unrelated priority
changes while changed requirements remain invalid after upgrade.

Fault injection interrupts the shared transaction after revision/history/audit
work by rejecting its outbox publication. Another test holds a database advisory
lock at revision insertion and terminates the actual blocked PostgreSQL backend:
no partial history survives, the sealed command remains, and a new store recovers
it. Database synchronization observes the blocked statement instead of relying
on sleep. The native test loses the action acknowledgement after the Issue commit,
edits requirements, constructs a new store/installed executor, revokes/restores
current rights and verifies one original status receipt and one common node result.

## Qualification commands

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
# OCTACITY_POSTGRES_URL names only the task-owned disposable loopback service.
cargo test --locked -p octacity-server-application -p octacity-server-store --all-features --lib
cargo test --locked -p octacity-server-store-postgres --test issue_execution --test issues --test migrations -- --include-ignored --test-threads=2
cargo test --locked -p octacity-server --test project_text_intake -- --include-ignored --skip native_resources_publish_and_read_project_inputs_through_the_shared_mechanism --skip issue_execution_http_resumes_original_inputs_after_runtime_restart --test-threads=2
cargo test --locked -p octacity-server --test project_text_intake pipeline_status_native -- --ignored
cargo test --locked -p octacity-server-api-rest
cargo clippy --locked -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres -p octacity-server-api-rest -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check
```

| Qualification | Result |
| --- | --- |
| Final application/store libraries, all features | 545 application + 81 store tests passed |
| PostgreSQL execution, Issue and migration suites | 20 + 24 + 6 = 50 passed; all 9 status scenarios also passed after final codec qualification |
| Native shared publication/recovery suite | 10 passed, including subprocess recovery; the local status/rights/acknowledgement scenario also passed after final codec qualification |
| Native manual Issue journey without Factory/providers | 1 passed |
| REST library, route, DTO and contract suites | 12 + 17 + 25 + 115 = 169 passed |
| Clippy, five affected packages, all features/targets | Passed with `-D warnings` after the final source changes |
| Formatting/whitespace | `cargo fmt --all -- --check` and `git diff --check` passed |
| Architecture | 63 workspace packages, 256 dependency edges passed |
| OpenSpec | Strict work-card validation passed |

The earlier full library run passed 544 + 81 tests before the final candidate-only
qualification case; the final complete run above includes that case and its
implementation. Additional exact commands used for final manual/native and
PostgreSQL status qualification were:

```sh
cargo test --locked -p octacity-server --lib runtime::issue_tests -- --ignored
cargo test --locked -p octacity-server-store-postgres --test issue_execution pipeline_status -- --ignored --test-threads=2
```

The two unchanged S3-dependent native scenarios were not rerun: this step changes
no object-store or HTTP-host composition. UI sources and HTTP inventory are
unchanged. Documentation has no executable behavior; strict OpenSpec and
whitespace/reference checks apply.

## Resources

The disposable PostgreSQL service uses an already cached image, a 512 MiB memory
limit and 384 MiB tmpfs. No image pulls, new dependencies or unrelated cache
deletions were required. Builds reused the existing target with debug/incremental
data disabled and two compiler jobs; free disk was monitored (about 13 GiB at
start and 9–10 GiB during qualification). The task-owned PostgreSQL container was
removed after database/native checks; about 10 GiB remained free. No commit is
created without a new explicit request.

The pending-command audit/outbox timestamp correction is qualified in the
[task 3.8 review corrections](verification-3.8.md#review-corrections).
