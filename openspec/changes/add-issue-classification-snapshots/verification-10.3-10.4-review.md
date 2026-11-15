# Tasks 10.3–10.4: review corrections

## Frozen invocation baseline

The PostgreSQL selected-field fence no longer rebases an automatic invocation onto
a later pipeline receipt. It compares every supplied selected field against the
actual captured revision, including intermediate change-and-restore history. The
explicit action's existing receipt-order policy and original-operation replay remain
separate. The rejected result creates no partial Issue update.

Observed Red: `selected_field_invocations_cannot_rebase_over_a_competing_saved_result`
accepted the second different result assessed at the same old baseline and replaced
Priority. Green rejects it, preserves the first result, retains the conflict after
store restart and replays the first operation's original receipt.

## Current admission freshness

Snapshot validators now require the current trusted admission observation explicitly.
Manual/Source owners supply the guard time after asynchronous observation/material
reads. The original reservation/admission timestamp stays unchanged for identity and
replay. PostgreSQL and the in-memory owner use the same temporal validation.

Observed Red: the real review/request/publication journey qualified an actual
retained command proof, advanced the trusted clock to `fresh_until` while reading
it and incorrectly admitted using the original reservation time. Green rejects that
fresh admission, leaves the preparation pending with its original `reserved_at`,
and permits recovery with valid current qualification. Already admitted replay
continues to return the original Work/snapshot without refreshing evidence.

## Consumed material dependencies

Automatic saves retain original comment/document references from the exact consumed
Issue-snapshot projections and matching frozen Artifact Context. The resolver follows
accepted-result and frozen input/caller references through the existing Flow journal.
It reuses the same history ceiling as frozen-input validation, not newly appended
results. Payload results carry their producing content dependencies; raw input views
follow only the selected subtree and never implicitly forward Context.

The existing bounded mapping walker exposes copied-content pointers for a selected
output subtree and follows active `if_present` branches. Constants and existence tests
alone copy no material body. No mapping grammar, persisted lineage format, provider
adapter or operator setting is introduced. The existing Issue-material selector is
shared by snapshot publication and metadata-only applicability reconstruction. Original
Issue revisions and the exact configuration supply identities/order; no fresh body or
latest-publication lookup invents a dependency. Unrelated approved material is not pinned.

Observed Red: `ordinary_form_retains_only_consumed_document_dependencies_and_rejects_replacement`
accepted ordinary analysis but saved an empty material dependency list. Green retains
only the consumed document, permits removing an unselected document, rejects removal
of the consumed document during analysis without partially saving values, and marks
a saved assessment incompatible after its consumed document is removed. The accepted
analysis record remains intact in both outcomes.

The projection case `projection_dependencies_follow_only_active_selected_material_paths`
observed Red for the missing selection API, then Green for active/fallback paths,
exact array items, untouched sibling inputs and missing output fields. Existing
Incoming and accepted-record owner cases also characterize the exact resolved read
identities through `FlowDataHistory::input_reads`.

## Commands

All Cargo commands use bounded build artifacts and the task-owned disposable PostgreSQL:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export OCTACITY_POSTGRES_URL=postgres://octacity:octacity-test@127.0.0.1:15439/postgres

