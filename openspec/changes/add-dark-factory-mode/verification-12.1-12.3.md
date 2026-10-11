# Independent verification plans and source-free checks

Tasks 12.1–12.3 add verification policy and evidence qualification to the common
immutable Flow/Build contracts. Node keys, classes, ordering, quorum, projections,
profiles, thresholds and transitions remain configuration. The runtime has no
business-stage dispatch. The supplied nested template and tool installation
contract are documented in
[`verification-flow/README.md`](../../../config/factory/flows/verification-flow/README.md).

The common plan binds the original admission, actual Flow invocation and workflow
cycle, exact candidate, immutable policy and its projection recipes, profiles and
budgets. Each selected check separately binds its deployment receipt, public
contract, endpoint and own evidence to the exact frozen input digest, including
execution generation. Intent restoration recomputes these identities against
trusted admission, rather than consulting current configuration. Qualification uses
existing retained Build outputs and Evidence Manifests, including exact input
Artifact references. A required failure cannot be overridden by reasoning or
quorum; absent/stale/indeterminate required evidence cannot pass.

Black-box checks reject undeclared Context categories, Artifacts, mounts, hosts
and credential/identity grants. The signed ordinary Job may omit source authority;
the Agent then projects no host source into the sandbox and uses scratch as the
working directory. Evaluation may still explicitly select read-only source.
Capture/materialization without a source grant fail closed. This is one technical
capability used by the existing backends, not a verification-specific executor.

Six parameter documents exercise real deterministic HTTP contract sequences,
latency, public HTML/accessibility, security headers/denied access, exact deployed
candidate/receipt and finite post-deployment observations. The tool emits only
bounded typed observations. The application qualifies all seven command/reasoning
Build completions against actual retained execution identities and tests complete
success and required failure despite passing reasoning. The template terminal
records observations; qualified plan evaluation supplies acceptance authority.

Full caller/journey wiring is task 12.7. Deployment mutation and promotion are
12.4–12.6. These tests do not qualify live model providers, deployment services or
released Linux VMs; production composition/security qualification remains
16.1/16.4. OCI source visibility code is updated and Linux fixture initializers
remain compatible, but Linux-only backend tests cannot run on this macOS host.
No schema migration, dependency or lock-file change is needed.

## Initial implementation TDD evidence

Rust commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
Each row records an observed behavioral failure or missing required public API
before its implementation. Fixture mistakes and environment failures do not
count as Red. Logs are local under `/tmp`; the test names identify repeatable
repository boundaries. Configuration behavior is tested before editing it.

