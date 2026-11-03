# Reproduction command verification (task 5.4)

## Delivered boundary

The ordinary installed command profile optionally freezes a bounded observation
contract. Its canonical report schema, coverage/symptom predicates and exact
subject/input/environment/criterion are verified by the existing command compiler,
runtime profile parser and output owner. No additional executor, journal, native
shell parser, model adapter or provider client was introduced.

The optional released template places one ordinary command before the analyst,
with exact Context and one explicit technical retry. Both templates retain two
reasoning stages. The default supports feature N/A and unresolved defects without
an invented independent classification producer. The optional template requires
its report schema and safe immutable selected binding to be filled through normal
draft authoring; management resolution remains task 7.1.

Safe owner metadata is projected into the frozen analyst input. The gate compares
selected command policy and original Work subject/command input scope, checks both
freshness deadlines, and retains assessment/evidence separately from original
scheduling metadata. It does not accept model-authored verification metadata.
Technical execution stops stay stops with usage and existing operator recovery;
they do not become negative reproduction evidence or configured effects.

## TDD observations

Commands below use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0`, `--locked -j2`. Each filter was run before its corresponding
implementation and again afterward. Test-setup/API spelling errors were corrected
before observing a behavioral Red; those errors are not counted as evidence.

| Public seam / filter                                                                     | Observed Red                                                 | Observed Green                                                                          |
| ---------------------------------------------------------------------------------------- | ------------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| application `command_observation_requires_a_complete_applicable_symptom_check`           | Required observation API absent                              | Positive/negative and substituted subject/input/command/criterion/coverage checks pass  |
| application `unavailable_command_conditions`                                             | Unavailable report without observations rejected             | Indeterminate unavailable/input accepted; false verdicts rejected                       |
| application `installed_observation_contract`                                             | Required frozen profile option absent                        | Contract changes task identity; omitted contract retains legacy identity                |
| application `an_unbounded_observation_policy`                                            | Oversized predicate accepted for dispatch                    | Rejected before command I/O                                                             |
| application `command_observation_rejects_candidate_checkout`                             | Candidate Context accepted as base evidence                  | Exact Work subject required                                                             |
| application `command_report_schema`                                                      | Required canonical report-schema API absent                  | Closed fields, selected observation shape and common depth/byte bounds pass             |
| application `observation_command_refuses_an_incompatible_output_schema`                  | Compiler accepted incompatible output                        | Exact selected primary Report required                                                  |
| application `observation_bindings_expose_only_frozen_report_metadata`                    | Required safe choice API absent                              | Exact safe metadata exposed; legacy plain command advertises none                       |
| application `configured_command_observation_cannot_accept_a_schema_valid_unbound_report` | Schema-valid report accepted despite observation contract    | Bound canonical report verification rejects forged input                                |
| application `command_success_with_terminal_failure_facts`                                | Successful Job with timeout/failure facts accepted           | Contradictory terminal facts rejected                                                   |
| app `command_file_freezes_optional_observation_criteria_with_the_native_task`            | Native file rejected new contract field                      | Preserved through native conversion; invalid policy/schema rejected early               |
| core `triage_without_command`                                                            | Released schema rejected truthful absent proof               | Feature N/A and unresolved defect/review outcomes pass                                  |
| core `triage_accepts_only_applicable_completed_command_findings`                         | Released schema rejected command provenance                  | Positive/negative accepted; policy/scope/time substitution routes to review             |
| core `triage_projects_absent_command`                                                    | Analyst input omitted explicit absent observation            | Default mapping and independent assessment gate pass without synthetic facts            |
| core `triage_analyst_consumes_only_the_safe_owner_projection`                            | Projection returned absent despite verified command metadata | Frozen safe header retained; raw probe output omitted                                   |
| core `triage_analyst_gate_retains_results`                                               | Summary omitted command evidence                             | Ordinary record/restore retains exact evidence and original Work metadata               |
| application `triage_optional_command`                                                    | Required optional template absent                            | Ordinary publication binds five nodes, two AI profiles and exact command schema/profile |
| application `triage_analyst_compiled_prompt`                                             | Protected prompt omitted observation contract                | Executed prompt explains finding/status/criterion relevance and uncertainty             |

Existing generic failure/retry behavior was characterized before relying on it:
`failed_reproduction_commands_remain_technical_stops_even_when_a_report_was_published`
passes for failed/timed-out executions while retaining actual usage. Qualified
positive/negative/unavailable reports also pass through ordinary Artifact
publication and retain actual Build/Attempt/Job/tool/task/schema identities.

## Review and qualification

The Standards and Spec reviewers independently reviewed the complete tracked and
untracked task diff against `b469ef4`; both reported no actionable findings.
Earlier review suggestions (shared predicate precondition, early validation,
truthful environment semantics and command-before-analyst placement) are included.
The full regression exposed one stale public-read fixture missing the new evidence
field; it was updated and the complete suite rerun successfully. Clippy found a
single-element test loop left after removing duplicate assessment verification;
it was simplified without changing behavior and the checks passed.

Final commands and observed results, with the Cargo environment/flags above:

- `cargo test -p octacity-server-factory -p octacity-server-application --lib --quiet`:
  **318 Factory + 845 Application passed**. Application includes the existing
  long-running requirements/release journeys (367.74 seconds); none failed.
- `cargo test -p octacity-server-application --lib command_ --quiet`:
  **48 passed** (also covered by the complete Application run).
- `cargo test -p octacity-server --lib command_file_freezes_optional_observation_criteria_with_the_native_task`:
  **1 passed** through the actual native profile-file conversion.
- `cargo clippy -p octacity-server-factory --all-targets -- -D warnings`:
  passed.
- `cargo clippy -p octacity-server-application --tests -- -D clippy::all`:
  passed. The pre-existing unused `pending_factory_profile_fixture` compiler
  warning remains; it is unrelated to this change.
- `cargo clippy -p octacity-server --tests -- -D clippy::all`: passed.
- `python3 tools/check_architecture.py`: **64 packages / 263 edges**, passed.
- `cargo fmt --all -- --check`, scoped `pnpm exec prettier --check`,
  `openspec validate add-factory-triage-pipeline --strict`, and
  `git diff --check`: passed.

The native test link emitted the existing macOS `__eh_frame` compact-unwind
size warning; compilation and the runtime test succeeded. No compiler/linker
warning was treated as a behavioral Red.

Free disk was **36 GiB before and 35 GiB after task 5.4** (`df -h .`). Builds used
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0` and
`-j2`; no cleanup of user data was needed.