cargo test --locked -p octacity-server-store-postgres --test issue_execution selected_field_invocations_cannot_rebase -- --ignored --nocapture
cargo test --locked -p octacity-server-application --lib direct_issue_snapshot_requires_current_trusted_observation -- --nocapture
cargo test --locked -p octacity-server-factory --lib projection_dependencies_follow -- --nocapture
cargo test --locked -p octacity-server --test factory_configured_flow ordinary_form_retains_only_consumed -- --ignored --nocapture
```

Each command observed Red for the required behavior/API before its implementation,
then Green. Initial compilation/setup errors were corrected and are not counted as
behavioral Red. The two public Issue-save tests use the real PostgreSQL owner; model
execution is controlled through the existing ordinary Build fixture, not a write
shortcut. The expiry journey uses the actual retained-command qualification reader
with controlled clock advancement during asynchronous reading.

Final targeted regression results follow below.

| Command with the exports above | Result |
| --- | --- |
| `cargo test --locked -p octacity-server --test factory_configured_flow ordinary_form_ -- --ignored --nocapture` | 4 passed |
| `cargo test --locked -p octacity-server-store-postgres --test issue_execution -- --ignored --nocapture` | 37 passed |
| `cargo test --locked -p octacity-server --test project_text_intake snapshot -- --ignored --nocapture` | 17 passed |
| `cargo test --locked -p octacity-server-factory --lib` | 354 passed |
| `cargo test --locked -p octacity-server-application --lib snapshot -- --nocapture` | 63 passed, including delayed proof expiry |
| `cargo test --locked -p octacity-server-application --lib factory::authoring::tests::factory_configuration_cqrs_tests -- --nocapture` | 74 passed |
| `cargo test --locked -p octacity-server --lib issue_snapshot -- --ignored --nocapture` | 18 passed |
| `cargo clippy --locked -p octacity-server-factory -p octacity-server-store -p octacity-server-application --lib -- -D warnings` | passed |
| `cargo clippy --locked -p octacity-server-store-postgres -p octacity-server --lib -- -D warnings` | passed |
| `cargo fmt --all --check` | passed |
| `git diff --check` | passed |
| `python3 tools/check_architecture.py` | passed: 64 packages / 264 dependency edges |
| `openspec validate add-issue-classification-snapshots --strict` | passed |

The service `octacity-review-fixes-postgres` is task-labelled and separate from a
user's stand. It was stopped after verification. These are targeted checks, not a
full-workspace or live-provider qualification. Existing application lib-test helper
warnings and the macOS large-unwind linker warning remain visible in the logs;
production library Clippy with denied warnings passed. Documentation-only changes
were checked through formatting/diff validation and OpenSpec; TDD does not apply to
non-executable prose. No commit was created.


## Follow-up: producer Context across a newer consuming baseline

The Context lookup now uses each producing input's captured Issue identity/version,
not the final field writer's newer baseline. The existing immutable revision cache
loads that original material selection once. The destination baseline, source
ownership, requirements applicability and current material fences remain unchanged.
No schema, endpoint, operator setting or provenance format was added.

Public PostgreSQL regression:
`ordinary_form_consumer_rejects_a_producers_context_removed_between_invocations`.
The existing ordinary agent fixture now supports two sequential reasoning steps with
explicit accepted-result input and an aggregate budget for both. It retains only the
admission/schema/input fields the fixture actually uses; model execution still goes
through the existing ordinary Build test port and real journal.

1. A receives an explicitly selected Issue-owned document in frozen Context and
   captures the approved material revision. Its analysis is accepted; field saving
   remains pending.
2. The document is removed from Issue materials without changing requirements.
3. B starts afterward, captures the newer revision, and consumes A's exact accepted
   payload. B's own Context contains only the ordinary task, not the document.
4. Both analyses remain accepted, but both saves conflict. B must retain A's missing
   document dependency; no field/history update or extra Build is created. Repeated
   dispatch claims no new work.

Observed Red: A's save conflicted, but B incorrectly saved Priority;
`(delivered, failed)` was `(1, 1)` instead of `(0, 2)`. Green requires both outbox
entries to retain the explicit `Conflict` result and leaves the Issue unchanged.
Initial fixture budget errors were corrected before this behavioral Red and are
not counted as evidence of the defect.

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export OCTACITY_POSTGRES_URL=postgres://octacity:octacity-test@127.0.0.1:15439/postgres
cargo test --locked -p octacity-server --test factory_configured_flow ordinary_form_consumer_rejects -- --ignored --nocapture
cargo test --locked -p octacity-server --test factory_configured_flow ordinary_form_ -- --ignored --nocapture
```

