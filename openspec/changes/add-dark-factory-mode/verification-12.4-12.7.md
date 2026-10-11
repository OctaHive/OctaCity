# Configurable promotion and the nested release journey

Tasks 12.4–12.7 extend the common immutable Flow/Build contracts. A promotion
strategy is an operator-owned value understood by the installed deployment
capability, rather than a Factory runtime enum or business-node switch. The
editable example is documented in
[`promotion-flow/README.md`](../../../config/factory/flows/promotion-flow/README.md).

`PromotionPlan` binds the original admitted closure/limits, complete frozen
input including cycle and generation, exact candidate from trusted Context,
environment contracts, structured allocation, finite observation window, evidence
requirements, node budget and the actual configured disposition node, including
finite outcomes, bindings and its deterministic program where applicable.
Restoration recomputes these identities against original admission.
Production without predecessor gates requires explicit operator permission and
immutable low-risk Work. Required gates select exact committed outcomes through
the common input bindings; owner and store independently verify those sources.

`FactoryPromotionBuildAdapter` qualifies execution around the existing ordinary
Build adapter. The common intent retains the exact action binding. CI/CD receives
bounded protected `action_input` containing the frozen plan and its digest, so it
does not need to read Factory storage or reconstruct policy from model output.
The common engine does not interpret that document or select a rollout strategy.
Lost-response recovery observes the same operation before dispatching again;
CI/CD owns deployment identity, rollout mechanics, credentials and rollback.
Model/ordinary command causality has no action or protected action input.

Typed observations require actual independently retained Build provenance,
exact input/plan/candidate/environment identities and bounded timestamps. A
passing observation covers the full selected window; failure may finish early.
The action checks output freshness before completion. The configured decision
also checks freshness at the authoritative owner decision time, preventing a
previously passing but expired observation from authorizing continuation.
Finite deterministic/human outcomes and optional bounded Decision Signals use
existing contracts. A signal cannot add a route after failed evidence.

The full owner contract starts with ordinary authorized manual admission. It
executes two-phase triage, research and specification authoring/review, protected
test-first implementation, validation and independent review, seven black-box
verification checks, candidate review publication, human approval, nested rollout
and bounded observation. The failure path covers small accepted-contract Work,
rollback and external monitoring submitting new Work through normal admission
and both triage phases. The old run remains unchanged. Owner runners are recreated
between ticks to exercise recovery from retained state. A separate admitted
configuration test adds, removes and reverses optional nodes without runtime edits.

This journey exposed two aggregate/local budget mistakes in generic Build intent
reservation and completion. Aggregate consumption now uses the admission budget;
owner and store separately enforce the owning Flow's local reservation from
authoritative history. Sibling Flow consumption cannot exhaust a child's local
budget. No new lifecycle fields or duplicate budget journals were introduced.
The final diff check also preserved the existing proof requirement for command
and reasoning nodes when adding Build-backed trusted actions.

These are owner/application contracts using installed-capability test ports,
not live deployment-service qualification. Concrete GitHub Delivery is task 13;
root installation/live backend qualification remains task 16.1. No new dependency,
lock-file change or migration is required. No commit is created for these tasks.

## TDD evidence

Rust commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
Each row records a test run before production behavior changed. Missing required
public APIs count as Red; fixture errors and environment failures do not. Logs
are local under `/tmp`; the commands identify repeatable repository boundaries.

