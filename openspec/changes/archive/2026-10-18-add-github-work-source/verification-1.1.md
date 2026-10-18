# Task 1.1: admission, status/outbox and Delivery baseline

Status: complete for task 1.1 only.

Verification date: 2026-10-16. Base commit:
`4e20afafc02795042fa3f331fcc37ecdeeb1c74f`.
Toolchain: Rust/Cargo 1.99.0, as selected by `rust-toolchain.toml`.

## Public boundaries retained

- Manual intake uses `AdmitManualFactoryWorkCommand` through
  `FactoryAdmissionHandlers` and the authorized management command handler.
  Observations use the public `FactoryRunStore` snapshot and audit/evidence
  probes of the existing in-memory contract adapter.
- Configured `set_status` uses the ordinary action/node runner and
  `FactoryWorkStatusOutboxWorker`, retaining arbitrary status values, exact
  operation identities, frozen authority, retry limits and authoritative Flow
  history independently of reporting availability.
- GitHub Delivery uses the existing wire decoder, `GithubPlugin`, installed
  process entry point and verified Delivery host. Provider tests use controlled
  HTTP/HTTPS peers and local Git repositories; no live account is involved.

## Baseline before implementation changes

These commands all exited successfully before any production refactoring.
The admission and status suites were run before adding characterization tests.

```shell
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
# 8 passed; 0 failed; 0 ignored.

cargo test --locked --jobs 2 -p octacity-server-application --lib factory_flow_action_tests -- --test-threads=2
# 15 passed; 0 failed; 0 ignored.

cargo test --locked --jobs 2 -p octacity-github -p octacity-delivery-protocol -p octacity-server-delivery -- --test-threads=2
# 21 passed; 0 failed; 0 ignored:
# 4 wire contracts, 14 provider unit tests, 1 credential contract,
# 1 provider-process contract and 1 verified-host contract.
```

The 44 baseline tests retain exact replay before mutable revision resolution,
atomic active-run admission limits, Project/Repository visibility, bounded
metadata, configured arbitrary statuses, alternate action plugins, outbox
recovery/exhaustion, exact candidate publication, uncertain-write recovery,
credential separation, network ceilings, HTTPS trust and process correlation.

## Admission characterization added before reuse

Two tests were added and the selected-dependency visibility test was extended
in `server/application/src/factory_admission_tests.rs`:

1. `replay_after_handler_restart_retains_the_original_intent_when_vcs_is_unavailable`
   reconstructs the handler over the retained store with an unavailable VCS
   resolver. A new transport key and later request time still return the
   original Work/Run, base revision and admission time, with no new persisted
   effects. A fresh identity fails as unavailable. This is handler recovery
   over an in-memory store, not PostgreSQL/process-restart qualification.
2. `replacing_admitted_artifacts_conflicts_without_resolving_or_replacing_the_original_work`
   independently changes task, acceptance and specification inputs. Reuse of
   either the original transport key or the external identity under a new key
   returns Conflict before another VCS resolution. The original snapshot,
   audit and admission remain intact and the original request still replays.
3. `hidden_selected_dependencies_are_rejected_before_factory_revision_resolution`
   retains the Repository check and separately hides the selected Factory
   Configuration, task, acceptance and specification Artifact. Each attempt
   returns the safe Invalid outcome without VCS access, admission or audit.

Individual characterization commands:

```shell
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests::replay_after_handler_restart -- --test-threads=2
# 1 passed; 0 failed.

cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests::replacing_admitted_artifacts -- --test-threads=2
# 1 passed; 0 failed.
```

TDD evidence: this task changes tests and verification documentation only.
There is no new executable product behavior and no production refactoring.
Characterization passed against the existing implementation; there is no
missing-behavior Red/Green cycle to claim. Any later behavior correction or
new Source/Reporter behavior still requires its own observed Red then Green.

## Provider-independent extension constraint

The requested GitLab, Jira and other-provider extensibility follows the
existing [extension boundaries](../../../../docs/architecture/factory-extension-boundaries.md)
and the change design: shared admission/replay, normalized Work/Artifact
inputs, exact target Configuration/Repository and non-authoritative reporting
remain provider-neutral. The target code Repository is independent of the
system supplying a ticket. Provider SDKs, ticket selectors, native status or
workflow translation, network policy and separately scoped credentials stay
inside installed capability adapters, outside the Flow interpreter.

The existing `another_action_plugin_receives_the_common_projected_input_without_a_runtime_change`
contract is retained in the passing status suite. GitHub is the first concrete
Source/Reporter implementation in this change. This baseline does not qualify
a GitLab/Jira adapter, nor does it treat GitHub labels or Issue-state enums as
a universal ticket schema. The reviewed proposal/design/specs remain coherent
with this constraint and need no planning rewrite for task 1.1.

## Final scoped verification

```shell
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
# 10 passed; 0 failed; 0 ignored, including both new tests and the
# extended dependency-visibility characterization.

cargo clippy --locked --jobs 2 -p octacity-server-application --all-targets -- -D warnings
# Exit 0.

cargo fmt --all -- --check
# Exit 0 after formatting the modified test file.

python3 tools/check_architecture.py
# Exit 0: 61 workspace packages, 245 dependency edges.

openspec validate add-github-work-source --strict
# Change is valid.

git diff --check
# Exit 0.
```

The final scoped baseline is 46 passing tests: 10 admission, 15 action/status
and 21 Delivery contracts. Only the admission test module and this change's
verification/task records were edited; production admission, Flow, outbox,
provider implementation and the running local stand were not changed.

An additional broad application run was started with:

```shell
cargo test --locked --jobs 2 -p octacity-server-application --lib -- --test-threads=2
```

That optional run was deliberately interrupted with SIGINT during the longer
requirements scenarios; Cargo exited 101 because the test process received
signal 2. No assertion failure was observed before interruption, but the
384-test suite did not complete and is not reported as passing. The explicit
10-test admission command above completed successfully afterward. Full
repository, PostgreSQL, REST/UI and security regressions remain task 6.4;
this baseline does not qualify new Source/Reporter capabilities or live
GitHub/GitLab/Jira integration.