The original four ordinary-form tests passed before adapting the fixture. The task
uses only its own disposable `octacity-context-lineage-postgres` service; no live
provider or user's stand is involved. Final verification results are recorded below.

Follow-up results:

| Check | Result |
| --- | --- |
| Original `ordinary_form_` baseline before changing the fixture | 4 passed |
| `ordinary_form_` with the A → B regression and original WIP ceilings | 5 passed |
| `cargo clippy --locked -p octacity-server-application --lib -- -D warnings` | passed |
| `cargo clippy --locked -p octacity-server --test factory_configured_flow -- -D warnings` | passed |
| `cargo fmt --all --check` | passed |
| `git diff --check` | passed |
| `python3 tools/check_architecture.py` | passed |
| `openspec validate add-issue-classification-snapshots --strict` | passed |

Only the aggregate fixture budget is widened for two executions; original WIP
ceilings are preserved. Accepted journal records are checked by identity/content,
without assuming SQL insertion order. The disposable PostgreSQL service was stopped
after verification. No commit was created.

## Follow-up: read provenance without field-writing capability

`FactoryIssueInputs` now owns read-only Issue origin capture. The common runtime
installs it independently of field-writing profiles, management principals and
node write permissions. It resolves the canonical Work association and captures
the Issue revision before awaiting protected Context acquisition. The existing
frozen input baseline carries this origin through restoration and technical
retries; no second persisted lineage format or store is introduced. Pure control
steps avoid this read, and ordinary Work without an Issue association keeps its
existing input path.

`FactoryIssueFieldWrites` only dispatches accepted local effects. It no longer
prepares inputs or determines whether a producing agent retains read provenance.
Material traversal requires the original captured revision for nonempty Context;
it never substitutes the consuming writer's baseline when that origin is absent.
Read provenance grants no write authority.

Public PostgreSQL regression:
`ordinary_form_consumer_retains_context_from_a_non_writing_producer`.
A has no `issue_fields` outcome, has deny-all node permissions, and executes before
any writer is constructed. Its accepted analysis consumes an Issue document.
The document is removed before B starts. B consumes A's exact accepted payload and
has a field-writing outcome with a newer invocation baseline. Both analyses and
exactly two ordinary Build intents remain accepted; A creates no `issue.fields.*`
effect, B's save conflicts, and Issue values remain unchanged. Repeated dispatch
claims no additional work.

Observed behavioral Red: `(delivered, failed)` was `(1, 0)` instead of `(0, 1)`;
B incorrectly saved after its ordinary producer lost the document dependency.
Green rejects that save with the explicit retained `Conflict` outcome. The earlier
writing-A regression also remains green, with both pending saves conflicting.

With the bounded Cargo settings and disposable PostgreSQL URL above:

```sh
cargo test --locked -p octacity-server --test factory_configured_flow ordinary_form_consumer_retains_context_from_a_non_writing_producer -- --ignored --nocapture
cargo test --locked -p octacity-server --test factory_configured_flow ordinary_form_ -- --ignored --nocapture
```

The first command established Red before implementation. After separating the
owners, the second command passed all six ordinary-form regressions. Initial
compilation and fixture-assertion corrections are not counted as behavioral Red.
The task-owned service is `octacity-context-provenance-postgres`; it uses tmpfs
storage and does not alter the user's stand. Final checks are recorded below.

| Check with the exports above | Result |
| --- | --- |
| `cargo test --locked -p octacity-server --test factory_configured_flow ordinary_form_ -- --ignored --nocapture` | 6 passed |
| `cargo test --locked -p octacity-server --lib runtime::factory::factory_profile_file_tests -- --nocapture` | 13 passed |
| `cargo clippy --locked -p octacity-server-application -p octacity-server --lib --test factory_configured_flow -- -D warnings` | passed |
| `cargo fmt --all --check` and `git diff --check` | passed |
| `python3 tools/check_architecture.py` | passed: 64 packages, 264 edges |
| `openspec validate add-issue-classification-snapshots --strict` | passed |

