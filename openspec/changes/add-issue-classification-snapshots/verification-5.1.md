# Task 5.1: Project-backed triage application

Review corrections and subsequent qualification are recorded in
[the 5.1–5.2 review report](verification-5.1-5.2-review.md). That report supersedes
the preview and legacy-label availability behavior described below.

## Behavior

Authorized authoring capabilities expose one optional `field_contract` containing
an exact Project definition selection and a content-addressed Flow data schema.
The schema reuses the existing Issue snapshot field compiler, including active
Project priority/component/type choices, fixed size choices and active semantic
classification properties. It introduces no second vocabulary store. Unpublished
catalogs or contracts exceeding existing Flow schema limits omit this capability.

The public authoring/execution test uses that same schema reference for the
accepted triage outcome and local action input, then publishes through the
authorized Flow command. The native node owner executes the accepted producer
and explicit `apply_issue_classification` action. It applies priority, component,
type, size and unresolved reproducibility with exact accepted-record provenance.
Completing triage alone leaves the Issue unchanged. Applying classification
preserves the admitted Work, creates no labels or provider effect, and leaves
the Project with exactly one Run. Delivery is never activated implicitly.

Typed action parameters may omit `size_labels`. Publication checks the Project
catalog pin, selected built-ins/semantic bindings, schema compatibility and exact
record projection, while retaining existing receipt, permission and byte checks.
Tests reject stale catalogs, unbound properties, foreign choices, substituted
values and missing explicit field selection. Numeric-priority/label publication
and execution retain their previous contracts; immutable examples are unchanged.

The PostgreSQL writer now accepts an authoritative native Source association
without a manual execution request. It still requires the exact retained
association, a real requirements revision, applicability/concurrency checks and
the existing runtime authority fences. A public Source-admission/store test
qualifies this path. Historical associations without a requirements baseline
remain ineligible. No database migration, provider write or new authority is
introduced. Reproduction choice values do not establish independent evidence.

The additive OpenAPI contract documents the optional field contract; locally
generated client declarations and TypeScript checks consume it. Console controls
for selecting these typed fields remain task 6.4. API authoring is documented in
[the operating guide](../../../docs/operations/factory-pipeline-authoring.md#apply-triage-classification-to-the-issue).

## Observed Red/Green

All Cargo commands used `CARGO_INCREMENTAL=0`, `--offline` and `--locked`.
Native tests used owned disposable PostgreSQL through `OCTACITY_POSTGRES_URL`.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server \
  --test project_text_intake \
  classification_authoring_offers_project_field_contract_without_size_labels \
  -- --ignored --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server \
  --test project_text_intake published_triage_explicitly \
  -- --ignored --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-store-postgres --test factory_discovery \
  native_source_admission_retains_the_matching_linked_issue_revision \
  -- --ignored --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers \
  classification_capabilities_document_the_shared_project_contract \
  -- --nocapture
```

Each reached an independent behavioral Red before its implementation:

- Authoring rejected the installed capability with an empty legacy label map;
  after the additive contract/metadata change it passed (1 test).
- Public Flow publication rejected typed action parameters without `size_labels`
  at `/action/parameters`; after typed publication validation and defaulting it
  published and executed successfully (1 test).
- The exact native Source association was rejected with an Issue conflict solely
  because it had no manual request; after retaining the actual baseline fence,
  its explicit typed size application passed (1 test).
- OpenAPI lacked `FactoryIssueClassificationFieldContract`; after documenting
  the optional closed contract, the public schema test passed (1 test).

Fixture/catalog corrections and compile errors are not counted as Red. The
additional typed-publication rejection cases characterize the new guarded
contract and passed together with the positive path.

## Regression and checks

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-factory --lib
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::authoring
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --test project_text_intake -- --ignored --test-threads=2
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --test project_text_intake
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --test factory_configured_flow classification \
  -- --ignored --test-threads=2
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-store-postgres --test issue_execution classification \
  -- --ignored --test-threads=2
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers openapi_drift
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-factory -p octacity-server-application \
  -p octacity-server-api-rest -p octacity-server-store-postgres \
  -p octacity-server --all-targets --all-features -- -D warnings
CARGO_INCREMENTAL=0 cargo run --offline --locked --quiet \
  -p octacity-server-api-rest --example export_openapi > /tmp/octacity-51-openapi.json
(cd ui && OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-51-openapi.json pnpm api:generate)
(cd ui && pnpm typecheck)
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Observed unique passing tests:

| Suite | Passed |
| --- | ---: |
| Factory core, including existing immutable triage examples | 351 |
| Application authoring | 80 |
| Project text intake, native and ordinary | 42 |
| Configured classification runtime | 13 |
| PostgreSQL classification execution | 9 |
| REST OpenAPI drift | 14 |
| Native Source association/application | 1 |
| **Total** | **510** |

Two broader text-intake tests initially lacked MinIO configuration. After an
owned disposable MinIO bucket and the HTTP fixture's expected test account were
configured, both passed individually; the HTTP restart test passed in 9.13s.
These environment failures are not behavioral Red. Repeated focused runs are
not counted again. Clippy, generation, TypeScript, formatting, architecture and
strict OpenSpec checks passed. Documentation and comment-only changes have no
executable behavior and require no TDD cycle.
