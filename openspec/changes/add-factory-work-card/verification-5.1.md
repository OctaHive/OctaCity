# Task 5.1: accepted Issue changes retain original Reporter desired state

## Scope and boundaries

Tests exercise authorized Issue commands, the public store/mutation-outbox
contract, configured actions through `FactoryNodeRunner`, and the common
producer-policy contract. Existing command publication owns the transaction;
existing Reporter effects own native interpretation. No provider SDK, worker,
outbox implementation, execution or external request path was added.

New mapped status commands freeze the original Issue revision, link, mapping,
writer, workflow/status, finite effects and accepted command/actor/scope. Factory
commands additionally retain original Work/Run/Flow/cycle/attempt/action lineage.
The common sealed command retains its reviewed mapping (including absence), so
replacement between review and application conflicts instead of redirecting a
fresh effect. Completed replay returns the original outcome. Legacy intents
without desired material cannot be populated by later attachment or replacement.

The accepted same-item status-action chain checks explicit stage effects and
inherited input mappings. Overlapping labels/native state require one producer;
opaque legacy effects fail closed when paired with card effects. Unrelated fields
remain managed independently. The check uses original Source admission/native
item, not display values, a later link or provider names. A local-only action
needs neither a provider nor Reporter. Unsupported remote metadata writes remain
unavailable under the existing mapping contract. Dispatch/ordering/recovery
composition and explicit conflict resolution remain tasks 5.2–5.3.

## TDD evidence

All Rust commands use `--offline`, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` and
`CARGO_BUILD_JOBS=2`. PostgreSQL uses the disposable bounded-tmpfs
`octacity-issue-push-51` container. Fixture/import compilation errors and the
initial zero-match stage filter are excluded from behavioral Red/Green evidence.

| Command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-application --lib issue_source_mapping_push_requires` | A committed local move retained no original desired link/effects (`null`) | Original link, exact writer/workflow/status, configured effects and scoped command are retained; existing divergence/pull/revocation scenarios still pass |
| `cargo test -p octacity-server-application --lib issue_pipeline_push_review` | Required accepted-action push review API was absent | A mapping installed after an explicitly absent review rejects the fresh move, leaving Issue and push intents unchanged |
| `cargo test -p octacity-server-application --lib issue_source_push_requires_one_producer` | Required producer-policy API was absent | Overlapping label/state effects reject; unrelated labels remain allowed |
| Same producer filter, resource-alias case | An alias for the already qualified same native item bypassed overlap rejection | Scope uses the original native item; resource aliases do not introduce a second producer |
| Same producer filter, opaque-scope contract | Required representation of an opaque producer was absent | Unknown legacy scope rejects when the card manages effects, while an empty card effect permits the direct producer |
| `cargo test -p octacity-server-application --lib configured_local_issue_action` | The real node/command owner retained no explicit absent push review | A local configured action completes without Reporter and retains its absent review |
| `cargo test -p octacity-server-application --lib issue_legacy_pipeline_command` | An old accepted producer without push review acquired a newly attached mapping's intent | Fresh and pending legacy commands still complete their original local move and replay, while retaining no new push intent |
| `cargo test -p octacity-server-store-postgres --test issues source_push_freezes_original -- --ignored` | The existing mutation outbox contained no original desired link/effects | Exact desired material commits with the move; store reconstruction and original replay preserve one identical outbox row |

The configured-action test was then qualified for both local-only and linked
mapped tasks as
`configured_issue_action_retains_exact_push_review_and_origin_without_provider_io`
in the `factory_flow_action_tests` regression. It verifies exact
accepted action lineage and preserves the original desired material through
mapping replacement. Attaching and enabling a mapping after a previously local
action produces no retroactive push, including when replaying its sealed command.

Before extracting shared test setup/parameter decoding,
`cargo test -p octacity-server-application --lib factory_flow_action_tests`
passed all 20 existing action/reporting scenarios. The corrected focused
`stage_routes_queue` filter also passed its actual stage/outbox scenario.

## Final verification

All focused TDD cycles above passed. Final regression qualification uses the same
bounded Rust environment; PostgreSQL tests use `OCTACITY_POSTGRES_URL` pointing to
the disposable local database.

| Command or suite | Result |
| --- | --- |
| `cargo test --offline -p octacity-server-application --lib issue_` | 107 passed |
| `cargo test --offline -p octacity-server-application --lib factory_flow_action_tests` | 21 passed |
| `cargo test --offline -p octacity-server-application --lib source_` | 143 passed |
| `cargo test --offline -p octacity-server-store --lib` | 81 passed |
| `cargo test --offline -p octacity-server-store-postgres --test issues -- --ignored --test-threads=2` | 42 passed, including desired-state restart/replay |
| Existing native Source/Reporter application regressions | 78 passed; 9 database-specific Reporter scenarios excluded from that invocation |
| Existing runtime PostgreSQL regressions | 2 passed |
| REST library and HTTP contracts | 12 + 135 passed |
| `cargo clippy --offline -p octacity-server -p octacity-server-application -p octacity-server-store-postgres --all-features --all-targets -- -D warnings` | Passed |

The native test linker emitted the existing macOS `__eh_frame` size warning;
the tests completed successfully. No new dependency, migration or frontend
contract was added. Builds used two jobs without incremental/debug artifacts;
approximately 11 GiB remained free after qualification. The disposable PostgreSQL
container is removed after testing.

Documentation-only updates have no executable TDD cycle. They are validated with
`openspec validate add-factory-work-card --strict`, `cargo fmt --all -- --check`,
`python3 tools/check_architecture.py` and `git diff --check`.
