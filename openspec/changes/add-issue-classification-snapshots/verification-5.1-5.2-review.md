# Tasks 5.1–5.2: review corrections

## Scope and behavior

This report supersedes the availability/discovery descriptions in the original
task reports. It does not complete task 5.3 or the visual field controls in 6.4.

Classification publication compiles the selected fields directly from the exact
authorized Project catalog. The optional all-fields preview may exceed a Flow
schema's 16 KiB limit without blocking a valid bounded selection. One authorized
catalog read supplies both metadata and publication validation; no vocabulary or
schema compiler is duplicated.

Retired or missing legacy size labels suppress the offered legacy label mapping,
not the typed classification capability. Typed actions without label effects
remain publishable. Explicit unavailable label effects still reject, and the
legacy Console form remains disabled when no complete mapping is available.

Typed classification can retain the producer's bounded `assessment_inputs`
object. Publication requires an exact accepted-record projection; declared
dependencies cannot be omitted or replaced. Execution independently enforces
the same rule for already prepared graphs. The existing Issue owner validates
subject/material references and staleness. A native test publishes a Flow,
applies classification with an exact comment dependency, then removes the
selected material and observes `MaterialChanged`. Work and provider outbox
remain unchanged.

The preset catalog offers both `ticket-delivery/v1` and `v2`. Opaque continuation
cursors distinguish versions, including at page size one. Existing identity
cursors still skip every version of the named identity. Exact release resolution
continues through the same ordinary authoring owner. Console opens the existing
v1 producer-result example; selecting v2 explains its unsupported required Issue
input and disables opening it. API clients can resolve v2. This does not add the
visual Project-field controls planned in 6.4.

All 161 pre-existing JSON assets under `config/factory` retain their recorded
SHA-256 digests. No migration, provider write, new registry or new permission
boundary was introduced by these corrections.

## Observed Red/Green

All Cargo commands used `CARGO_INCREMENTAL=0`, `--offline` and `--locked`.
Native tests used disposable PostgreSQL; the full intake suite also used MinIO.
Tests exercise public authorized authoring, native execution or rendered Console
controls. Environment and compilation failures were not counted as Red.

1. `valid_large_catalog_can_publish_a_small_field_selection`: a valid large
   catalog published, its preview was absent, and size-only Flow publication
   failed at `/action/parameters/fields`. After selected-field compilation, it
   published successfully.
2. `retired_legacy_label_does_not_disable_typed_classification`: the public
   capability was absent after retiring an unused legacy label. After isolating
   legacy availability, typed publication succeeded and explicit legacy effects
   still rejected.
3. `published_classification_preserves_declared_material_dependencies`: exact
   dependency publication initially failed at `/input_schema`. After supporting
   the envelope, publication, execution and material staleness checks passed.
   Extending the test to reject dependency omission then failed because that
   Flow published. The publication guard made this assertion pass too.
4. `classification_cannot_drop_accepted_assessment_dependencies_at_execution`:
   a prepared graph that projected dependencies away incorrectly resolved
   `done`. The runtime guard made it reject without Issue, record or outbox
   changes; all 14 classification runtime tests passed.
5. The `missing-labels` case in `FlowGraphEditor.test.tsx` initially rendered an
   enabled legacy classification form. After the UI guard, it displayed the
   setup message, kept the action disabled and emitted no classification action.
6. The `required=false` case in `FactoryAuthoring.test.tsx` initially enabled
   opening v2 despite lacking its required input controls. After the guard, v2
   was disabled with an explanation; switching to v1 still completed the existing
   successful open/publication path. All five tests in that file passed.
7. `preset_discovery_retains_versions_with_lossless_pagination`: discovery
   returned only `v2` instead of `v1` and `v2`. After retaining both releases and
   adding version-aware cursors, page-size-one traversal and legacy cursor
   compatibility passed. All 87 application authoring tests passed.

Targeted Red/Green commands:

