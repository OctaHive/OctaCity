# Configured Factory foundation: verification through 10.1

Completed tasks: 9.12–9.17 and 10.1. No commit, staging or push.

## Result and scope

One bounded data-schema catalogue, node input/result journal, declarative gate
evaluator, ordinary Build adapter and fenced node owner now serve operator-named
steps. Dedicated triage/research execution types and the old triage table were
removed. The supplied base Flow retains eligibility, classification, defect and
feature investigations, evidence-based routing, component/severity/dependencies,
requirements-needed acceptance and its declared downstream control path.

The same public configured owner runs manual admission through an unknown
`accessibility_audit`, durable pool publication/selection and every declared
protected-test, implementation, review and verification node. PostgreSQL client
reconstruction between passes restores the exact input/result/intent journal,
selection and canonical Accepted Work contract. Feature evidence retains a
real proposal Artifact identity and exact ordinary Build/Attempt/Job provenance.
Model small-Work assertions cannot override independent large-Work facts.
Configuration replacement cannot alter the pinned bypass policy.

Accepted Work is a bounded canonical projection of immutable Work, exact gate
binding and complete retained source ancestry. The common fenced journal checks
that projection before committing acceptance; no parallel requirements table
or mutable second authority was introduced.

This verifies configured control paths using hermetic ordinary execution and
output-verifier adapters. Requirements authoring/review (10.2/10.3), production
protected-test isolation (11), additional primitive adapters, supervised worker
composition (16.1) and the UI editor (15) remain their separate plan tasks.

## TDD evidence

The refactor started from a passing application/core/store baseline. Behavior
changes used public contracts and observed Red before implementation. Fixture
or environment errors were corrected and did not count as Red. Representative
reproducible filters are below; append `--lib` to select the unit boundary.

| Command | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-application --lib a_configured_bounded_cycle` | Owner repeatedly replayed completed nodes; configured exhaustion was never reached | Fresh bounded attempts reach the exact exhaustion terminal |
| `cargo test -p octacity-server-application --lib frozen_predecessor_inputs_survive` | A later different predecessor invalidated a previously frozen input | Historical input restores from its original accepted source snapshot |
| `cargo test -p octacity-server-application --lib ownership_and_frozen_deadlines` | Successor ownership could invoke ordinary dispatch after the original deadline | Deadline and ownership reject dispatch before effects |
| `cargo test -p octacity-server-factory --lib accepting_gate_cannot_commit` | Journal accepted a gate whose complete Accepted Work projection exceeded its byte ceiling | Acceptance fails atomically before commit |
| `cargo test -p octacity-server-factory --lib an_ordinary_build_cannot_select` | Schema-compatible output could select another declared semantic outcome | Only the frozen ordinary Build outcome is accepted |
| `cargo test -p octacity-server-factory --lib declared_outcomes_can_share` | Required common output-mapping API was absent | Shared mappings restore and explicit outcome mappings take precedence |
| `cargo test -p octacity-server-factory --lib supplied_classification_retains` | Supplied shape rejected component, severity and dependencies | Bounded fields are retained through accepted output mappings |

Additional Red/Green slices covered generic typed inputs/results, configured
mapping/gate contracts, common action-owner integration, current Work priority
and budget binding, full queue reservation, durable selection before dispatch,
and Factory cleanup prevention while a generic linked Build has a retention
hold. Lost-response replay and ordinary retries were characterized through the
public generic adapter before removing their dedicated phase implementation.

## Final checks

Compilation used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` after
the requested root `target` cleanup; the first compilation had reduced free
space below 5 GiB. Final free space is above that threshold. This changes local
build output only, not repository compiler settings.

- `cargo test -p octacity-server-factory -p octacity-server-store -p octacity-server-application --lib`: **391 passed** (169 core, 68 store, 154 application).
- `cargo test -p octacity-server-store-postgres --test factory_discovery --test migrations --test retention -- --ignored`: **20 passed** (13 Factory contracts, 3 migrations, 4 retention), including the originally reported append-only diagnostics regression.
- `cargo test -p octacity-server --test factory_configured_flow -- --ignored`: **3 PostgreSQL whole-owner contracts passed**.
- PostgreSQL checks used `OCTACITY_POSTGRES_URL` pointing at an explicitly disposable local PostgreSQL 18 service. The ordinary execution/verifier adapters are hermetic fixtures; this is not production worker qualification.
- `cargo clippy -p octacity-server-factory -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres -p octacity-server --all-features --all-targets -- -D warnings`: passed.
- `RUSTDOCFLAGS='-D warnings' cargo doc -p octacity-server-factory -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres --no-deps`: passed.
- `python3 -m unittest discover -s tools/tests -p test_check_architecture.py`: **24 passed**.
- `python3 tools/check_architecture.py`: passed; business-stage dispatch guard included.
- `cargo fmt --all -- --check`, `git diff --check`, and `openspec validate add-dark-factory-mode --strict`: passed.

## Review corrections (2026-10-10)

All five review findings were addressed without committing or introducing
business-node dispatch in Rust. The corrections preserve the existing common
input, result, runtime and ordinary Build contracts:

- Expired execution is recovered through `factory_build_for_operation`, a
  side-effect-free lookup. Fresh dispatch/retry remains bounded by the original
  deadline; already published output is revalidated against that deadline.
- A configured subflow call atomically retains one child Flow and initial cycle.
  Explicit `caller_input` projections freeze parent data; completion retains the
  exact child terminal result reference. Accepted Work includes this ancestry.
  Suspended call frames do not consume executable WIP alongside their children.
- Every repeated node execution has a zero-based generation in its frozen input,
  ready entry and selection. Store validation binds it to attempt history.
  PostgreSQL uniqueness and capacity queries use the same generation scope.