| Command / public boundary | Observed Red | Green log prefix under `/tmp` |
| --- | --- | --- |
| `cargo test -p octacity-server-factory --lib promotion_plan_freezes` | Immutable plan API absent | `octacity-12.4-plan-` |
| Same crate, `--lib promotion_plan_rejects` | Empty environment selection accepted | `octacity-12.4-plan-bounds-` |
| Same crate, `--lib production_without_gates` | Required authorization API absent | `octacity-12.4-all-green.log` |
| Same crate, `--lib trusted_actions_can_select` | Valid capability-matched action Build rejected | `octacity-12.5-build-binding-` |
| Same crate, `--lib promotion_gate_cannot` | An action result accepted as a required gate | `octacity-12.5-gate-kind-` |
| Same crate, `--lib build_intent_freezes_action` | Intent action API absent | `octacity-12.5-intent-action-` |
| `cargo test -p octacity-server-application --lib ordinary_build_receives_the_exact_frozen` | Ordinary dispatch lacked exact action causality | `octacity-12.5-action-dispatch-` |
| `cargo test -p octacity-server-factory --lib promotion_observations_bind` | Typed bounded observation API absent | `octacity-12.6-observation-` |
| `cargo test -p octacity-server-application --lib promotion_observes_the_same` | Trusted promotion Build adapter absent | `octacity-12.5-promotion-adapter-` |
| Same crate, `--lib trusted_action_build_completion` | Actual Build proof rejected for trusted action completion | `octacity-12.5-provenance-` |
| `cargo test -p octacity-server-factory --lib a_build_backed_trusted_action` | Provider-only action record could complete | `octacity-12.5-no-proof-` |
| `cargo test -p octacity-server-application --lib promotion_completion_rejects` | Foreign-input CI report accepted | `octacity-12.6-qualified-observation-` |
| Same command, extended freshness case | Expired Build proof accepted | `octacity-12.6-freshness-` |
| `cargo test -p octacity-server-factory --lib promotion_criteria_are` | Actual criterion/budget API absent | `octacity-12.4-criteria-` |
| Same crate, `--lib supplied_rollout_flow` | Requested repository configuration absent | `octacity-12.7-rollout-config-` |
| Same crate, `--lib supplied_observation_policy` | Expired passing report still selected continuation | `octacity-12.6-decision-freshness-` |
| Same crate, `--lib build_intent_reserves_the_factory` | Aggregate sibling usage charged to local Flow, yielding `BudgetExceeded` | `octacity-12.7-nested-budget-` |
| Same crate, `--lib build_reservation_keeps` | Local reservation verification API absent | `octacity-12.7-local-reservation-` |
| `cargo test -p octacity-server-application --lib build_completion_charges_aggregate` | Completion rejected aggregate usage below admission ceiling but above local ceiling | `octacity-12.7-completion-budget-` |
| Same crate, `--lib promotion_observes_the_same` | Complete protected plan input missing from ordinary CI causality | `octacity-12.5-ci-plan-input-` |
| `cargo test -p octacity-server-factory --lib omitting_a_build_binding` | Reasoning record without actual Build proof could complete when binding was absent | Red: `octacity-12-final-proof-red.log`; Green: `octacity-12-core-final.log` |

The corresponding paired prefixes contain `red.log` and `green.log`, except the
explicitly named combined Green logs. All seven strategy choices plus an arbitrary
plugin choice pass through the same plan boundary. Risk-class and bounded-signal
tests characterize the existing immutable Work/Decision Signal contracts.
Configuration add/remove/reorder and the full nested journey compose those tested
contracts. Core behavioral tests passed before extracting the shared finite
criterion validation; existing specification/Build fixtures are reused.
Documentation and formatting have no executable behavior, so TDD does not apply
to those edits; their relevant checks are listed below.

## Regression and repository checks

| Command | Result / log under `/tmp` |
| --- | --- |
| `cargo test -p octacity-server-factory --lib` | 255 passed: `octacity-12-core-final.log` |
| `cargo test -p octacity-server-application --lib -- --test-threads=4` | 215 passed: `octacity-12-application-regression.log`; the final proof-preservation change is additionally covered by the targeted Build/promotion checks below |
| `cargo test -p octacity-server-store --all-features` | 68 unit tests and one documentation contract passed: `octacity-12-store-regression.log` |
| `cargo test -p octacity-server-application --lib factory_release_journey_tests -- --nocapture` | All three nested journey/configuration contracts passed: `octacity-12.7-journey-final.log` |
| Same crate, `--lib factory_node_build_tests` | Final 24 Build contracts passed after the proof-preservation fix: `octacity-12-build-final.log` |
| Same crate, `--lib factory_promotion_tests` | Final four promotion contracts passed: `octacity-12-promotion-final.log` |
| `cargo test -p octacity-server-factory -p octacity-server-application --doc --all-features --locked` | Four documentation contracts passed: `octacity-12-doctests-final.log` |
| `cargo check --workspace --all-targets --all-features --locked` | Passed before the final protected-plan input extension: `octacity-12-workspace-check.log`; final all-target compilation is also covered by the Clippy command below |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Passed on final code: `octacity-12-clippy-final.log` |
| `RUSTDOCFLAGS='-D missing_docs' cargo doc --workspace --all-features --no-deps --locked` | Passed: `octacity-12-rustdoc.log` |
| `python3 tools/check_architecture.py` | 56 packages and 211 dependency edges pass: `octacity-12-architecture.log` |
| `python3 -m unittest discover -s tools/tests` | 316 tests run, five skipped, no failures: `octacity-12-tools-tests.log` |