Independent architecture and specification reviews found no further actionable
issues. These are targeted checks, not live-provider or full-workspace qualification.
The macOS test linker emitted its existing large unwind-table warning; tests and
Clippy completed successfully. Documentation-only updates have no executable TDD
boundary. The disposable PostgreSQL service was stopped after verification. No
commit was created.

## Follow-up: material provenance through subflow returns

Payload consumption now follows a returned call's existing `subflow_source()` to
the exact retained child result before inspecting its producing input. Successive
nested returns follow the same references, bounded by the retained record count.
The parent call's input does not stand in for the child's analysis. Raw-input,
metadata and Artifact-reference views keep their existing selection semantics;
no new journal format, permission, operator setting or persistence owner is added.

The public PostgreSQL regression
`ordinary_form_consumer_retains_context_returned_from_a_subflow` executes a real
SubflowCall around ordinary A, with no field-writing outcome and deny-all node and
child Flow permissions. A's ordinary Build consumes an Issue document in Context.
The parent accepts the exact child payload and retained return reference, with no
Context of its own. The document is then removed before B captures its newer
baseline and consumes the call's accepted payload to fill fields.

Observed Red before the implementation change: B incorrectly saved;
`(delivered, failed)` was `(1, 0)` instead of `(0, 1)`. The initial fixture contract
error (a child policy broader than the call's deny-all policy) was corrected before
this behavioral Red and is not counted as bug evidence.

The test requires the child analysis, accepted call result and B analysis to remain
retained, with exactly two ordinary Build intents. A creates no field-writing
effect, B's save retains an explicit `Conflict`, Issue values remain unchanged and
repeated delivery claims no new work. It shares the existing document/Issue/save
journey rather than adding a parallel mock writer or provenance implementation.

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export OCTACITY_POSTGRES_URL=postgres://octacity:octacity-test@127.0.0.1:15439/postgres
cargo test --locked -p octacity-server --test factory_configured_flow ordinary_form_consumer_retains_context_returned_from_a_subflow -- --ignored --nocapture
```

The six existing ordinary-form regressions passed before adapting their shared
fixture. The new case established Red through accepted Build/Flow/Issue boundaries
using the task-owned `octacity-subflow-provenance-postgres` service. Final Green
and regression results follow below.

Green: the same single-case command passed after the bounded return-reference
walk was added. The complete `ordinary_form_` filter then passed all seven cases,
including the new regression and prior direct-producer, conflict, denied-write,
lost-ack/restart and exact-material cases. The existing nested-Flow PostgreSQL
recovery contract also passed:

```sh
cargo test --locked -p octacity-server --test factory_configured_flow ordinary_form_ -- --ignored --nocapture
cargo test --locked -p octacity-server --test factory_configured_flow configured_nested_flow_restores_caller_and_child_results_in_postgres -- --ignored --nocapture
```

Independent specification and maintainability reviews found no further actionable
issues. These are focused behavioral checks, not a full-workspace or live-provider
qualification. The macOS linker emitted its existing large unwind-table warning;
both test commands completed successfully. Documentation-only edits have no
executable TDD boundary.

| Final check with the bounded Cargo exports above | Result |
| --- | --- |
| `cargo clippy --locked -p octacity-server-application -p octacity-server --lib --test factory_configured_flow -- -D warnings` | passed |
| `cargo fmt --all --check` and `git diff --check` | passed |
| `python3 tools/check_architecture.py` | passed: 64 packages, 264 edges |
| `openspec validate add-issue-classification-snapshots --strict` | passed |

The task-owned tmpfs PostgreSQL container was stopped and removed after the test
runs. The user's stand was not modified. No commit was created.
