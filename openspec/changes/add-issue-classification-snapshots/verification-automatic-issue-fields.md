# Task 10.3: automatic guarded application of ordinary agent forms

## Implementation

An ordinary successful reasoning outcome opts in with `issue_fields` containing
one Project-derived form and an exact installed writer/policy. Publication checks
the generated result shape, Project/catalog pin, outcome kind, installed capability
and node permission. There is no operator-authored apply node, source/target map,
new import endpoint or model-accessible write tool.

The Flow contract owns the selection DTOs. Project catalog resolution and scalar
validation remain in the existing store contract through `IssueFieldFormContract`
and `IssueFieldTargetContract`; the enum/schema is not duplicated across crates.

The common read-only input owner captures the authoritative Work–Issue association
and current Issue revision before Context acquisition and before the ordinary Build
starts, including ordinary producers without field-writing outcomes. The baseline
participates in the frozen input digest, restoration and retries. The node owner rejects an
opted-in invocation without that baseline. Accepted ordinary Build output and its
pending local effect are committed together through the common fenced journal.

The same installed `FactoryIssueClassificationAction` supplies management authority
to the automatic path and the explicit action adapter. The existing outbox supervises
deferred writes using Factory worker claim/batch settings. It retains record digests,
not private result copies. The guarded Issue command validates current definitions,
source ownership, selected-field conflicts and original requirements applicability.
Its original receipt remains the replay authority. A separate outbox claim fences
fresh deferred writes without requiring the completed model attempt to remain live.
No additional persistence owner, migration or scheduling loop was added.

Field origins retain the actual Run, node, node attempt, accepted record, reviewed
form and invocation baseline. Retained metadata rejects an invocation baseline that
differs from the assessed revision. `pending`/`claimed`, `delivered`, `conflict` and
`forbidden` application states remain separate from successful analysis.

The common runtime installs input capture independently of writers. The normal
file-profile installer separately connects outbox delivery to the trusted writer;
the dispatcher has no input-preparation responsibility. REST/client/Console selector
work and template migration remain in tasks 10.4–10.8; no UI completion is claimed by this task.

## TDD evidence

Public boundaries: ordinary outcome serialization, frozen input restoration,
authorized Flow validation, the common node owner with ordinary Build output,
normal profile installation and the real PostgreSQL Issue/history/receipt/outbox
ports. Controlled Build outputs replace the model transport, not result acceptance.

Observed Red/Green:

- `cargo test -p octacity-server-factory --lib ordinary_outcome_retains_selected_issue_form`:
  Red rejected the new outcome option as an unknown field; Green retains the exact
  form/writer selection in the immutable definition.
- `cargo test -p octacity-server-factory --lib issue_field_baseline_is_frozen`:
  Red lacked the required frozen-baseline API; Green checks digest binding,
  immutable restoration and preservation across technical retries.
- `cargo test -p octacity-server-application --lib selected_issue_form_cannot_publish`:
  Red accepted an uninstalled writer; Green rejects it without granting effects.
  A separate authorized authoring case publishes a single ordinary agent with the
  generated Project form and rejects substituting another form's field selection.
- `cargo test -p octacity-server-application --lib selected_form_queues_one_guarded`:
  Red accepted analysis with zero correlated pending writes; Green commits one
  correlated effect and resumes the retained result without calling an executor.
  A second Red allowed a Build intent without an Issue baseline; Green rejects it
  before creating any attempt or Build intent.
- `cargo test -p octacity-server-store --lib selected_issue_field_form_attributes_the_actual`:
  Red lacked actual invocation attribution. A further behavioral Red accepted
  inconsistent retained baseline attribution; Green retains node/attempt/baseline
  and rejects that inconsistency.
- `cargo test -p octacity-server --lib file_issue_classification_installs_an_explicit_local_writer`:
  Red installed zero automatic dispatchers; Green installs the ordinary writer's
  deferred save path without dependency I/O during installation. Input capture is
  now installed separately by the common runtime, including configurations without writers.