```sh
export CARGO_INCREMENTAL=0
export OCTACITY_POSTGRES_URL=postgres://postgres:octacity-test@127.0.0.1:55433/postgres
cargo test --offline --locked -p octacity-server --test project_text_intake \
  valid_large_catalog_can_publish_a_small_field_selection -- --ignored --nocapture
cargo test --offline --locked -p octacity-server --test project_text_intake \
  retired_legacy_label_does_not_disable_typed_classification -- --ignored --nocapture
cargo test --offline --locked -p octacity-server --test project_text_intake \
  published_classification_preserves_declared_material_dependencies -- --ignored --nocapture
cargo test --offline --locked -p octacity-server --test factory_configured_flow \
  classification_cannot_drop_accepted_assessment_dependencies_at_execution -- --ignored --nocapture
cargo test --offline --locked -p octacity-server-application --lib \
  preset_discovery_retains_versions_with_lossless_pagination -- --nocapture
(cd ui && pnpm exec vitest run src/features/factory/FlowGraphEditor.test.tsx -t missing-labels)
(cd ui && pnpm exec vitest run src/features/factory/FactoryAuthoring.test.tsx -t required=false)
```

Existing native publication/execution tests passed before and after extracting
the shared execution helper. Documentation and formatting changes have no
executable behavior and require no separate Red/Green cycle.

## Regression qualification

Observed results:

- Application authoring: **87 passed**.
- REST handlers and OpenAPI contracts: **259 passed**.
- Full native Project text intake: **45 passed**, including every ignored test.
- Configured native classification: **14 passed**, including the new execution
  omission guard.
- Console contracts, authoring, graph editor and publication: **68 passed**.
- Build-context preflight: **28 passed**.
- Clippy for application, REST and server, all targets/features with warnings
  denied: passed.
- OpenAPI export and client generation, TypeScript, ESLint, changed-file Prettier,
  frontend and repository architecture, Rust formatting, strict OpenSpec and
  diff whitespace checks: passed.

The first full intake invocation supplied S3-named environment variables instead
of the fixture's `OCTACITY_MINIO_*` variables: 43 tests passed and two stopped on
missing environment values. This was a setup error, not Red. With the documented
fixture variables, the full suite passed (45/45, 112.49s). Disposable services
were stopped after qualification.

```sh
export CARGO_INCREMENTAL=0
cargo test --offline --locked -p octacity-server-application --lib factory::authoring -- --quiet
cargo test --offline --locked -p octacity-server-api-rest --test v1_handlers -- --quiet
OCTACITY_MINIO_ENDPOINT=http://127.0.0.1:59001 \
OCTACITY_MINIO_ACCESS_KEY=octacity OCTACITY_MINIO_SECRET_KEY=octacity-secret \
  cargo test --offline --locked -p octacity-server --test project_text_intake \
  -- --include-ignored --test-threads=2 --quiet
cargo test --offline --locked -p octacity-server --test factory_configured_flow \
  classification -- --ignored --test-threads=2 --quiet
cargo clippy --offline --locked -p octacity-server-application \
  -p octacity-server-api-rest -p octacity-server --all-targets --all-features -- -D warnings
cargo run --offline --locked --quiet -p octacity-server-api-rest \
  --example export_openapi > /tmp/octacity-review-5152-openapi.json
(cd ui && OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-review-5152-openapi.json pnpm api:generate)
(cd ui && pnpm typecheck && pnpm lint && pnpm check:architecture)
(cd ui && pnpm exec prettier --check src/app/presentation/messages.en.ts \
  src/app/presentation/messages.ru.ts src/features/factory/FactoryAuthoring.test.tsx \
  src/features/factory/FactoryExample.tsx src/features/factory/FlowGraphEditor.test.tsx \
  src/features/factory/FlowIssueClassification.tsx)
(cd ui && pnpm exec vitest run src/api/factorySetupContracts.test.ts \
  src/features/factory/FactoryAuthoring.test.tsx src/features/factory/FlowGraphEditor.test.tsx \
  src/features/factory/FactoryPublication.test.tsx)
python3 -m unittest tools.tests.test_verify_local_stand_context
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```