| Command / public boundary | Observed Red | Observed Green / log prefix under `/tmp` |
| --- | --- | --- |
| `cargo test -p octacity-server-factory --lib verification_plan_freezes` | Required immutable plan API absent | Admission/configuration replacement, exact subject and restore checked: `octacity-12.1-plan-` |
| `cargo test -p octacity-server-factory --lib verification_definition_rejects_unknown` | Unknown checks and impossible quorum accepted | Rejected before admission: `octacity-12.1-policy-` |
| `cargo test -p octacity-server-factory --lib verification_check_targets_are_selected` | Required frozen-input plan API absent, then intent did not retain plan | Dynamic targets and tamper-resistant intent restore pass: `octacity-12.1-target-`, `octacity-12.1-intent-` |
| `cargo test -p octacity-server-application --lib ordinary_verification_build` | Build causality lacked plan | Both command and reasoning ordinary dispatch retain it: `octacity-12.1-build-` |
| `cargo test -p octacity-server-factory --lib supplied_verification_flow` | Requested template missing | Seven configured checks and failure routing pass: `octacity-12.1-template-` |
| `cargo test -p octacity-server-factory --lib verification_plan_binds_the_exact_endpoint` | Required exact endpoint contract absent | Endpoint substitution changes/restoration rejects plan: `octacity-12.1-endpoint-` |
| `cargo test -p octacity-server-factory --lib verification_plan_does_not_reuse` | Different cycles shared one plan digest | Fresh cycle cannot reuse proof: `octacity-12.1-cycle-scope-` |
| `cargo test -p octacity-server-factory --lib verification_observation_binds` | Typed observation API absent | Exact plan/check and measurement bounds verified: `octacity-12.2-observation-` |
| `cargo test -p octacity-server-application --lib verification_evidence_qualifies` | Qualified verification proof/evaluator APIs absent | Exact provenance, required failure, freshness and duplicates checked: `octacity-12.2-qualified-evidence-` |
| `cargo test -p octacity-server-application --lib verification_build_cannot_commit` | Foreign plan/check report accepted | Rejected before journaling: `octacity-12.2-report-scope-` |
| `cargo test -p octacity-server-application --lib verification_evidence_qualifies` | Evidence input list omitted the selected public contract | Exact inputs retained: `octacity-12.2-evidence-inputs-` |
| `cargo test -p octacity-server-factory --lib black_box_verification_rejects` | Implementation Context accepted | Repository, typed implementation and summary Context rejected: `octacity-12.3-context-` |
| `cargo test -p octacity-server-factory --lib black_box_verification_can_inspect` | Declared own evidence unnecessarily rejected | Only exact selected evidence accepted: `octacity-12.3-evidence-context-` |
| `cargo test -p octacity-server-factory --lib verification_definition_rejects_undeclared` | Undeclared network/credential/identity grants accepted | Rejected at publication: `octacity-12.3-grants-` |
| `cargo test -p octacity-server-application --lib compiler_supports_source_free` | Source-free evaluation rejected | Three-mount evaluation compiles: `octacity-12.3-source-codex-` |
| `cargo test -p octacity-server-application --lib black_box_build_rejects_source` | Forbidden source/other mounts dispatched | Rejected before external dispatch: `octacity-12.3-source-dispatch-` |
| `cargo test -p octacity-server-application --lib verification_dispatch_rejects_endpoint` | Endpoint substitution/embedded credentials dispatched | Rejected before network authority: `octacity-12.3-endpoint-grants-` |
| `cargo test -p octacity-server-application --lib verification_endpoint_uses_the_existing_canonical_ipv6` | Canonical IPv6 grant rejected | Exact endpoint accepted: `octacity-12.3-ipv6-` |
| `cargo test -p octacity-protocol --lib v3` | Signed source-free layout rejected | Optional source authority and forbidden source operations checked: `octacity-12.3-source-protocol-` |
| `cargo test -p octacity-execution-microsandbox --lib` | Required source visibility API absent | Public sandbox plan omits source and uses scratch: `octacity-12.3-source-backend-` |
| `python3 -m unittest discover -s tools/tests -p test_factory_verification_probe.py` | Tool/typed sequence API absent | Real HTTP sequence retains failure: `octacity-12.2-probe-` |
| Same Python suite | Headers, denied-access, latency and finite repeat assertions absent | Assertions and pre-network bound rejection pass: `octacity-12.2-bounded-probe-` |
| Same Python suite | Deployment JSON assertions absent | Exact selected release checked: `octacity-12.2-deployment-probe-` |
| Same Python suite | Frozen-target assertion selector absent; six requested profiles missing | All six profiles produce typed observations: `octacity-12.2-target-assertion-red.log`, `octacity-12.2-presets-` |
| Same Python suite | CLI emitted no typed report | Declared JSON stdout and oversized-input rejection pass: `octacity-12.2-cli-` |
| Same Python suite | Streaming body overran observation window | Bounded incremental observation passes: `octacity-12.2-window-` |
| Same Python suite | Profile base URL could replace the frozen endpoint | Rejected before network: `octacity-12.3-probe-target-` |
| Same Python suite | Changed deployment digest with unchanged name passed | Full exact receipt and ChangeSet assertions pass: `octacity-12.2-exact-deployment-` |

Each paired prefix has `red.log` and `green.log`, except the explicitly named
target-selector Red followed by the profile Green. Existing evidence tests passed
before fixture-only refactoring (`octacity-12.2-evidence-baseline.log`). Additional
characterization/integration tests exercise ordinary source-free Agent execution
and the seven-profile composition through the already-tested public contracts.
Documentation and formatting have no executable behavior; TDD does not apply to
those edits.

## Initial implementation checks

| Command | Result / log under `/tmp` |
| --- | --- |
| `cargo test -p octacity-server-factory -p octacity-server-application -p octacity-protocol -p octacity-execution -p octacity-execution-microsandbox -p octacity-job --all-features --lib` | Initial 561 passed: `octacity-12-regression.log` |
| `cargo test -p octacity-server-factory --lib` | Final 233 passed, including cycle correction: `octacity-12-core-complete.log` |
| `cargo test -p octacity-job --all-features --lib` | Final 43 passed, including source-free Job characterization: `octacity-12-agent-regression.log` |
| `cargo test -p octacity-server-application --all-features --lib` | Final 206 passed: `octacity-12-app-complete.log` |
| Python suite above | Final nine passed: `octacity-12-probe-complete.log` |
| `cargo check --workspace --all-targets --all-features --locked` | All final workspace targets checked, including previous CI consumer: `octacity-12-workspace-check-final.log` |
| `cargo clippy -p octacity-server-factory -p octacity-server-application -p octacity-protocol -p octacity-execution -p octacity-execution-microsandbox -p octacity-execution-containerd -p octacity-job --all-targets --all-features -- -D warnings` | Passed: `octacity-12-clippy-complete.log` |
| `RUSTDOCFLAGS='-D warnings -D missing_docs' cargo doc -p octacity-server-factory -p octacity-server-application -p octacity-protocol -p octacity-execution --all-features --no-deps` | Passed: `octacity-12-rustdoc.log` |
| `cargo test -p octacity-server-factory -p octacity-server-application -p octacity-protocol -p octacity-execution --doc` | Four documentation contracts passed: `octacity-12-doctests.log` |

Together, the final affected Rust suites cover 565 tests. Architecture validation
passes with 56 workspace packages and 211 dependency edges. Strict OpenSpec,
`cargo fmt --all -- --check` and `git diff --check` pass. When free disk space
fell below the authorized 5 GiB threshold after all builders finished, only the
repository `target` build output was removed. Test logs remain under `/tmp`.

