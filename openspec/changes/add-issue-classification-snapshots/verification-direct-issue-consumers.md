# Task 10.4: direct, producer-independent Issue consumers

## Contract and scope

Consumers select stable core/property identities under their own aliases. Snapshot
selection no longer consults fixed classification roles or familiar verdict names.
The shared applicability check retains requirements, exact code, material and
assessment-definition fences. Typed admission mappings accept any required selected
Project choice; they retain explicit `WorkPriority` / `RiskClass` conversions and
allowlists. They accept neither arbitrary prose nor invented defaults.

An explicit `FlowIssueEvidenceSelection.observation` pins environment, criterion and
expected command finding. `IssueSnapshotObservations` reuses the existing accepted
command-result reader, independent Artifact authority and trusted clock. The snapshot
retains exact qualified evidence references and projects only record digests. Required
missing/unqualified proof blocks preparation; optional unavailable proof is omitted.
Final manual/Source admission rechecks original references, freshness and exact
resolved subject. Admitted replay keeps the original data without requalification.
The common command finding type lives with the Factory contract and is reexported
by its existing application API; no second observation parser is introduced.

One shared `issue_choice_threshold` compiles the reviewed active order of Size or
any custom choice field into existing concrete predicates. The consumer supplies
its input path/outcomes. The existing Size template binder delegates to this compiler;
execution never looks up mutable choice order or special field/node names.

Existing command reports and finite Decision Signal projection/routing remain
ordinary inputs around an automatic selected-field step. No provider adapter,
live-provider qualification, source/target mapper or rules framework is added.
Template/catalog installation and REST/Console authoring remain tasks 10.5–10.8.
Observation-dependent hosts install the same existing evidence reader in preparation
and admission; absence fails closed. This task does not claim UI migration complete.

## Observed Red/Green

Tests use authorized authoring/review/request/publication boundaries, ordinary
accepted command artifacts, the common admission validator and the normal gate
interpreter. Controlled command bytes replace external transport, not acceptance
or evidence qualification.

- `issue_snapshot_admission_uses_direct_project_choice_fields_without_fixed_roles`:
  Red rejected a required ordinary Project choice at `/issue_snapshot/admission`;
  Green publishes independent priority/risk/allowlist rules with no role registration.
- `direct_property_snapshot_requires_current_assessment_without_semantic_registration`:
  Red accepted a stale ordinary property after requirements changed; Green returns
  field-specific inapplicability before reserving execution or mutating Issue data.
- `direct_snapshot_identity_survives_role_reassignment_without_claiming_observation_proof`:
  Red rejected a stable property ID because optional catalog roles changed; Green
  consumes its scalar as data and creates no observation/result authority. The
  obsolete role-reinterpretation expectation was removed, not preserved by an adapter.
- `direct_choice_thresholds_freeze_size_and_unrelated_project_field_order`:
  Red lacked the generic threshold API. Green routes both sides of Size and unrelated
  custom-choice thresholds through ordinary gates; serialization and later catalog
  reordering preserve the original predicate. New compilation sees the new order.
  Unknown/retired thresholds and non-choice fields are rejected. Existing Size route
  behavior was checked before its binder was refactored to use the shared compiler.
- `direct_issue_snapshot_requires_current_trusted_observation_without_fixed_roles`:
  initial Red lacked explicit observation composition; runtime Red rejected even a
  valid independently retained observation. Green retains the real qualified record
  without a classification role. Negative cases cover denied Artifact access,
  missing records, wrong environment/criterion/finding, wrong subject and expiry.
  A further Red accepted a different exact subject; Green fences it during selection
  and again against resolved admission code. Another Red accepted expiry between
  review and admission; Green rejects it at the common final validator.
- The same public request/publication journey observed Red when an Artifact grant
  was revoked during input-byte publication: Work was incorrectly admitted. Green
  rechecks authority immediately before admission. Recovery after restoring the
  grant uses the original frozen references; replay after admission keeps the same
  Work/snapshot even when that grant is subsequently revoked.

## Commands and environment

All Cargo commands use bounded artifacts:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
```

Focused Red/Green commands:

```sh
cargo test --locked -p octacity-server-application --lib issue_snapshot_ -- --nocapture
cargo test --locked -p octacity-server-application --lib direct_property_snapshot_requires_current_assessment -- --nocapture
cargo test --locked -p octacity-server-application --lib direct_snapshot_identity_survives_role_reassignment -- --nocapture
cargo test --locked -p octacity-server-application --lib direct_choice_thresholds -- --nocapture
cargo test --locked -p octacity-server-application --lib direct_issue_snapshot_requires_current_trusted_observation -- --nocapture
```

Regression results (all passed):

| Command after the exports above | Result |
| --- | --- |
| `cargo test --locked -p octacity-server-application --lib snapshot -- --nocapture` | 63 passed |
| `cargo test --locked -p octacity-server-application --lib factory::authoring::tests::factory_configuration_cqrs_tests -- --nocapture` | 74 passed, including the new direct-choice threshold case |
| `cargo test --locked -p octacity-server-application --lib factory::commands::tests::factory_command -- --nocapture` | 55 passed |
| `cargo test --locked -p octacity-server-application --lib factory::authoring::factory_presets -- --nocapture` | 23 passed, including existing Size routing |
| `cargo test --locked -p octacity-server-application --lib factory_decision_signal_tests -- --nocapture` | 31 passed |
| `cargo test --locked -p octacity-server-store --lib issue_classification -- --nocapture` | 26 passed; this same group passed before extracting shared applicability |
| `cargo test --locked -p octacity-server-factory --lib` | 353 passed |
| `cargo test --locked -p octacity-server-application --lib direct_choice_thresholds -- --nocapture` | 1 passed |
| `cargo test --locked -p octacity-server-application --lib direct_issue_snapshot_requires_current_trusted_observation -- --nocapture` | 1 passed after the final late-revocation Red/Green extension |
| `cargo clippy --locked -p octacity-server-factory -p octacity-server-store -p octacity-server-application --lib -- -D warnings` | passed |
| `cargo fmt --all --check` | passed |
| `git diff --check` | passed |
| `python3 tools/check_architecture.py` | passed: 64 packages / 264 dependency edges |
| `openspec validate add-issue-classification-snapshots --strict` | passed |

Database regressions used only the task-labelled disposable PostgreSQL 18.6 service,
not a user's stand. The harness creates and removes its databases:

```sh
export OCTACITY_POSTGRES_URL=postgres://octacity:octacity-test@127.0.0.1:15439/postgres
cargo test --locked -p octacity-server --test project_text_intake snapshot -- --ignored --nocapture
cargo test --locked -p octacity-server --lib issue_snapshot -- --ignored --nocapture
```

Results: 17 publication/admission/restart/integrity tests and 18 Source snapshot
readiness, authorization, refresh, replay and material tests passed. The ordinary
non-ignored Source filter also passed its independent error-normalization case.
The disposable service was stopped after verification.

These are focused checks, not a claim of a full workspace/CI or live-provider run.
The application lib-test build still emits its existing 11 unused test-support
warnings; production library Clippy with denied warnings passed. Documentation and
comment edits change no executable behavior; formatting, link targets and OpenSpec
validation qualify those edits without an artificial behavioral test.

Review corrections qualify the invocation baseline, current admission clock and
consumed material provenance in [the follow-up verification](verification-10.3-10.4-review.md).
