# Task 3.6: explicit accepted pipeline Issue-status contracts

## Boundary and scope

`ApplyIssuePipelineStatusCommand` passes trusted accepted producer data to the
existing `IssueStore::transition_issue` port. Manual commands use the same
transition policy and transactional publisher. There is no provider dependency,
Reporter requirement, automatic completion-to-status mapping, new execution
journal or second transition handler. Ordinary manual creation remains intact.

The accepted input is not deserializable as a management request. It pins an
explicit workflow/version/edge/outcome and retains the original Issue,
requirements, Work/Run and action provenance. The trusted producer is responsible
for accepting the producing Flow/cycle/attempt; the Issue boundary checks the
retained Work association and original reviewed requirements independently.
Generic Flow integration is not installed by this contract-first task.

The in-memory backend keeps applicability beside the existing Work association
under the Issue lock. Only the winning Work advances its own marker; manual
status moves, status moves from another execution, requirements changes and
workflow adoption invalidate older markers, including changes away and back.
Current source ownership is checked independently before a new move: source-owned
Status blocks manual/pipeline replacement. The source-policy fixture changes
ownership without advancing the marker. The specification also requires
invalidation for committed source status moves; source-transition integration
remains outside this task. Reads, new execution requests and unrelated metadata
changes alone preserve applicability. No requirements authority is invented for
legacy Artifact associations.

Exact evidence qualification reuses `FlowDataHistory::validate` and canonical typed
Decision/Evidence discovery facts. Only selected records for the original
Work/Run/candidate grant the supported evidence requirements. Passed required
checks need a nonempty manifest and all gates passed. Freshness is checked at
the move time. History stores safe provenance/digests, not raw results.

Accepted commands retain their original intent in the existing transition replay
map before evaluation. Definitive rejection cannot be retargeted. Atomic failure
leaves a resumable original command without partial business state or mutation
evidence. Successful replay returns its historical result while checking current
authorization. Work/Run results remain immutable.

PostgreSQL explicitly fails closed for pipeline commands in this slice. Durable
applicability, retained-command restart recovery and runtime composition are task
3.7. Generic authoring and controls are task 3.8. These tasks remain unchecked.

## Observed Red/Green

Before shared-policy changes, all 34 existing Issue behavioral tests passed.
The existing three Factory evidence/action tests also passed before their fixture
was extended to supply exact retained records. Environment or fixture errors and
an accidental zero-test filter were excluded from Red evidence.

Each command below used the resource settings in the qualification section and
the common prefix:

```sh
cargo test --locked -p octacity-server-application --lib
```

| Filter / slice | Observed Red | Observed Green |
| --- | --- | --- |
| `issue_pipeline_action` — shared local move | Missing accepted-action and application command APIs | 1 passed: explicit pipeline origin, unchanged content/Work association and no execution dispatch |
| `issue_pipeline_action` — pinned selection | Missing typed workflow/edge/outcome selection API | 2 passed: retargeted edge and mismatched outcome rejected |
| `issue_pipeline_action` — lineage | Missing exact Issue/requirements provenance API | 4 passed: original reviewed association checked; sequential moves and historical replay retained |
| `issue_pipeline_action` — rejected-command recovery | A stale rejected command was accepted after rebinding it to the new head | 5 passed after sealing original intent in the existing replay map |
| `issue_pipeline_action` — exact evidence | Missing candidate and retained-evidence qualification API | 9 passed with real accepted/unqualified/failed journal records |

Further contract checks passed through public application/store boundaries:
manual/content/status changes away and back; workflow adoption away and back;
parallel execution and its winning move; unchanged reads and priority edits;
current rights and visibility; source-owned Status; absent/legacy association;
wrong Run; changed Flow/cycle/attempt metadata on replay; unrelated, unselected,
unknown and expired evidence; and atomic commit interruption with exact recovery.
Qualified completion/Decision without an explicit action leaves Issue unchanged.

The final pre-refactor Issue suite passed **61 tests**, including **14** new
accepted-action tests. After that baseline, manual availability and authoritative
manual/pipeline mutation were refactored to one shared eligibility implementation.

## Qualification

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
cargo test --locked -p octacity-server-application -p octacity-server-store --all-features --lib
cargo test --locked -p octacity-server-api-rest
cargo test --locked -p octacity-server-store-postgres --test issues --no-run
cargo clippy --locked -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres -p octacity-server-api-rest --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check
```

| Qualification | Result |
| --- | --- |
| Full application/store libraries, all features | 543 application + 81 store tests passed |
| REST library, route, DTO and contract suites | 12 + 17 + 25 + 115 = 169 tests passed |
| PostgreSQL Issue integration test compilation | Passed; no database execution or restart recovery claimed |
| Clippy, four affected packages, all features/targets | Passed with `-D warnings` |
| Formatting and whitespace | `cargo fmt --all -- --check` and `git diff --check` passed |
| Architecture | 63 workspace packages, 256 dependency edges passed |
| OpenSpec | Strict work-card validation passed |

The full library run includes all 14 new accepted-action tests after the common
eligibility refactor, as well as existing manual Issue and Factory journeys.
PostgreSQL behavioral recovery remains task 3.7. UI source and HTTP inventory
were unchanged by 3.6; REST regression verifies their existing boundaries.
Documentation-only changes have no executable behavior and are validated with
strict OpenSpec and whitespace/reference checks rather than an artificial test.

## Resource use

Builds reuse the existing target with incremental/debug data disabled and two
compiler jobs. Approximately **14 GiB** remained free during qualification. No
dependencies, Docker services, image pulls, release bundles or unrelated cache
deletions were needed. No commit is created without a new explicit request.