- The supplied research gates require a separate `terminal_resolution` report
  for terminal outcomes. The declared predicates check acceptance, exact outcome,
  input identity and freshness. These are template settings, not runtime names.
- Design/spec documentation now describes common envelopes and execution
  identities instead of removed research-specific wire versions and ordinals.

| Public test filter | Observed Red before correction | Green behavior |
| --- | --- | --- |
| `factory_node_build_tests` | A successor observing at 102 after deadline 100 received InvalidInput | Existing completion restores without a second creation |
| `configured_nested_calls` | Missing explicit caller projection, then owner remained Waiting | Exact child result and caller ancestry restore under a successor |
| `a_pooled_bounded_repeat` | One selection instead of two | Every execution gets a distinct reservation; old capacity is released |
| PostgreSQL `configured_pooled_repeats` | Second pass selected zero entries instead of one | Two selections survive client reconstruction |
| `supplied_research_configuration` | Cannot-reproduce selected without terminal proof | Missing, rejected, stale or mismatched terminal proof escalates |
| PostgreSQL `configured_nested` | Snapshot after successor completion was Unavailable | Historical snapshot uses the common observer/fence contract |

The nested PostgreSQL scenario additionally reproduced and corrected a stale
snapshot check requiring the original attempt owner to complete the node.
It now delegates to the same observer validation as transition and core logic.
Negative coverage rejects a parent result without its child record, and invalid
terminal proof on both defect and feature routes. Queue coverage checks distinct
execution generations, active new capacity and released previous capacity.
Documentation-only corrections do not require a behavioral Red test.

## Explicit Flow and lifecycle cleanup (9.18–9.20)

Publication now requires an explicit Flow, and the published domain type cannot
restore without it. Admission and PostgreSQL publication/restoration retain that
exact closure; all fixed-stage closure builders and fallback calls were removed.
An explicit custom Flow may publish without implementation, validation or
evaluation roles. Role metadata remains only for existing capability projections.

Production reconciliation requires a Flow coordinator. The old global next-action
policy and state-successor matrix are absent from production exports and dispatch.
Historical drivers are isolated in `test-support`/`cfg(test)` fixtures to preserve
candidate/evidence/evaluation/delivery contract coverage. They are not installed by
production composition. Local capability transitions cannot choose another phase;
the store checks starts against the pinned graph and accepted operation facts.

A configured root terminal commits `FlowResolved` with the exact root identity and
arbitrary terminal key, plus technical Run completion. Both stores independently
reconstruct the terminal from retained graph history. Negative coverage rejects
fabricated completion atomically; terminal replay after lease expiry preserves the
snapshot. Same-worker replacement fences and expired-lease successors use the
common observer protocol; current-store fencing remains mandatory.

Commands below used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
Each listed Red was observed before its implementation correction:

| Public test filter | Observed Red | Green |
| --- | --- | --- |
| `publication_requires_an_explicit_flow` | Publication accepted omitted Flow | Publication rejects it |
| `an_explicit_custom_flow_needs_no_fixed_stage_roles` | Empty roles rejected as `configuration stages` | Explicit custom Flow publishes |
| `configured_intake_research_unknown_step_and_pool_share_one_owner_interface_across_restart` | Root terminal left Run Admitted | Exact terminal durably completes Run |
| `invalid_worker_bounds_fail_before_claiming` | Required coordinator argument absent from constructor API | Coordinator is mandatory; bounds still fail before claims |
| `capability_facts_cannot_choose_a_business_successor_without_the_graph` | Local facts permitted implementation → validation | Cross-node authority requires graph validation |
| `projected_attempts_use_the_common_observer_protocol_after_fence_replacement` | Replacement observer rejected | Replacement/successor works; unrelated overlapping worker fails |
| `a_published_configuration_cannot_restore_without_its_explicit_flow` | Missing Flow deserialized successfully | Published type requires Flow |
| Architecture guard test | Fixed closure/decision/successor functions went undetected | All three are rejected outside test support |

Targeted command forms were `cargo test -p octacity-server-factory <filter> --lib`,
`cargo test -p octacity-server-application <filter> --lib`, and
`python3 -m unittest tools.tests.test_check_architecture.ArchitecturePolicyTests.test_configured_factory_runtime_rejects_phase_contracts_and_name_dispatch`.

Final validation: 396 library tests, 23 PostgreSQL contracts and 24 architecture
tests passed. Clippy passed for all five packages with warnings denied; architecture,
OpenSpec, formatting and diff checks passed. The disposable database was removed.

- `cargo test -p octacity-server-factory -p octacity-server-store -p octacity-server-application --lib`.
- With an explicitly disposable PostgreSQL 18 service:
  `cargo test -p octacity-server-store-postgres --test factory_discovery --test factory_retention --test migrations -- --ignored`.
- With the same service:
  `cargo test -p octacity-server --test factory_configured_flow -- --ignored`.
- `cargo clippy -p octacity-server-factory -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres -p octacity-server --all-targets --all-features -- -D warnings`.
- `python3 -m unittest tools.tests.test_check_architecture`,
  `python3 tools/check_architecture.py`, `openspec validate add-dark-factory-mode --strict`,
  `cargo fmt --all -- --check`, and `git diff --check`.

Documentation changes have no executable behavior. No commit, staging or push was requested or performed.


## Common-engine follow-up

Tasks 9.21–9.26 subsequently removed the remaining fixed-role projections,
Stage/MacroCall authority, legacy lifecycle and Build origin exceptions, and
installed the optional supervised common owner. Current verification and scope
boundaries are recorded in [verification-generic-engine.md](verification-generic-engine.md).
The earlier counts above remain the historical checks for their increments.