These tests qualify semantic/owner boundaries, not an installed real reproduction
environment. Actual packaged qualified-Agent execution and probe/environment
qualification remain task 8.1; no paid model or live GitHub write was performed.
The user requested stopping after this task; section 6 and later tasks remain
unchecked.

## Review follow-up: unavailable reproduction and shared dependencies

The 5.3–5.4 review found that a completed wrapper reporting an unavailable
environment could reach accepted rejection when the analyst recommended it.
The released gate now sends defects with `environment_unavailable` or `needs_input`
command status to human review before considering model route recommendations.
Feature N/A and completed applicable command reports preserve their route policy.

TDD at the existing public `FlowGateProgram::select_at` boundary:
`cargo test --locked -j2 -p octacity-server-factory --lib triage_unavailable_defect_reproduction`
first failed with `rejection` instead of `escalation` for
`environment_unavailable/unknown/rejection`. After the declarative guard was added,
the same test passed. It covers both unavailable/input statuses, all seven route
recommendations, three unresolved defect assessments and the declared human-review
terminal, with feature N/A and completed-command controls.

The duplicated dependency-item shape was then extracted to the existing named
schema-reference mechanism. This is a behavior-preserving refactor, so it uses
Green-before/Green-after characterization rather than a fabricated Red. Before
extraction, 17 triage tests passed, including a new test that checks both dependency
scopes at 0/1/64/65 items, required members, closed fields and field-size bounds.
After extraction they still pass. A temporary probe calling the public
`FlowDefinitionTemplate::data_schemas` captured all 23 existing expanded schema
digests before extraction and verified exact agreement afterward; only the new
shared alias was added. Published/admitted schema identities remain unaffected.

Cargo verification uses `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0` and `--locked -j2`. Final scoped checks are recorded below.
These review fixes were initially verified without commits; the user subsequently
authorized committing them.

Final follow-up verification:

- `cargo test --locked -j2 -p octacity-server-factory --lib --quiet`:
  **320 passed**.
- `cargo test --locked -j2 -p octacity-server-application --lib triage_ --quiet`:
  **20 passed**.
- `cargo test --locked -j2 -p octacity-server-application --lib retained_public_results_read_classification_without_mutation_or_private_context --quiet`:
  **1 passed**, preserving authorized public result reads without mutation.
- `cargo clippy --locked -j2 -p octacity-server-factory --all-targets -- -D warnings`:
  passed. Application emitted only its pre-existing unused fixture warning.
- Architecture check: **64 packages / 263 edges**, passed. Rust formatting,
  scoped Prettier, strict OpenSpec validation and `git diff --check` passed.

Disk monitoring observed **56 GiB initially, 54–56 GiB during checks, and
55 GiB on the final reading**. Debug symbols and incremental compilation remained disabled, with
`-j2`; no user data was deleted. The fixes change only the released template,
characterization/regression tests and documentation. Commit creation was
authorized separately after this verification.
