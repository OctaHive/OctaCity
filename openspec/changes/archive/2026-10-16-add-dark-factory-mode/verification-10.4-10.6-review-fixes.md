# Review corrections for tasks 10.4–10.6

The six review findings are addressed without adding business-stage dispatch or
changing the configured journey into an engine-owned flow.

## Corrected contracts

1. Execution feedback cannot substitute the accepted specification or its
   candidate. Generic predecessor bindings can explicitly select a producer's
   frozen input using `input_payload`. The supplied gates compare the complete
   specification against that input and bind independently qualified facts to
   the exact candidate. The unspecified route requires both specifications to
   be absent and binds facts to the actual execution revision. Explicit `absent`
   predicates preserve the existing unknown semantics for missing comparison
   operands; separate ordered rules handle the optional cases.
2. A bounded restart remains restorable after an already started sibling
   finishes. Both current progress and restored cycle boundaries select the
   lowest attempt number when several branches request a restart. A substituted
   cause is rejected. Historical repeat counts use the preceding execution
   prefix, so later cycles cannot change an earlier boundary.
   A restart edge cannot authorize its target within the preceding cycle;
   runtime-history validation only admits ordinary edges there and validates
   the restart entry through the separately retained cycle boundary.
3. A typed Requirements Defect requires a nonempty bounded list of exact
   evidence references. The independent verifier resolves/checks those exact
   references against the execution subject and returns qualified references in
   `requirements_evidence`. The configured gate compares both lists exactly;
   reason-only reports, empty evidence and altered digests cannot authorize
   correction.
4. Queue readiness uses the common interpreter's complete ready-node set, so
   both fanout siblings can be published. Progress and readiness share one
   control calculation. Inspection does not reserve capacity for historical
   fanout directives again; actual dispatch retains its separate WIP check.
5. Runtime history validates the admitted closure once and passes that validated
   contract into cycle validation. This is a behavior-preserving refactor, made
   after the complete core/store behavioral baseline passed.
6. Architecture and configuration documentation describe implemented
   specification authoring/review/publication/readiness and distinguish the
   remaining protected-test and production/provider integration tasks.

## TDD evidence

Commands below follow `cargo test` and use
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`. Logs are under `/tmp`.

| Public boundary and command | Observed Red before implementation | Green log |
| --- | --- | --- |
| `-p octacity-server-factory --lib a_predecessor_can_explicitly_select_the_frozen_input_without_trusting_its_result` | The explicit `input_payload` selector was unsupported | `octacity-review-fix-input-green.log` |
| `-p octacity-server-factory --lib execution_feedback_cannot_replace_the_frozen_specification_or_its_candidate` | The gate contract lacked independently selected trusted input | `octacity-review-fix-lineage-green.log` |
| `-p octacity-server-factory --lib explicit_optional_field_absence_does_not_change_unknown_fact_semantics` | The explicit absence predicate was unsupported | `octacity-review-fix-absence-green.log` |
| `-p octacity-server-factory --lib a_restart_remains_restorable_after_a_declared_sibling_finishes` | A later-numbered completed sibling invalidated the restart; reversed restored row order then selected a different cause | `octacity-review-fix-restart-green.log`, `octacity-review-fix-restart-order-green.log` |
| Same public complete runtime-history contract | A restart edge admitted its target in the preceding cycle | `octacity-review-fix-restart-boundary-green.log` |
| `-p octacity-server-factory --lib a_requirements_defect_requires_exact_independently_verified_evidence_references` | A reason-only report was accepted by the closed contract | `octacity-review-fix-evidence-green.log` |
| `-p octacity-server-factory --lib every_ready_fanout_sibling_remains_visible_without_reserving_existing_work_again` | A complete readiness query was absent | `octacity-review-fix-frontier-green.log` |
| `-p octacity-server-application --lib every_ready_fanout_sibling_can_be_published_to_its_configured_pool` | Public store publication rejected a declared ready sibling | `octacity-review-fix-pool-green.log` |

Corresponding Red logs use the same names with `-red.log`. The complete
configured gate regression also passes in `octacity-review-fix-evidence-green.log`.
The history refactor followed a passing 201-test core and 68-test store baseline
in `octacity-review-fix-refactor-baseline.log`. Documentation has no executable
behavior, so TDD does not apply to that correction.

## Final verification

Final regression results are recorded after all corrections, including the
history refactor. The PostgreSQL suite uses a task-owned disposable PostgreSQL
18 service. Its separate PostgreSQL/MinIO production composition test remains
outside this scope, as documented in the original verification report.

Verification commands (with the same debug-profile environment above):

```sh
cargo test -p octacity-server-factory -p octacity-server-store -p octacity-server-application --lib
cargo test -p octacity-server --test factory_configured_flow -- --skip requirements_contract
cargo test -p octacity-server --test factory_configured_flow -- --ignored --skip production_factory_owner_uses_installed_bindings_and_recovers_the_same_arbitrary_node
cargo test -p octacity-server --test factory_configured_flow -- --ignored --skip production_factory_owner_uses_installed_bindings_and_recovers_the_same_arbitrary_node --skip specification_publication_and_correction_recover_exact_contracts_in_postgres
cargo test -p octacity-server --test factory_configured_flow declared_correction_cycles_retain_exact_causality_across_postgres_restart -- --ignored
cargo test -p octacity-server-store-postgres --test factory_discovery -- --ignored
cargo test -p octacity-server-application -p octacity-server-factory --doc
cargo clippy -p octacity-server-factory -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres -p octacity-server --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D missing_docs' cargo doc --no-deps --all-features -p octacity-server-factory -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres -p octacity-server
python3 tools/check_architecture.py
openspec validate add-dark-factory-mode --strict
cargo fmt --all -- --check
git diff --check
```

The complete PostgreSQL matrix passed before the additional same-cycle route
guard. After that guard, all 26 other PostgreSQL contracts and the focused
correction/restoration contract were rerun against the final implementation.

| Regression boundary | Passing tests | Log under `/tmp` |
| --- | ---: | --- |
| Application, including full requirement paths and both correction origins | 179 | `octacity-review-fixes-final-libraries.log` |
| Factory core | 201 | `octacity-review-fixes-final-libraries.log` |
| Common store | 68 | `octacity-review-fixes-final-libraries.log` |
| Configured owner portable contracts | 18 | `octacity-review-fixes-final-portable.log` |
| Complete PostgreSQL configured-flow suite, including publication/correction matrix | 27 | `octacity-review-fixes-postgres.log` |
| PostgreSQL store/discovery/diagnostics | 13 | `octacity-review-fixes-store-postgres.log` |
| Application/Factory doctests | 4 | `octacity-review-fixes-doctests.log` |

These suites total **510 passing tests**, with no failures. The final PostgreSQL
reruns also pass: 26 regression contracts in
`octacity-review-fixes-final-postgres-regression.log` and the focused correction
contract in `octacity-review-fixes-final-postgres-correction.log`.

Strict Clippy (`octacity-review-fixes-final-clippy.log`) and public API
documentation with missing-doc errors (`octacity-review-fixes-final-doc.log`)
pass. Architecture validation passes for 56 packages and 211 dependency edges;
strict OpenSpec validation, formatting and diff whitespace checks pass.

The disposable PostgreSQL service has been removed. Approximately 14 GiB
remain free; the user-authorized 5 GiB cleanup threshold was not reached.
The lock-file fix is the separate commit `0ec2140`.