The earlier CI fixture correction was reproduced as missing `cumulative_changes`,
then fixed with `None` and verified through 42 Job tests plus Clippy/formatting.
Only that one-line fix was committed as
`4d90334 test(agent): update capture fixture for cumulative manifest`.
Verification task changes remain uncommitted.

## Review fixes

All five review findings are corrected without adding node-name dispatch or another
lifecycle model:

1. HTTP JSON assertions compare types recursively, distinguish booleans from
   numbers, and reject malformed JSON Pointer escapes and array indices.
2. Verification selection applies only to the policy's explicitly selected Build
   nodes. Unselected helper Builds keep their ordinary contracts.
3. Target selection and retained Evidence Manifests share one canonical inventory
   validator. It counts the public contract, rejects duplicate Artifact identities
   regardless of digest, and enforces the existing 256-entry retention ceiling.
4. Malformed HTTP protocol responses yield typed indeterminate observations with
   no response details or traceback. Earlier failures remain failures.
5. The common immutable plan contains policy metadata; each check resolves only
   its own frozen input. Different schemas and later own evidence no longer change
   the common plan. Reports also bind the exact input digest, preventing reuse
   across retries or substituted targets.

### Review-fix TDD evidence

The following targeted commands were run before and after their implementation.
Python commands start with `python3 -m unittest discover -s tools/tests
-p test_factory_verification_probe.py`; Rust commands use the debug settings above.
Logs use the prefix `/tmp/octacity-12-fixes-`.

| Command suffix / public boundary | Observed Red | Green log |
| --- | --- | --- |
| Python `-k distinguish` | Numeric expectations accepted scalar and nested booleans | `json-green.log` |
| Python `-k json_pointer` | Invalid array indices and escapes were accepted | `pointer-green.log` |
| Python `-k malformed_http` | CLI exited with an uncaught malformed status-line error | `probe-green.log` |
| Python `-k frozen_input` | Protected configuration rejected the required exact input identity | `probe-green.log` |
| `cargo test -p octacity-server-factory --lib a_verification_flow_can_execute` | Ordinary unselected helper intent failed verification validation | `helper-green.log` |
| Same crate, `--lib checks_with_distinct_inputs` | First check attempted to resolve another node's missing Context projection | `independent-input-green.log` |
| Same crate, `--lib verification_observation_cannot_be_reused` | Required per-check input API and report input digest were absent | `core-green.log` |
| Same crate, `--lib verification_rejects_impossible` | A contract duplicated in selected evidence was admitted | `core-green.log` |
| Same crate, `--lib a_verification_target_has_one` | Required complete retained-inventory boundary was absent | `core-green.log` |
| `cargo test -p octacity-server-application --lib configured_checks_publish` | Closed observation schema rejected the required input digest | `report-schema-green.log` |

The inventory test also checks one Artifact identity with different digests.
The retained-inventory boundary accepts 255 extra Artifacts plus the contract and
rejects 256 extras. Ordinary closed payload arrays have a stricter 128-item ceiling;
invalid oversized fixtures were not counted as behavioral Red. Existing retained
Evidence Manifest behavior passed before extracting its validator
(`input-inventory-baseline.log`, `evidence-baseline.log`).

The seven-check application composition exercises distinct schemas and an actual
earlier retained evidence Artifact through ordinary Build completion and proof
qualification (`composition-green.log`). Reports for a different frozen input
are rejected before journaling. Documentation and formatting changes have no
executable behavior and are validated by the documentation and formatting checks.

### Review-fix regression checks

| Command | Result / log under `/tmp` |
| --- | --- |
| `cargo test -p octacity-server-factory --lib` | 238 passed: `octacity-12-fixes-core-green.log` |
| `cargo test -p octacity-server-application --all-features --lib` | 207 passed: `octacity-12-fixes-app-regression.log` |
| `python3 -m unittest discover -s tools/tests -p test_factory_verification_probe.py` | 13 passed: `octacity-12-fixes-probe-green.log` |
| `cargo check --workspace --all-targets --all-features --locked` | Passed: `octacity-12-fixes-workspace.log` |
| `cargo clippy -p octacity-server-factory -p octacity-server-application --all-targets --all-features -- -D warnings` | Passed: `octacity-12-fixes-clippy-final.log` |
| `RUSTDOCFLAGS='-D warnings -D missing_docs' cargo doc -p octacity-server-factory -p octacity-server-application --all-features --no-deps` | Passed: `octacity-12-fixes-rustdoc.log` |
| `cargo test -p octacity-server-factory -p octacity-server-application --doc` | Four passed: `octacity-12-fixes-doctests.log` |

The corrected core/application suites total 445 passing Rust tests, plus 13 Python
tests and four documentation contracts. Architecture validation still passes with
56 packages and 211 edges. Strict
OpenSpec, final formatting and whitespace checks pass. Free space remained above
5 GiB during this correction; no cleanup was needed. These fixes remain uncommitted.
