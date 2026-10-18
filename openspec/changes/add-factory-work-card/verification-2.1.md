# Task 2.1 — Project business workflow application and memory contracts

Implemented on 2026-10-18. Scope: bounded immutable status/transition vocabulary,
authorized validation/publication/default selection, retained versions and memory
behavior. This is the backend model slice; PostgreSQL, HTTP and Console settings
remain tasks 2.2–2.4. Issue creation and actual guarded movement remain 2.5.

## Behavior and reuse

- Public `IssueWorkflowHandlers` use the narrow `IssueWorkflowStore` port and the
  existing authorized management dispatch, audited mutations and security-scoped
  idempotency. Memory behavior extends the existing Project store, not the
  integration or Factory configuration store.
- Separate workflow, status and transition UUID types keep display names/order
  independent of identity. Workflow definition versions and Project default
  selection versions are distinct positive types.
- Status entries contain ID/name only. The immutable workflow owns one required
  `initial_status_id` and 0–64 unique `terminal_status_ids` referencing its own
  statuses. Empty terminal selection and an initial status that is also terminal
  are valid. Missing settings, legacy per-status flags, unknown references and
  duplicate terminal selections are rejected.
- Validation permits 1–64 statuses and 0–256 direct transitions and bounded
  canonical UTF-8 names. It rejects duplicate IDs,
  ambiguous repeated edge/origin declarations, invalid endpoints, self-edges,
  empty/repeated origin lists and repeated/unknown evidence requirements. Only
  manual/pipeline/source origins and two finite exact-evidence declarations are
  supported; unknown guard/script fields are rejected.
- The ordinary retained v1 JSON template supplies the first default without a
  write, administrative publication, Factory, repository or installed provider.
  Its Project-scoped implicit workflow ID uses the non-nil Project UUID. Future
  templates must retain that original version's meaning.
- Publication creates an immutable version and preserves old references. It
  does not change the default. Explicit default selection pins a same-Project
  exact version and uses a separate selection precondition. Neither operation
  performs task migration, evidence evaluation, admission or provider calls.
- Administration is distinct from ordinary View. Denial precedes publication
  and replay; invalid publication/foreign selection retain the old state with
  no partial replay/audit/outbox record. Exact receipt replay remains original
  after state changes; changed intent or stale preconditions conflict.

See [Issue workflow contracts](../../../docs/reference/issue-workflows.md) for
the public model and compatibility boundary.

## Original task 2.1 test-first cycles

Each cycle started with a public application test, before implementing that
behavior. Commands used the environment below and this prefix:

```sh
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
cargo test --locked -p octacity-server-application --lib <test-name>
```

| Public test name | Observed Red | Observed Green |
| --- | --- | --- |
| `issue_workflow_default_is_declarative_stable_and_needs_no_administration_or_factory` | Missing `GetDefaultIssueWorkflowQuery` / handlers | 1 test passed |
| `issue_workflow_validation_bounds_business_policy_without_interpreting_names_or_scripts` | Missing `ValidateIssueWorkflowQuery` contract | Both current workflow tests passed |
| `issue_workflow_publication_retains_versions_replays_original_intent_and_requires_administration` | Missing publication/retained-version application contracts | All 3 current workflow tests passed |
| `issue_workflow_default_selection_is_explicit_versioned_and_replays_its_original_target` | Missing default-selection application contract | All 4 current workflow tests passed |

These are missing-required-API compiler Reds, permitted by the repository TDD
rule. Incidental import/attribute/exhaustive-match wiring errors were corrected
and are not counted as behavioral Reds. After implementation, runtime assertions
exercise outcomes through authorized handlers rather than compilation alone.

Two additional regression tests passed against that implementation without a
behavior change: invalid publication/foreign default selection produce no
partial writes, and exactly 64 statuses/256 transitions are accepted while one
extra is rejected. The original focused command passed **6 tests**:

```sh
cargo test --locked -p octacity-server-application --lib issue_workflow_
```

## Original task 2.1 regression and resource checks

```sh
cargo test --locked -p octacity-server-domain -p octacity-server-store \
  -p octacity-server-application --lib
cargo clippy --locked -p octacity-server-domain -p octacity-server-store \
  -p octacity-server-application --lib --tests -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check
```

Full library regressions passed: **483 application tests and 81 store tests**,
with no failures or ignored tests; the domain library has no unit tests. The
application suite completed in 362.27 seconds. These runs include manual/Source
admission, reporting, integration authority, Project-store contracts and the
existing long Factory execution/recovery journeys as well as the six new tests.

Clippy, formatting, architecture and strict OpenSpec validation pass. Architecture
retains 63 packages and 255 dependency edges; no dependency or lockfile changes
were needed. Task 2.1 is complete; total change progress is **3 of 56 tasks**.

Free space was checked before and during builds: approximately 12 GiB remained.
Only the existing `target` was used, with no container builds, cache deletion,
timeout increases or test serialization. Earlier dirty source/planning files
were preserved; this task only adds its own implementation/evidence and updates
its completion checkbox. `simplify-integration-setup` is untouched.

## Workflow-level settings revision

Revised on 2026-10-18 following the explicit request to move initial/terminal
selection out of individual status entries. Proposal, design, delta specs and
tasks 2.1/2.3/2.4 now use the same workflow-level fields. Completed tasks remain
complete; this revision does not implement PostgreSQL, HTTP or Console settings.
There is no released task 2.1 schema or persisted PostgreSQL workflow data to
migrate. The same default IDs, choices and edges are retained without a legacy
parser, duplicate settings record or new storage port. Memory storage retains
the whole immutable definition through the existing publication/replay boundary.

Each new behavioral test ran and failed before its implementation changed.
Commands used the build environment documented above:

| Command after `cargo test --locked -p octacity-server-application --lib` | Observed Red | Observed Green |
| --- | --- | --- |
| `issue_workflow_initial_and_terminal_choices_are_workflow_settings` | Runtime assertion: root `initial_status_id` was absent (`None` instead of the expected stable ID) | Focused `issue_workflow_` run: 7 passed after replacing status flags with workflow fields and updating the retained template |
| `issue_workflow_settings_validate_references_sets_and_reject_legacy_status_flags` | Runtime assertion: an unknown terminal reference was accepted | Focused `issue_workflow_` run: 8 passed after enforcing terminal membership, uniqueness and bounds |

The tests also cover empty terminal selection, an initial status that is also
terminal, missing required settings, legacy status-flag rejection, ID/name-only
status serialization, and invalid publication leaving default/audit/outbox/replay
state unchanged. Existing publication/version/replay tests now explicitly change
the initial/terminal choices and retain the original definition. The maximum-size
fixture validates 64 selected terminal statuses along with 64 statuses/256 edges.

Current revision regression commands and observed outcomes:

```sh
cargo test --locked -p octacity-server-application --lib issue_workflow_
# 8 passed
cargo test --locked -p octacity-server-store --lib
# 81 passed
cargo test --locked -p octacity-server-application --lib factory_admission_tests
# 87 passed, including manual intake and open provider identities
cargo clippy --locked -p octacity-server-store -p octacity-server-application \
  --lib --tests -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check
```

All listed checks passed. The full application suite documented in the original
section was not repeated for this isolated model revision. Architecture remains
63 packages/255 edges, with no dependency changes. Free space remained around
11 GiB using the existing target, two build jobs and disabled incremental/debug
output. Other pre-existing edits, CI fixes and `simplify-integration-setup` were
preserved. Change progress remains 3/56, with task 2.1 complete.