Strict OpenSpec validation, `cargo fmt --all -- --check` and `git diff --check`
also pass. Disk space remained above the authorized 5 GiB cleanup threshold;
no build-output deletion was necessary for these tasks.

The complete core/application/store regression suites cover 538 passing Rust unit
tests. The final targeted Build/promotion suites and five documentation contracts
also pass. Tasks 12.4–12.7 are complete; overall plan progress is 95/121 tasks.

## Review fixes

Both review findings are corrected in the promotion capability, without changing
the generic owner/control lifecycle or adding flow/node-name dispatch:

1. Observation measurements must end no later than the actual publication of the
   immutable primary Build receipt. The common Build contract already requires
   publication before trusted verification and verification before owner time.
   Comparing with publication also rejects an old report reverified after the
   claimed future measurement window. Positive fixtures now publish and verify
   receipts when their observations actually finish, separately from Build creation.
2. `PromotionPlan.criteria` freezes the selected existing `FlowNodeDefinition`,
   including its finite outcomes and input bindings. Deterministic criteria still
   validate their exact built-in program; human criteria use existing authorized
   control and append-only completion. Reasoning/action nodes cannot serve as
   criteria. The protected CI input carries this complete frozen declaration.
   This corrects the unreleased Factory plan format; no compatibility fallback or
   additional lifecycle type is introduced.

The human journey retains the actual configured health/freshness policy before
making human continuation eligible, selects a separate post-observation Human Gate
as the plan criterion, and asserts that the owner waits until its exact authorized
response. Core plan construction also exercises a Human Gate with no deterministic
node at all, proving that a dummy evaluator is unnecessary.

### Review-fix TDD evidence

Rust commands use the debug settings above. The following failures were observed
before the corresponding production change:

| Command | Observed Red | Observed Green / log under `/tmp` |
| --- | --- | --- |
| `cargo test -p octacity-server-application --lib promotion_cannot_accept_measurements` | A report published/verified at 30 describing measurements ending at 60 was accepted when observed at 61 | `cargo test -p octacity-server-application --lib factory_promotion_tests`: all five passed; `octacity-12-fixes-chronology-red.log`, `octacity-12-fixes-chronology-green.log` |
| `cargo test -p octacity-server-factory --lib promotion_freezes_a_configured_human` | A configured Human Gate criterion failed with `InvalidConfiguration: Flow gate program` | `cargo test -p octacity-server-factory --lib promotion_plan_tests`: all 15 passed; `octacity-12-fixes-human-red.log`, `octacity-12-fixes-human-green.log` |

The chronology test also rejects the same earlier bytes reverified at 60; the
existing positive adapter case accepts publication/verification exactly at the
measurement boundary. Human plan tests retain both finite outcomes and restore
the exact immutable plan. The full human journey characterizes the existing
control contract after the newly supported plan construction. Fixture compile
mistakes are not counted as Red. Documentation-only edits have no executable
behavior and are validated through the checks below.

### Review-fix regression checks

| Command | Result / log under `/tmp` |
| --- | --- |
| `cargo test -p octacity-server-factory -p octacity-server-store --all-features` | 256 core tests, 68 store tests and two documentation contracts passed: `octacity-12-fixes-core-store.log` |
| `cargo test -p octacity-server-application --lib -- --test-threads=4` | All 217 tests passed, including all four release journeys and requirements/development rework: `octacity-12-fixes-application.log` |
| `cargo test -p octacity-server-application --lib a_configured_human_post_observation` | Full manual Work/specification/implementation/verification/delivery/rollout journey waits for the exact authorized human post-observation response, then completes: `octacity-12-fixes-human-journey.log` |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Passed: `octacity-12-fixes-clippy.log` |
| `RUSTDOCFLAGS='-D warnings -D missing_docs' cargo doc --workspace --all-features --no-deps --locked` | Passed: `octacity-12-fixes-doc.log` |
| `cargo test -p octacity-server-application --doc --all-features --locked` | Three documentation contracts passed: `octacity-12-fixes-app-doc-tests.log` |

No commit is created. Architecture validation still passes with 56 packages and
211 dependency edges. Strict OpenSpec validation, formatting and whitespace
checks pass. Free space remains above the authorized 5 GiB cleanup threshold.

The final affected suites total 541 passing Rust unit tests and five passing
documentation contracts. Both review findings are resolved; free space is about
20 GiB and no build-output cleanup was needed.
