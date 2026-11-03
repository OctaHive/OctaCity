# Review corrections for tasks 5.1–5.2

## Corrected contracts

The P3 representation-coupling finding is resolved by removing
`assessment.duplicate_candidate_json`. Assessment now contains a boolean
`duplicate` proposal. The candidate is retained once in its verified native detail
claim, which the existing MCP verifier compares structurally with the actual
response. The declarative gate selects that claim by its reviewed native
connection/tool/value selector. It no longer compares two JSON-containing strings.
Whitespace and object member order therefore cannot change duplicate acceptance.

The stage result permits at most two claims: one selected candidate and one
required coverage value. Because candidate and coverage selectors are distinct,
successful coverage leaves only one possible detail claim. Additional claims fail
the result contract instead of making candidate selection ambiguous. Ordering is
not part of the gate policy. The shared capture ceiling remains 64 calls, and
multiple searches/detail reads may still be investigated and retained.

The P2 instruction gap is resolved in the default Project Profile seed. Its
instructions now state the assessment fields, native tool/selectors,
`arguments_json`, `value_json`, `json_text: 0`, the coverage requirement and the
two-claim result bound. They distinguish assessment from acceptance and forbid
invented coverage or model-supplied owner metadata. A complete illustrative result
includes a native Issue's identity, revision, content, workflow/status and timestamps.
Operators selecting another native contract must review both policy and Profile
instructions before publication.

No production Rust, verifier specialization, JSON parser, dependency or execution
owner was added. Test fixture options now accept the selected Role definition and
exact result schema/verifier so the compiled-prompt test exercises the actual
release instructions and model-facing contract through the existing compiler.
The normal native Issue capture fixture is shared between its original contract
test and the new prompt/example test.

## Test-first evidence

Baseline: all 10 existing eligibility tests passed before changes. Cargo commands
below used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`
and `-j2` to limit build disk use.

| Command | Observed Red | Green |
| --- | --- | --- |
| `cargo test --locked -j2 -p octacity-server-application --lib triage_eligibility_candidate_formatting` | The old result schema rejects the new assessment-without-a-second-candidate contract (`Flow data contract`). | Compact and pretty/reordered JSON for the same observed entity both select `duplicate`; the candidate is absent from assessment. |
| `cargo test --locked -j2 -p octacity-server-application --lib triage_eligibility_keeps_one_candidate` | A verified third claim is accepted, violating the required unambiguous two-claim result contract. | Tighten the stage schema to two claims; extra candidates are rejected. |
| `cargo test --locked -j2 -p octacity-server-application --lib triage_eligibility_compiled_project_prompt` | The compiled Project invocation does not explain `assessment.duplicate`. | Release Profile instructions reach the actual protected prompt. The generated model schema declares boolean duplicate/two claims and excludes owner verification. The prompt example passes native evidence verification, the retained result schema and the unmodified default gate against independently specified Issue observations. |

All three increments observed behavioral/API Red before their implementation.
The existing native evidence verifier and its shared wire grammar are unchanged;
the release compatibility test permits only the explicit tighter claim bound.

## Final checks

Performed on 2026-10-29 with the same bounded Cargo environment:

| Command | Result |
| --- | --- |
| `cargo test --locked -j2 -p octacity-server-factory --lib` | 309 passed. |
| `cargo test --locked -j2 -p octacity-server-application --lib factory_role_materialization_tests` | 63 passed, including 13 eligibility tests. |
| `cargo test --locked -j2 -p octacity-server-application --lib factory_codex_observation_tests` | 26 passed. |
| `cargo test --locked -j2 -p octacity-server-application --lib factory_codex_output_source_tests` | 26 passed. |
| `cargo test --locked -j2 -p octacity-server-application --lib factory_configuration_cqrs_tests` | 25 passed. |
| `cargo test --locked -j2 -p octacity-server-application --test triage_context` | 5 passed. |
| `cargo clippy --locked -j2 -p octacity-server-factory --all-targets -- -D warnings` | Passed. |
| `cargo clippy --locked -j2 -p octacity-server-application --tests -- -D clippy::all` | Passed; existing unused `pending_factory_profile_fixture` compiler warning remains. |
| `cargo fmt --all -- --check` | Passed. |
| `python3 tools/check_architecture.py` | Passed: 64 packages, 263 dependency edges. |
| `openspec validate add-factory-triage-pipeline --strict` | Passed. |
| `git diff --check` | Passed. |

Release JSON syntax and local documentation links also passed validation.

The six disjoint test suites total 454 passing tests. The gate program remains
bounded at 6,626 bytes under the existing 8 KiB ceiling. Documentation updates
describe the current claim selection and executable instructions; they require
documentation validation rather than a separate behavioral TDD cycle.

Free disk space was 37 GiB before/during builds and 38 GiB at the final check.
No cleanup or commits were performed. Prior uncommitted task-5.1/5.2 work is preserved. Scope remains
those two review corrections; later triage tasks and packaged execution are not
claimed complete.
