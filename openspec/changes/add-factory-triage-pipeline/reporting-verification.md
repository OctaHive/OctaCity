# Task 6.1: finite result reporting

## Delivered boundary

`FactoryResultReportingDraft` is a pure Application authoring operation. It
compiles a complete reviewed outcome mapping and optional independent result
field dimensions into an ordinary reusable Flow. Enum choices and bounded integer
partitions select only operator-owned literal `set_status` effects from the
installed Source/Reporter editing policy. Model text supplies neither label names,
native item identifiers nor write authority. Missing labels/states, invalid scope,
incomplete/overlapping partitions, incompatible receipts and excessive native
node/edge plans are rejected. Concurrently applicable groups must own disjoint
installed native scopes (including whole managed label groups and state);
exclusive cases may share fields. Missing installed ownership cannot prove
independence for multiple coactive effects.

The result has one outcome selector, one selector per dimension, literal action
nodes and a final result projection. Its size grows with the sum of reviewed
choices, not their Cartesian product. Per-node, child, parent and normal admission
budget/permission ceilings remain explicit. Composition performs no provider,
credential or storage I/O and does not activate an installation.

`bind` inserts exact outcome-bound calls into an ordinary template. The parent
call reads the original accepted source record; child inputs retain the exact
caller input digest. The child returns the original result and the parent keeps
its original continuation. Substituted child definitions, alias collisions,
fan-out/repeat routes and wider authority fail closed. Normal complete-closure
publication still validates the installed action contract and current independent
Project writer/connection grants, including nested definitions. Installed choice
metadata alone is never a grant.

Ordinary `FactoryWorkStatusAction`, accepted-node journaling and the existing
ordered outbox own execution and delivery. The native item comes from the admitted
Work. Four selected outcome/component/size/priority effects produce four separate
logical operations. A worker can deliver one while the others remain pending,
without rewriting Work or accepted records. Both Console publication-review
locales explain independent delivery and distinguish queueing from confirmation.

## TDD observations

All Cargo commands used this environment and these flags:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -j2 -p octacity-server-application --lib TEST_FILTER
```

Each row below was run first at Red and again after its minimal implementation at
Green. The filter is the full test name shown in the first column. Missing required
public APIs established the initial compile-time Reds; incidental Rust errors or
incorrect fixtures were corrected and were not counted as behavioral Reds.

| Test filter                                                                              | Observed Red                                   | Observed Green                                                                                                      |
| ---------------------------------------------------------------------------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `finite_result_reporting_compiles_reviewed_literal_effects_to_ordinary_actions`          | Required draft/composition API absent          | Exact ordinary action, installed plugin contract, literal effects and caller binding accepted                       |
| `reporting_rejects_effects_outside_the_installed_label_and_state_choices`                | Unknown label accepted                         | Unknown label and state rejected                                                                                    |
| `reporting_requires_explicit_complete_outcome_coverage`                                  | Missing outcome silently accepted              | Missing and foreign outcomes rejected                                                                               |
| `explicit_no_effect_outcome_continues_without_a_status_action`                           | Explicit preservation rejected                 | Preserved route has no status action                                                                                |
| `independent_reporting_dimensions_grow_linearly_and_select_only_reviewed_effects`        | Dimension API absent                           | Two outcomes, six components, three sizes and two priority bands produce 18 nodes with the selected literal effects |
| `reporting_dimensions_require_complete_nonoverlapping_typed_partitions`                  | Missing enum choice accepted                   | Missing, duplicate, foreign, mistyped, gapped, overlapping and out-of-range choices rejected                        |
| `reporting_configuration_blocks_missing_writer_authority_scope_and_budget`               | Missing writer plugin permission accepted      | Absent execution permission, substituted resource and excessive node budget rejected                                |
| `reporting_binds_every_action_chain_to_the_exact_source_record_and_preserves_results`    | Binding API absent                             | Exact source-record inputs and shared child reference pass normal complete-closure configuration validation         |
| `reporting_binding_refuses_parent_collisions_unsupported_routes_and_authority_expansion` | Schema alias overwritten                       | Schema/node collisions, fan-out/repeat and parent authority/budget expansion rejected                               |
| `reporting_rejects_coactive_field_conflicts_but_allows_exclusive_effect_choices`         | Coactive add/remove of the same label accepted | Coactive label/state conflicts rejected; mutually exclusive ownership accepted                                      |
| `reporting_rejects_incompatible_receipts_and_graphs_above_native_limits`                 | Boolean receipt schema accepted                | Boolean/sample-only receipts and excessive linear graphs rejected                                                   |

Console Red/Green:

```sh
cd ui
pnpm exec vitest run src/features/factory/FactoryPublication.test.tsx \
  -t 'reviews exact immutable per-stage effects'
