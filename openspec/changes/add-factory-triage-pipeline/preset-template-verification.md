# Task 5.1: versioned triage template verification

## Scope and contracts

The release manifest at `config/factory/presets/triage/v1.json` composes two
reasoning stages and the existing eligibility/classification gates through the
ordinary `FlowDefinitionTemplate::from_manifest` resolver. Two small stage parts
adapt the original Work inputs and accepted handoff; gate programs and result
schema dependencies are reused from the base Flow. All classification routes
terminate in a triage result or human review. There are no development, research,
write-action, provider-specific executor or preset-registry additions.

Editable Project and Systems analyst Role seeds contain responsibility/profile
instructions only. Complete execution settings and authority remain operator
selections. The only production Rust extension is optional template
`profile_selection`, passed unchanged to the existing immutable node validator.
Omission preserves existing serialization; static and selected bindings cannot
override each other. The ordinary publication and admission boundaries retain
exact Role/Profile, schema, Context, gate, policy and budget contracts.

Tests use explicit in-memory installed-capability fixtures to qualify publication.
They do not claim that an executor or facts verifier is installed or ready.
Eligibility linked to retained MCP evidence, richer classifications, reproduction,
management resolution/readiness and packaged execution remain tasks 5.2–5.4,
7.1–7.2 and 8.1 respectively.

## Test-first increments

Before implementation, existing `flow_template` tests passed (7), and the
application's `triage_context` integration tests passed (5). Each increment below
was run through the core factory test boundary before its implementation change.

| Increment | Observed Red | Green implementation and outcome |
| --- | --- | --- |
| Versioned manifest | Missing bundled triage manifest at the correct release root | Add manifest/routing using existing schema/gate parts; exactly two reasoning stages and two gates instantiate, with terminal recommendations. |
| Exact Role/Profile selections | Deserialization rejects unknown `profile_selection` field | Add the optional template field and delegate to the existing node validator; each stage retains its own exact selection. |
| Editable responsibility seeds | Missing versioned Role seed files | Add independent Project/Systems analyst seeds; normal instantiation copies instructions and later seed edits cannot mutate copied Roles. Seeds contain no execution defaults. |
| Original ticket and completion | Base eligibility input rejects the declared task/completion/Work payload | Add explicit Project/analyst input schemas and stage bindings; both original Artifact references remain distinct, with a separately declared accepted gate handoff. |
| Normal publication/admission | `InvalidConfiguration { field: "Flow entry input schema" }` | Bind the manifest entry to `project_input`; ordinary publication and admission succeed and retain their closure after schema/gate edits and replacement. |

An initial test fixture used the wrong release-directory root; that setup failure
was corrected and Red rerun at the correct root. Compiler errors and invalid
fixture gate edits were corrected separately and are not counted as behavioral
Red. The final tests also characterize existing safeguards: missing analyst
handoff/required Context, withdrawn Roles, excessive selection budget, simultaneous
bindings and renamed stages. They required no new behavior in those validators.

The executable test cases are in
`server/core/octacity-server-factory/src/triage_template_tests.rs`. The scoped
command was used throughout the increments:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -j2 -p octacity-server-factory --lib triage_
```

## Regression and repository checks

Checks performed on 2026-10-29, with debug information/incremental compilation
disabled and at most two Cargo jobs to limit disk growth:

| Command (same Cargo environment as above) | Result |
| --- | --- |
| `cargo test --locked -j2 -p octacity-server-factory --lib` | 309 passed. |
| `cargo test --locked -j2 -p octacity-server-factory --lib triage_` | 6 passed after the final assertions and removal of an unused base input-schema import. |
| `cargo test --locked -j2 -p octacity-server-application --lib factory_configuration_cqrs_tests` | 25 passed, including selection and standalone Flow publication. |
| `cargo test --locked -j2 -p octacity-server-application --lib factory_role_materialization_tests` | 50 passed. |
| `cargo test --locked -j2 -p octacity-server-application --test triage_context` | 5 passed. |
| `cargo clippy --locked -j2 -p octacity-server-factory --all-targets -- -D warnings` | Passed. |
| `cargo fmt --all -- --check` | Passed. |
| `python3 tools/check_architecture.py` | Passed: 64 packages, 263 dependency edges, unchanged. |
| `openspec validate add-factory-triage-pipeline --strict` | Passed. |
| `git diff --check` | Passed. |

Application test builds report the existing unused test helper
`pending_factory_profile_fixture` in unchanged `factory_test_support.rs`.
No test failed. Review of the diff found no additional dispatcher, duplicated
gate/business logic or execution defaults; the release's unused base
`classification_input` import was removed with all six characterization tests
remaining green. The `facts` schemas remain explicit verifier dependencies.

Documentation changes explain the loader root, editable bounds, independent
selections/Context, fact-verifier prerequisite, snapshot/version behavior and
remaining operational qualification. They have no executable behavior requiring
a separate Red/Green cycle.

Disk availability was checked before and during builds: initially 38 GiB,
subsequently 37 GiB. No cleanup or deletion was needed. No commits were created.