- `cargo test -p octacity-server --test factory_configured_flow ordinary_form_ -- --ignored`:
  Red lacked the automatic integration API; Green passes the real-store cases below.

PostgreSQL cases:

1. A manual priority edit before analysis becomes its invocation baseline despite
   an older Work association. Analysis remains accepted while the save is pending;
   delivery applies only the accepted form and retains exact record/node/attempt
   attribution.
2. A concurrent manual edit or revoked management permission leaves Issue values
   unchanged, preserves accepted analysis, and records conflict/forbidden separately.
3. A writer commits and loses its acknowledgement. After a human correction,
   catalog retirement and worker/store restart, claim recovery replays the original
   receipt without overwriting the correction or creating another model Build.
   Re-preparation also retains the original frozen input after those edits.

The PostgreSQL service is disposable, task-labelled and separate from the user's
local stand. The test harness creates and drops its own databases.

## Environment and verification

An initial full-server build exhausted disk space. `cargo clean` removed 30.9 GiB
of Cargo artifacts; the test environment was restored before proceeding. Subsequent
checks use bounded build artifacts without incremental caches/debug symbols:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export OCTACITY_POSTGRES_URL=postgres://octacity:octacity-test@127.0.0.1:15439/postgres
```

The broad application run found one stale discovery assertion left from the
fixed-slot representation: it expected `classification.properties.severity` and a
boolean `component`. The fixture now checks the current Project/form version and
selected stable field identities. This changes no production behavior and adds no
old-format reader.

Final verification:

| Command | Result |
| --- | --- |
| `cargo test -p octacity-server-factory --lib` | 353 passed |
| `cargo test -p octacity-server-store --lib issue_classification_tests` | 26 passed |
| `cargo test -p octacity-server-application --lib issue_fields` | 2 passed: ordinary authoring and accepted-result/pending/replay owner |
| `cargo test -p octacity-server-application --lib` | 1,164 passed; the one obsolete discovery assertion described above failed |
| `cargo test -p octacity-server-application --lib factory::authoring::tests::factory_configuration_cqrs_tests` | 72 passed after updating that fixture, including the failed discovery case |
| `cargo test -p octacity-server --lib file_issue_classification_installs_an_explicit_local_writer` | 1 passed |
| `cargo test -p octacity-server-store-postgres --test issue_execution -- --ignored --test-threads=4` | 36 passed |
| `cargo test -p octacity-server --test factory_configured_flow ordinary_form_ -- --ignored` | 3 passed, including save after Run completion, conflict/denial and lost-ack recovery |
| `cargo clippy --locked -p octacity-server -p octacity-server-application -p octacity-server-factory -p octacity-server-store -p octacity-server-store-postgres --lib -- -D warnings` | passed |
| `cargo clippy --locked -p octacity-server-store-postgres --test issue_execution --test factory_discovery -- -D warnings` | passed |
| `cargo clippy --locked -p octacity-server --test factory_configured_flow --test project_text_intake -- -D warnings` | passed |
| `openspec validate add-issue-classification-snapshots --strict` | passed; task 10.3 complete, 57/64 overall |
| `python3 tools/check_architecture.py` | passed: 64 packages, 264 edges |
| `cargo fmt --all --check` and `git diff --check` | passed |

The nine existing selected-form behavioral tests passed before relocating the DTOs.
The type relocation and the obsolete-fixture correction preserve behavior; the
executable changes above have observed Red/Green. Documentation-only edits are
validated by the repository/OpenSpec checks and have no executable TDD boundary.

Rust test builds on macOS emitted the existing large unwind-table linker warning;
application test compilation also reported existing unused shared test helpers.
The production-library Clippy run passed with warnings denied.


The task-labelled disposable PostgreSQL container was stopped and removed after
verification. The user's ordinary local stand was not modified. No commit was created.

Review corrections qualify the invocation baseline, current admission clock and
consumed material provenance in [the follow-up verification](verification-10.3-10.4-review.md).