```

The existing ordinary publication-review test first failed because the delivery
notice was absent. It passed after adding the localized notice, while retaining
all exact immutable effect details and confirmed publication checks.

Existing owner regression tests were run before modifying the shared reporting
fixture: `stage_reporting` (3 passed), `status_action` (22 passed) and Factory
`input_reporting_maps` (1 passed). The generalized fixture reuses the same
admission owner with caller-supplied schemas; it does not replace production
publication, execution or delivery.

Two additional qualification tests exercise already implemented composition and
existing owners; they are not claimed as new Red/Green cycles:

- `reporting_execution_queues_independent_receipts_and_retains_exact_result_provenance`:
  repeatedly reconstructs the ordinary runner from retained state, verifies the
  original source-record/caller digest chain and unchanged final result, then
  delivers one of four separate operations. An initial assertion that the common
  test input provider supplied no Work Context was incorrect; that provider
  explicitly supplies it to every node. Correcting the assertion was a fixture
  correction, not a production behavior change.
- `supplied_two_stage_triage_publishes_with_independently_bound_reporting_flows`:
  composes reporting into both gates of the actual released triage template,
  retains both reasoning stages and exact selected Profile bindings, and validates
  the complete ordinary definition/schema closure.

All 13 `result_reporting` tests passed before and after the behavior-preserving
cleanup. The cleanup shares private effect/control node builders and computes
continuations directly; it adds no public abstraction or owner.

## Review and qualification limits

Separate Standards and Spec reviewers inspected tracked and untracked changes
against `05041b1c`. Their initial findings led to the coactive-field Red/Green
cycle, receipt/graph-bound qualification and removal of duplicated node emission.
Both repeat reviews found no remaining actionable issues in task 6.1. Reviews
were read-only; test outcomes above come from executed commands.

Current Project writer grants remain checked by the existing nested source-update
publication path. The complete Application suite includes
`stage_source_effects_require_current_separate_write_authority_and_installed_editing_ceiling`
and
`configuration_publication_rejects_stage_source_effects_without_an_installed_authorizer`.
These existing public-boundary tests cover separate writer authority, installed
editing limits and grant revocation; compiler metadata is not substituted for
those checks.

These are deterministic contract and owner tests, not a live GitHub, paid-model
or packaged Agent qualification. Canonical Issue association/workflow handling
(6.2), extended delivery/recovery scenarios (6.3), management setup/resolution
(7.x) and packaged journeys (8.x) remain separate tasks. The release manifest
retains its default absence of write actions/permissions; using this optional
composition requires explicit reviewed normal publication.

## Final checks

- `cargo test --locked -j2 -p octacity-server-factory -p octacity-server-application --lib --quiet`:
  **858 Application + 320 Factory passed**. The Application run includes all
  existing writer-grant/publication and stage-reporting regressions, plus the
  long-running requirements/release journeys (393.90 seconds); none failed.
- `cargo test --locked -j2 -p octacity-server-application --lib result_reporting --quiet`:
  **13 passed**, including complete-closure publication and ordinary runner/outbox
  execution. Passed before and after the private-builder refactor.
- `cargo clippy --locked -j2 -p octacity-server-application --tests -- -D clippy::all`:
  passed. The existing unused `pending_factory_profile_fixture` compiler warning
  remains; no new Clippy warning was introduced.
- `pnpm exec vitest run src/features/factory/FactoryPublication.test.tsx`:
  **8 passed**.
- `pnpm typecheck` and scoped `pnpm exec eslint ... --max-warnings=0`: passed for
  the publication component/test and both changed message catalogs.
- `pnpm build`: passed, including frontend architecture and bundle budgets. The
  build emitted the existing PostCSS `from` option warning.
- `python3 tools/check_architecture.py`: passed, **64 packages / 263 edges**.
- `cargo fmt --all -- --check`, scoped `pnpm exec prettier --check`,
  `openspec validate add-factory-triage-pipeline --strict` and `git diff --check`:
  passed. Documentation and formatting changes have no executable behavior and
  are covered by these checks rather than a fabricated Red/Green cycle.

Disk space was checked throughout with `df -h .`: **54 GiB at the start**, with
**51–53 GiB** available during validation. Cargo debug information and incremental
builds were disabled and concurrency limited to two jobs. No cleanup of user data
was needed. No commit was created, as requested; task 6.2 was not started.

## Native ownership review fix

Installed editing policies now carry optional, bounded `WorkSourceReportingOwnership`:
complete literal-label scope sets and the shared state scopes. Scope identities
are provider-owned and stable across profiles/aliases for the same native item.
They must match the qualified adapter's actual ownership units (including GitHub
managed label groups); they grant no authority. Unknown ownership cannot prove
independence for coactive producers. Legacy single-producer policy serialization
is unchanged.

The composer unions each exclusive group's scopes and rejects intersections between
coactive groups. It freezes each action's scope claim; ordinary installed-policy
authorization and retained dispatch validate that claim. Issue mapping publication
freezes trusted installed ownership in its existing content pin and rejects caller
substitution. Runtime producer review derives Source scopes from the selected
pinned profile and compares them with frozen Issue scopes before provider I/O.
No provider-specific parsing, new journal or delivery owner was added.

All Cargo commands below used `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`, `--locked` and `-j2`.
Each listed test was run before and after its implementation change:

| Package / test filter (`cargo test -p PACKAGE --lib FILTER`)          | Observed Red                                                                        | Observed Green                                                                                                                        |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `octacity-server-factory installed_reporting_ownership_resolves`      | Required ownership resolution API absent                                            | Same-group labels resolve equal scopes; independent groups differ; legacy bytes unchanged                                             |
| `octacity-server-factory installed_reporting_ownership_requires`      | Missing label coverage accepted                                                     | Missing/foreign labels, empty label/state scopes, foreign state and excessive scope sets rejected                                     |
| `octacity-server-application reporting_rejects_distinct_labels`       | Coactive component/size effects in one native group accepted                        | Different literal labels sharing a native group rejected; all 14 composer tests pass                                                  |
| `octacity-server-application stage_source_effects_require`            | Frozen scope claim API absent                                                       | Invented ownership rejected under actual installed writer policy, alongside grant/ceiling checks                                      |
| `octacity-server-application issue_source_mapping_freezes_installed`  | Immutable ownership field absent                                                    | Trusted ownership retained under mapping pin; original replay preserved; substitution rejected                                        |
| `octacity-server-application issue_source_push_requires_one_producer` | Different labels sharing one native group accepted                                  | Shared group rejected; known independent group accepted                                                                               |
| `octacity-server work_reporting_preserves_source_target`              | Known disjoint delivery refused because selected profile scopes were not propagated | Disjoint delivery succeeds; same-group conflict fails before provider I/O; disabled push and management partition isolation preserved |

The runtime command additionally used `-- --include-ignored` and an explicitly
configured disposable PostgreSQL service. It covers four independently migrated
scenarios through the ordinary retained outbox and Reporter owner.

The transport cycle used `cargo test -p octacity-server-api-rest --test v1_handlers
reporting_native_ownership`: Red reported an undocumented `ownership` property;
Green accepted the bounded closed contract, with optional legacy-compatible
policy, action and Issue mapping metadata. All 13 integration contract tests pass.

The README's outdated description of task 6.3 as future qualification was corrected.
That edit changes documentation only, so it has no executable Red/Green cycle.

Final review-fix regression:

- `cargo test --locked -j2 -p octacity-server-application --lib`: 861 passed.
- `cargo test --locked -j2 -p octacity-server-factory -p octacity-server-store --lib`:
  322 Factory and 109 Store tests passed.
- `cargo test --locked -j2 -p octacity-server-api-rest --test v1_handlers`:
  186 passed, including OpenAPI drift and legacy wire fixtures.
- `cargo clippy --locked -j2 -p octacity-server-factory -p octacity-server-store
-p octacity-server-application -p octacity-server -p octacity-server-api-rest
--tests -- -D clippy::all`: passed. A nested conditional was collapsed without
  changing behavior.
- Regenerated the UI API artifacts from the exported OpenAPI document; `pnpm
typecheck`, seven Factory authoring/Issue Source constraint tests and all eight
  `FactoryPublication.test.tsx` tests passed.
- Architecture validation, strict OpenSpec validation, formatting and diff checks
  passed. Follow-up read-only Standards and Spec reviews found no residual findings.

The first broad runtime `reporting::` run, concurrent with the full CPU-heavy
Application suite and builds, finished with 74 passed and four failures in
readiness, normal HTTP Issue delivery, normal shared Reporter delivery and composed
triage recovery. All four passed separately without changing production code,
budgets or test deadlines. A separate complete runtime run was used to distinguish
these load-sensitive failures from a contract regression. No test timeout was widened.

The complete isolated runtime harness run (`reporting:: --include-ignored
--test-threads=2`, using the same compiled test binary and disposable database)
passed all 78 tests in 163.28 seconds, without production or timeout changes.
The task-owned PostgreSQL container was removed after verification. Disk remained
at approximately 47–48 GiB free throughout this fix; no commits were created.
