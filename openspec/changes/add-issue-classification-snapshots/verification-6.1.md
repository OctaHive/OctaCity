# Task 6.1: classification REST values, provenance and applicability

## Public behavior

Dedicated Issue detail, exact revision and mutation receipts expose the retained
optional `classification` metadata alongside the canonical content/planning
values. Empty legacy metadata remains absent. Exact edit replay retains the
original values, attribution and revision after later edits. Definitions remain
available through the existing current/exact Project property routes with their
separate administration rights; no per-pipeline vocabulary or new mutable
classification document is introduced.

Two read-only routes delegate to the existing authorized
`ReviewIssueClassificationQuery`:

- `GET /api/v1/projects/{project_id}/issues/{issue_id}/classification`
- `GET /api/v1/projects/{project_id}/issues/{issue_id}/revisions/{version}/classification`

Both use Issue View authority, current visibility, no-store and a strong revision
ETag. They accept no query parameters: server time controls freshness and clients
cannot supply a subject or accepted/evidence flag. No execution target is selected
here, so code-dependent claims retain explicit unresolved-subject diagnostics.
Historical review uses the selected revision's requirements/definition pins and
evaluates evidence freshness at read time. Target-specific execution review is
task 6.2.

The closed OpenAPI components describe all field/origin variants, bounded
attribution, exact definition pins, safe material/dependency/evidence references
and per-field applicability reasons. Bounds reuse the Issue owner's constants.
Material references here are read-only assessment dependencies; document and
material-selection HTTP commands remain tasks 6.7 and 6.8. The existing generated
client consumes the exported schema without handwritten client types. Generated
files stay under the repository's ignored `ui/.generated/api` directory.

REST tests use the existing authorized Issue owner and in-memory store. A retained
pipeline origin and expired selected evidence are restored through the existing
test-only revision port, not accepted from a management request. The reader has
no Factory/transcript service, and the policy denies producer resources. Safe
provenance and unavailable/expired evidence diagnostics remain readable without
producer access. Hidden Issues return 404; denied edits return 403. Forged
classification/origin/actor/evidence/accepted/dependency setters reject across
create, content edit and planning edit. Source-owned changes conflict atomically.
Mutation evidence counts and exact HTTP reads prove that reads and rejections
publish no revision, audit, outbox or replay mutation.

## Observed Red/Green

1. `issue_http_classification_retains_field_origins_history_and_original_edit_receipts`
   failed because HTTP detail omitted committed field attribution (1 failed,
   0.03s). Returning the existing retained metadata made it pass (1 passed,
   0.03s), including exact history and original edit receipt replay.
2. `issue_http_classification_review_is_read_only_and_reports_exact_revision_applicability`
   failed because the review route returned 404 (0.03s). Wiring the existing
   authorized query made it pass (0.03s): changed acceptance reports
   `requirements_changed`, exact historical review retains the old assessment,
   reads cause no mutation and forged query authority is rejected.
3. `issue_http_classification_contracts_are_closed_bounded_and_reuse_project_definitions`
   failed with OpenAPI drift: `IssueMutationResponse.resource: undocumented
   property classification` (0.09s). Closed schemas and the optional resource
   property made it pass (0.09s), including immutable Project definition history.
4. The private-producer authorization test found an additional schema mismatch:
   reviewing a missing component in the empty catalog reports definition version
   zero. The schema originally allowed only a positive component version. After
   documenting zero for this unassessed review placeholder, all five new REST
   tests passed (0.15s). Retained component attributions still enforce positive
   publications in the Issue owner.

Missing test imports and the shared RecordingApplication query fixture were
corrected during setup; compilation errors were not counted as behavioral Red.
The first complete REST run also caught both new routes missing from the common
allow/deny request inventory. Adding those requests and their expected dispatches
restored the full authorization regression without changing product permissions.
Existing authorization/ownership behavior was characterized before any correction
was considered; it needed no new bypass or policy change.

Commands used for the targeted Red/Green cycles:

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers \
  issue_http_classification_retains_field_origins -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers \
  issue_http_classification_review_is_read_only -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers \
  issue_http_classification_contracts -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers issues::classification -- --nocapture
```

## Regression qualification

The complete REST suite passed **337 tests**, with no failures or ignored tests:
13 library, 27 integration administration, 32 wire contract, 264 handlers and one
deterministic export test. It includes both new routes in the common allow/deny
inventory, no dangling OpenAPI references and unchanged existing contract tests.

Application classification regression passed **21 tests** (142.44s), including
manual attribution, stale requirements, exact evidence qualification without
private producer access and retained explicit producer-result execution.
REST/application Clippy passed across all targets/features with warnings denied.
Production server composition also passed `cargo check --lib --all-features`
(36.71s). Rust formatting, the 64-package architecture check, strict OpenSpec
validation and diff whitespace checks passed.

The OpenAPI example was exported and `pnpm api:generate` regenerated the client.
Console typecheck, ESLint and formatting passed. Console unit qualification passed
**44 script tests and 1,108 component tests** (129 Vitest files).

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --all-targets --all-features
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib --all-features classification
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-api-rest -p octacity-server-application \
  --all-targets --all-features -- -D warnings
CARGO_INCREMENTAL=0 cargo check --offline --locked \
  -p octacity-server --lib --all-features
CARGO_INCREMENTAL=0 cargo run --offline --locked --quiet \
  -p octacity-server-api-rest --example export_openapi > /tmp/octacity-61-openapi.json
cd ui
OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-61-openapi.json pnpm api:generate
pnpm typecheck
pnpm lint
pnpm format
pnpm test:unit
cd ..
CARGO_INCREMENTAL=0 cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

No UI behavior, database format, provider integration or packaged delivery path
was changed. Browser/native-service qualification is outside this REST slice;
compilation and client generation are not counted as end-to-end execution.
Documentation-only updates have no executable behavior requiring a separate
Red/Green cycle.
