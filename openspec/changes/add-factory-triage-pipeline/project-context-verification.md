# Task 4.1: reviewed Project context compilation

## Delivered boundary

`TriageProjectContextDraft` compiles operator-reviewed immutable goals, rules and
supporting document references into the existing `FlowContextBinding`. It does
not resolve current documents, read bytes, create another context journal or
grant authority. Goals/rules are required; supporting documents use closed
`document.<alias>` names. The ordinary binding and Context Manifest retain their
existing count, encoded-size and aggregate-byte bounds.

The companion `ticket_binding()` loads the versioned ordinary input projection
from `config/factory/presets/triage/ticket-input-v1.json`. It selects only the
admitted Work's original task and completion references. The independent code
Repository/revision remains the immutable Work/node subject; native ticket text,
Role names and predecessor/model results cannot replace it.

The selected Profile uses the existing named `context_requirements`. The common
role package preserves exact instructions, inputs and provenance. Project rules
remain controlling policy; goals and supporting documents remain data. Names do
not dispatch behavior or add tool/credential permissions.

Missing goals/rules and a foreign draft Project return distinct actionable draft
problems. Document ownership, publication, digest, byte length and retention stay
with the existing readers. Context preparation/consumption now preserves a failed
selection's logical name and safe original `ApplicationFailure` class in
`ContextInputUnavailable`. No backend diagnostic or document body is retained in
that error. Failed reads do not produce a usable Context or fabricated assessment.

These are compilation and retained-input contracts for the later preset resolver
and stage setup. They do not deliver the HTTP readiness query (7.2), duplicate
lookup (4.2), isolated two-stage Agent execution (4.3), complete graph (5.1) or
Console setup (7.4). No live model, provider write or paid request was used.

## TDD evidence

All Cargo commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, `--locked` and `-j2`.
The existing role materialization suite passed **33 tests** before its fixture
was extended to accept an arbitrary Role display name.

| Public boundary / command | Observed Red and Green |
| --- | --- |
| `cargo test --locked -p octacity-server-application --test triage_context missing_project_goals -j2` | Red: the newly required public draft/problem API did not exist (`E0432`); Green: absent goals return `MissingProjectGoals` before context compilation. This was a missing API, not an environment failure. |
| `cargo test --locked -p octacity-server-application --test triage_context reviewed_project_context_freezes -j2` | Red: selection returned one entry instead of the specified five; Green: exact task, completion, goals, rules and supporting document identities/categories are selected, and a draft replacement cannot alter the retained binding. |
| `cargo test --locked -p octacity-server-application --test triage_context unresolved_project_context -j2` | Red: foreign context returned a generic bounds/selection problem; Green: foreign Project and missing rules have specific remediation. |
| `cargo test --locked -p octacity-server-application --test triage_context triage_ticket_projection -j2` | Red: the ordinary ticket projection API was absent (`E0599`); Green: real `FlowNodeInput::prepare` selects the original admitted references and separate exact code subject, with no model/predecessor result sources. |
| `cargo test --locked -p octacity-server-application --test factory_artifact_reader triage_goal_context -j2` | Red: foreign goals returned only “application input is invalid”; Green: foreign/changed-digest goals name `project-goals`, preserve `Invalid` and cause zero protected-byte reads. Each case owns a fresh publication fixture. |
| `cargo test --locked -p octacity-server-application --test factory_artifact_reader configured_context -j2` | Existing positive baseline passed. Red: altered declared context lost the input name during public `FactoryArtifactFlowInputs::prepare`; Green: failed preparation identifies the selection without exposing bytes, and the original deterministic manifest/read behavior still passes. |

The ticket fixture initially retained the old entry schema while changing its
node schema. Its legitimate `Flow entry input schema` rejection was corrected in
the test setup and is not counted as behavioral Red. A profile characterization
test initially expected the wrong prompt-section heading; that assertion was
corrected to the compiler's existing JSON-data heading without changing behavior.

Additional characterization verifies that missing backend bytes retain
`Unavailable` rather than becoming `Invalid` or an empty successful Context.
It also verifies that `Project`, `Systems analyst`, its
Russian display name and an arbitrary custom name use the same compiler and
permissions. Frozen Project rules precede additions, injected document text stays
in the data section, the emitted package retains exact content, and changed goals
bytes cannot compile. Oversized document choices fail without truncation, document
aliases cannot replace the ticket, and authority fields are rejected on the draft.

## Verification

| Command | Result |
| --- | --- |
| `cargo test --locked -p octacity-server-application --test factory_artifact_reader --test triage_context -j2` | 6 reader and 5 triage compilation tests passed after the final fixture changes. |
| `cargo test --locked -p octacity-server-application --lib factory_role_materialization_tests -j2` | 34 passed, including all four triage Role display-name cases and changed-goals rejection. |
| `cargo test --locked -p octacity-server-application --lib -j2 -- --test-threads=2` | 785 passed, no failures/ignored tests; the complete library regression finished in 926.87 seconds. |
| `cargo clippy --locked -p octacity-server-application -p octacity-server --lib -j2 -- -D warnings` | Passed; downstream server/REST consumers compile with the new safe error variant. |
| `cargo clippy --locked -p octacity-server-application --lib --tests -j2` | Passed; only the pre-existing unused `pending_factory_profile_fixture` warning was reported. |
| `python3 tools/check_architecture.py` | Passed: 63 packages, 261 dependency edges. |
| `cargo fmt --all -- --check`, `openspec validate add-factory-triage-pipeline --strict`, `git diff --check` | Passed. |

The complete application-library result was observed separately from the scoped
suites. The longer release/requirements-journey cases spend CPU in ordinary
persisted-history/Flow integrity validation, confirmed by a local process sample;
they all passed and were not shortened, ignored or changed for this task.

Free disk space remained about 16 GiB, above the 10 GB cleanup threshold. No cache
or stand data deletion was needed. No new containers, migrations, credentials or
dependencies were introduced, and no commit was created.
Local outputs use `/tmp/octacity-triage-4-1-*.log`.
