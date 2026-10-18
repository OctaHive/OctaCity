# Task 5.13 verification: stage-local source actions

## Result and boundaries

Stage effects are optional typed parameters of the existing `set_status` action.
`FactoryStageSourceUpdate` pins the original Source and Reporter bindings, reviewed
connection/access references and one bounded logical-status/add/remove/state row.
Flow nodes, outcome routing, nested definitions, execution permissions and the
existing Reporter/outbox remain the execution model. No provider DTO, native item
selector, credential, card command, poller, migration or runtime dependency was
added. Ordinary status actions retain their previous wire shape and validation API.

Both Flow and Factory Configuration publication accept an explicitly installed
`FactoryStageSourceAuthorizer` using `with_source_actions`. The existing Source
configuration handlers implement this small port using their current authorized
choice discovery, independent task-read/task-update grants and installed editing
ceiling. Unsupported/absent authorization fails closed. Publication validates the
complete resolved closure, including immutable nested definitions, and rechecks
current versions/grants. Shared parameter validation rejects contradictory labels,
status/capability mismatch and source effects without the durable outbox.

Protected deployment composition must attach the same trusted Source handlers to
both publication handlers; UI metadata alone does not install a writer. Stage
editing currently requires the Reporter's explicit editable-effect ceiling. Existing
installed-only/synchronous actions remain usable through their existing profiles;
the source editor explains when a compatible deferred profile is required. Null
connection pins remain representable for trusted legacy installed definitions;
the management authorizer requires an actual reviewed managed connection.

The projector obtains stage parameters only from verified accepted action history
under a current outbox claim. It compares the exact original Source, Reporter and
connection, validates the original installed effect ceiling and uses the original
native item. Stage effects take precedence over input-wide status defaults for that
action. There is no fallback to another target or newer mapping. Rotation/revocation,
ordered inspection/reconciliation, original deadlines and finite retries continue
through the existing protected Reporter. Optional later card links grant no writes.

## Authoring and progress

`FlowGraphEditor`/`FlowBindings` expose source effects on ordinary deferred status
actions. Operators route arbitrary declared outcomes through those nodes with the
existing transition/data-binding controls, including intermediate and nested flows.
The editor reuses the input editor's safe connection/profile API and shared status
mapping controls. It offers only compatible installed editable Reporter profiles,
keeps drafts in component memory and compares complete immutable references.
Choice refresh/reordering cannot change a selected binding by list position.
Validation and confirmation review exact effects and the immutable mapping digest
from the complete normalized closure; edits require a fresh publication proof.

Source diagnostics add optional allowlisted action/stage provenance: definition,
Flow invocation, cycle, node, outcome and accepted attempt. Stage entries identify
explicitly consumed records rather than guessing a stage from names or task text.
Shared history links to exact Run attempts and distinguishes reporting health from
task state and Run completion. PostgreSQL projects this metadata from existing
immutable records without locking the Run or introducing another progress journal.
Payloads, Context and provider bodies remain outside these diagnostic responses.

The existing no-action/no-write and arbitrary intermediate/nested stage regressions
remain passing. Manual Artifact admission, absent-card navigation and independent
code/task providers are preserved. Persistent cards and ownership/conflict controls
remain in `add-factory-work-card`; observation handoff remains task 5.14.

## TDD evidence

All Cargo invocations used `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2`.

| Boundary | Observed Red | Observed Green |
| --- | --- | --- |
| REST schema/fixtures | Missing `FactoryStageSourceUpdate` and reporting-origin contracts | Closed typed schemas, examples and generated client pass |
| Installed action | Existing parser rejects the required stage-effect parameters | Valid effects accepted; conflicting labels/status, extra fields and absent outbox rejected |
| Source management port | Required public `validate_source_update` API absent | GitHub and fake-provider current-grant/installed-ceiling tests pass, including revocation and foreign mapping rejection |
| Factory publication | Existing validation accepts unverified source-effect parameters | Both publication paths share fail-closed closure validation |
| Verified process/outbox | Stage-owned action rejected before the new parameter contract | Stage effects override input defaults and retain the original native item/mapping after input replacement and later visible card association |
| Diagnostics | Expected action provenance absent from pending reporting | Pending/applied/exhausted records retain safe origin; PostgreSQL read completes while a writer holds the Run lock |
| UI authoring | Guided source controls absent | Literal effects authoring, review, draft discard and browser keyboard path pass |
| Refreshed choices | Reordered choices prevent applying the selected mapping | Exact binding keys preserve the selected source/mapping and draft |
| Legacy plugin UI | Arbitrary null parameters crash the specialized editor | Unsupported profiles show guidance without requiring integration setup |
| UI progress/review | Stage/outcome and exact effect summary absent | Nested invocation/attempt links and frozen publication summary pass |

Core commands and results:

```sh
cargo test --locked -p octacity-server-application --lib factory_
# 386 passed. Final shared-validation refactor also verified with:
cargo test --locked -p octacity-server-application --lib factory_flow_action_tests
# 20 passed, including the intermediate/nested/no-action contracts.
cargo test --locked -p octacity-server-application --lib factory_admission_tests
# 82 passed, including independent manual and installed Source admission.
cargo test --locked -p octacity-server --lib work_source
# 64 passed; 9 existing explicitly opt-in PostgreSQL tests ignored.
cargo test --locked -p octacity-server --lib stage_owned_effects_override
# Final later-card/input-replacement journey passed for GitHub and a fake provider.
cargo test --locked -p octacity-server --lib source_reporting_diagnostics_does_not_wait_for_a_run_writer -- --ignored
# Passed against an owned disposable PostgreSQL container; lock timeout unchanged.
cargo test --locked -p octacity-server-api-rest
# 134 passed across library, administration, public-error and handler tests.
pnpm --dir ui test:unit
# 27 script checks + 453 Vitest tests passed.
pnpm --dir ui test:browser
# 82 passed, including keyboard stage editing, manual intake and existing visual checks.
# The final stage scenario also verifies reviewed effects and zero horizontal overflow at 640 px.
pnpm --dir ui exec vitest run src/features/factory/FactoryPublication.test.tsx src/features/factory/FlowSourceUpdate.test.tsx src/features/factory/SourceProgress.test.tsx
# 14 passed after final authoring/refactoring changes.
node --test ui/scripts/verify-architecture.test.mjs
# 23 passed after qualifying the measured stage-effect footprint.
pnpm --dir ui api:generate
pnpm --dir ui build
pnpm --dir ui lint
pnpm --dir ui format
```

The bundle gate first rejected the added feature against the previous 5.12 ceiling.
The measured output is approximately 934 KB JavaScript, 81 KB CSS and 1,016 KB static
content with 43 files. Aggregate ceilings are 935,936 JS and 1,017,856 static bytes;
entry (460,800), CSS (81,920), dependencies and asset-count limits are unchanged.
The architecture fixture was rebased to the measured footprint and still rejects
additional unplanned growth. No timeout increase or test serialization was used.

Clippy (`--all-targets --all-features -- -D warnings` for the five affected server
packages), Rust formatting, architecture, security matrix, local-stand context,
OpenSpec and whitespace checks qualify the implementation. Documentation-only
updates have no executable behavior; OpenSpec/whitespace checks apply instead.
Free space was checked throughout: approximately 15 GiB initially and 13 GiB after
checks. Existing caches/stand state were preserved; the owned PostgreSQL test
container is removed after qualification. No commit was created.
