# Review follow-up: independent reads and input/action compatibility

This follow-up fixes the two review findings in tasks 5.13–5.14. Earlier completed
tasks and their verification remain intact; task 6 is still pending.

## Independent installed read profile

`WorkSourceReadProfile` now owns the shared installed Source binding, plugin,
configuration, resource, current read access, security context, selector, network
and finite polling policy. Intake composes it as `FactorySourceProfile.read`
alongside separate `preparation`. Preview, observations and verified process
translation consume the same read profile directly. The original Factory
preparation data no longer travels into the public observation constructor.

No Deref compatibility layer, duplicate profile field set, provider-specific type,
new scheduler, credential lookup implementation or task-store dependency was
introduced. The existing native correlation, managed grant checks and original
Work preparation semantics are unchanged. Runtime installation callers now pass
the nested `.read` profile to preview/observation constructors; no persisted or
REST representation changed.

## Input/action compatibility before activation

An active automatic input is checked against every explicit `set_status` source
update in its exact target closure, including nested definitions. Source,
Reporter and the complete connection reference must match the input pins.
Publication and retained Reporter dispatch share `validate_for_input`; Flow
publication and input activation share the same bounded action-parameter reader.
Current grants remain independently required, and Reporter dispatch still uses
the original accepted Work and native item.

Validation/publication return safe `/inputs/{index}/definition/target` diagnostics
for mismatches before scheduling intake. Disabled inputs remain retainable for
repair, but activation requires compatible pins. Corrected, matching pins can
still be published and discovered normally. Manual admission is unaffected.

## Observed Red / Green

Before refactoring, `cargo test --locked -p octacity-server --lib work_source`
passed 71 tests (9 existing opt-in PostgreSQL tests ignored).

| Public boundary | Red before implementation | Green |
| --- | --- | --- |
| Independent read installation | `independent_read_profile_observes_without_factory_preparation` did not compile because the required read-only profile API was absent | A profile containing no acceptance/Artifact/code-source/classification settings reads the known native task through a verified Source process for GitHub and a fake provider, without Work or Artifact publication |
| Automatic input validation/publication | `automatic_input_rejects_a_stage_connection_revision_that_cannot_report_for_its_work` failed because validation returned a successful `PublishWorkSources` after the grant revision changed while the Flow retained old pins | Validation and publication reject the conflicting active input; no scan target is created; publishing a corrected target succeeds |

The positive control validates the original compatible target before the revision
change. Additional public-boundary cases cover another permitted Source profile,
another permitted Reporter profile, absent Reporter, nested actions and disabled
inputs for both GitHub and a fake provider. Invalid test-fixture compilation was
corrected before observing the behavioral Red; environment failures were not
counted as Red. No timeout increase or test serialization was used.

## Commands and results

Cargo builds used the existing lockfile and bounded disk/concurrency settings:

```sh
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2

# Red commands; corresponding tests then passed in the Green regressions below.
cargo test --locked -p octacity-server --lib independent_read_profile_observes
cargo test --locked -p octacity-server-application --lib automatic_input_rejects_a_stage_connection_revision

cargo test --locked -p octacity-server --lib work_source
# 72 passed; 9 existing explicitly opt-in PostgreSQL tests ignored.
cargo test --locked -p octacity-server-application --lib factory_admission_tests
# 84 passed, including independent manual intake and existing Source replay/reconciliation.
cargo test --locked -p octacity-server-application --lib factory_configuration_cqrs_tests
# 20 passed.
cargo test --locked -p octacity-server-application --lib factory_flow_action_tests
# 20 passed, including intermediate/nested reporting and retained outbox retry.
cargo test --locked -p octacity-server-application --lib source_configuration::reporting
# 4 passed after strengthening the corrected-publication positive control.
cargo test --locked -p octacity-server --lib work_reporter_tests
# 4 passed, including the original absolute-deadline regression.
cargo test --locked -p octacity-server-api-rest
# 134 passed across library, administration, public errors and handlers.
cargo clippy --locked -p octacity-server -p octacity-server-application --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
python3 tools/security_matrix.py validate
python3 tools/verify_local_stand_context.py
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
git diff --check
```

The refactor changes no provider binary, wire contract, database schema, UI or
manual admission identity. Prior native HTTPS and browser evidence remains in the
earlier verification reports; those unrelated suites were not rerun here. No
live external service, image download or stand mutation was needed.
Documentation-only edits have no executable behavior; OpenSpec and whitespace
checks apply. Work stayed on `main`, with no commit requested or created. Disk
space was checked throughout and remained approximately 13 GiB free. Existing
caches, stand state and earlier uncommitted changes were preserved.
