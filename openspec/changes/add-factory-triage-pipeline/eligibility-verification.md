# Task 5.2: evidence-backed eligibility policy verification

This records the initial task-5.2 qualification. The subsequent
[review corrections](review-fixes-verification.md) replace the repeated candidate
JSON with a unique verified detail claim and supply the compiled instruction
contract; that document describes the current behavior and final regression run.

## Delivered behavior

The versioned triage manifest now selects `eligibility-results-v1.json` and
`eligibility-gate-v1.json` instead of the base Flow's simpler eligibility
observation/facts gate. This is configuration of the existing schema, native MCP
result verifier, immutable record and declarative evaluator contracts. Task 5.2
adds no production Rust, dispatcher, provider search adapter, mutation tool,
credential owner, duplicate index or journal.

The bounded Project assessment contains fit, uncertainty, a reason and an optional
exact native candidate JSON value. The retained result uses the existing common
MCP envelope. Named schema references share its repeated Artifact/pin/claim/
observation shapes, and a compatibility test compares the expanded release schema
with `factory_mcp_conclusion_shape`.

The default reviewed policy selects native `issues/read_issue` entity data at
`/issue` and `issues/search_issues` bounded coverage at `/coverage/has_more`.
These are editable policy literals. The compatible external fixture uses its own
native `lookup` tool and reviewed selectors without a new executor/query parser.
An external contract must be qualified as an authorized entity read and truthful
coverage/truncation source before those selectors may authorize rejection.

The gate requires the exact `native-mcp-evidence` verifier pin, successful native
capture/calls, the declared bounded coverage observation and fresh output. Producer
Build/Attempt/Job and the exclusive freshness deadline come from the accepted
record's owner metadata, not model fields. Generic MCP verification continues to
report unknown overall coverage, `duplicate_proven: false` and
`global_absence_proven: false`. The business policy accepts a bounded assessment;
it does not claim a global absence proof or an independently proven semantic fit.
Required Project goals/rules remain the existing Context/selected-Profile contract.

Clear eligible assessments select `classify`; exact observed duplicates select
`duplicate`; clear out-of-scope assessments select `out_of_scope`. Explicit input
requests select `needs_input`, and unresolved/conflicting evidence defaults to
`review`. Only `classify` has an analyst successor. Other outcomes terminate at
`rejected` or `project_review`. Gate results retain the finite decision, eligibility
flag, bounded reason and evidence references; the analyst receives only the
declared accepted handoff and original Work inputs. No path grants write authority.

## TDD evidence

Baseline before task 5.2: the six task-5.1 template tests and seven existing
Role/MCP-bound-capture tests passed. The following increments were implemented
only after observing their behavioral Red through public result/policy contracts.
All commands below used this environment:

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
```

| Red command | Observed failure | Green |
| --- | --- | --- |
| `cargo test --locked -j2 -p octacity-server-application --lib triage_eligibility_accepts_only` | Base result schema rejects the verified native MCP conclusion with `Flow data contract`. | Versioned result schema and reviewed duplicate rule accept the observed candidate; terminal rejection has no analyst successor. |
| `cargo test --locked -j2 -p octacity-server-application --lib triage_eligibility_routes_clear` | Clear fit selects `review`, expected `classify`. | Add finite eligible/out-of-scope rules and terminal mapping; both cases pass without asserting global absence. |
| `cargo test --locked -j2 -p octacity-server-application --lib triage_eligibility_waits` | Explicit missing input selects `review`, expected `needs_input`. | Add the declared needs-input outcome; input/review/inconclusive cases skip the analyst. |
| `cargo test --locked -j2 -p octacity-server-application --lib triage_eligibility_requires_the_actual` | Gate input rejects the newly required owner producer/freshness contract. | Add owner metadata projections and exact verifier/producer/freshness predicates; substituted or stale evidence follows review. |

Compiler/setup failures were corrected separately and are not Red evidence.
The evaluator correctly rejected an object literal in an early policy draft;
the final program compares the three scalar verifier pin fields using its
existing contract. No evaluator bounds or validators were weakened.

Further tests characterize the existing capture/result safeguards and compose
them with the new policy. They cover forged candidates/records, write tools,
ambient connections, model control fields, oversized reasons, unknown fit,
incomplete/missing/truncated coverage, failed/malformed/cancelled calls and forged
verification assertions. A default native Issue-tool fixture and a separately
reviewed external native-tool fixture both pass. Ten eligibility tests are green.

The committed-record fixture constructs ordinary verified Build provenance and
fenced node completions, then uses `FlowNodeInput::prepare`,
`FlowGateProgram::record` and `FlowInterpreter::progress`. It verifies actual
terminal decisions, the only permitted analyst successor, owner metadata
selection, retained evidence and private Project Context exclusion. It qualifies
the semantic owner boundary; it is not a packaged Agent/model execution pilot.

Task 5.1's publication/admission tests remain green with the expanded handoff.
Its publication fixture now selects the native primary-output verifier instead
of an unused eligibility `facts` report, and the obsolete schema import is removed.

## Regression and quality checks

Performed on 2026-10-29, with the Cargo environment above and two jobs:

| Command | Result |
| --- | --- |
| `cargo test --locked -j2 -p octacity-server-factory --lib` | 309 passed. |
| `cargo test --locked -j2 -p octacity-server-factory --lib triage_` | Final 6 passed after adapting the draft-edit fixture to the new input path while preserving its original provenance checks. |
| `cargo test --locked -j2 -p octacity-server-application --lib factory_role_materialization_tests` | 60 passed, including all 10 new eligibility cases. |
| `cargo test --locked -j2 -p octacity-server-application --lib factory_codex_observation_tests` | 26 passed. |
| `cargo test --locked -j2 -p octacity-server-application --lib factory_codex_output_source_tests` | 26 passed, including ordinary retained native MCP output validation. |
| `cargo test --locked -j2 -p octacity-server-application --lib factory_configuration_cqrs_tests` | 25 passed. |
| `cargo test --locked -j2 -p octacity-server-application --test triage_context` | 5 passed. |
| `cargo clippy --locked -j2 -p octacity-server-factory --all-targets -- -D warnings` | Passed. |
| `cargo clippy --locked -j2 -p octacity-server-application --lib -- -D warnings` | Passed. |
| `cargo clippy --locked -j2 -p octacity-server-application --tests -- -D clippy::all` | Passed; existing unused-helper compiler warning remains. |
| `cargo fmt --all -- --check` | Passed. |
| `python3 tools/check_architecture.py` | Passed: 64 packages, 263 edges, unchanged. |
| `openspec validate add-factory-triage-pipeline --strict` | Passed. |
| `git diff --check` | Passed. |

Release JSON syntax, schema-reference closure and local documentation links also
passed. Changes from the preceding task 5.1 were preserved without a commit.

Application test builds retain the pre-existing unused
`pending_factory_profile_fixture` warning in unchanged test support. No test failed
in the final scoped regression run (451 tests across the six disjoint suites).
The canonical gate program is 6,496 bytes, below the existing 8 KiB ceiling.
All schema aliases resolve; `classification_facts` remains an explicit external
verifier dependency rather than an unused graph import.

The README documents native selectors, authority/freshness requirements,
assessment-versus-fact semantics and external-server qualification. Documentation
edits have no executable behavior requiring their own TDD cycle.

Disk remained at 37 GiB free throughout this task's builds. No cleanup was needed
and no commits were created. Rich analyst/reproduction contracts (5.3–5.4),
management setup/readiness and packaged execution remain separate tasks; this
verification does not declare the complete triage preset operational.
