# Shared preset discovery and resolution (7.1)

## Delivered boundary

The single application owner is `FactoryPresetHandlers`, composed into the normal
management server and exposed as authorized, non-cacheable queries:

- `GET /api/v1/projects/{project_id}/factory-presets?limit=1`:
  bounded Project-visible release discovery, content-bound release reference,
  typed parameter requirements, independent profile aliases and required technical
  node kinds, with an optional exclusive `after` cursor. Discovery remains available before Factory execution is installed.
- `POST /api/v1/projects/{project_id}/factory-presets/resolve`:
  select the exact discovered `preset`, prospective `flow_id` and
  `configuration_id`, independent exact stage `profiles`, same-Project
  `repository`, per-stage `context`, explicit `execution` ceilings and installed
  `configuration` policy aliases. Optional `input` selects existing connection,
  Source/Reporter, supported predicate and literal reporting contracts. Optional
  `effects` maps advertised accepted gate aliases to caller-owned child Flow IDs
  and the existing `FactoryResultReportingDraft` parameters.

A complete response contains ordinary `flow` and `configuration` drafts, optional
ordinary disabled `input` intent, and ordinary child Flow `dependencies` in
publication order. The Configuration includes the complete pinned root/child
closure and deduplicated content-bound schemas. Effects use the existing finite
reporting compiler/binder and installed Source action authorizer; no names dispatch
business behavior. Input and effect targets must agree when selected together.

`completion` is the reviewed bundled process-completion draft, explicitly separate
from ticket-authored implementation acceptance. Resolution writes no managed
input or document. Exact selected profiles retain their ordinary runtime/MCP and
safe reference contracts; resolution obtains no secret values or MCP observations.

Missing selections produce `required` diagnostics at their request-field paths.
Unavailable exact dependencies produce safe `unavailable` diagnostics; affected
responses contain no publication drafts or private installation errors. Closed
requests reject unsupported authority, model, provider and activation fields.

Repository checks use the existing authorized immutable Repository query and its
canonical digest. Context checks use the existing Artifact metadata authority,
including exact ownership, publication and retention; they never invoke content
reads. Artifact visibility is checked again after metadata reads, and Project
visibility is rechecked before disclosing complete private profile drafts.
Installed Source/Reporter choices remain the current metadata/grant authority.

## Ordinary review and publication

Resolution reserves no identity, version, budget or lease and emits no mutation,
audit or outbox effect. It has no provider/model/secret-value service. Tests make
Artifact content reads fail immediately and observe unchanged mutation evidence.

An operator reviews the returned content, then uses existing Flow and Factory
Configuration validate/publish endpoints with their normal idempotency keys and
version preconditions. Standalone child Flow publications precede standalone root
publication; the ordinary Configuration draft already contains its complete
closure. The optional input is separately published through existing Source
configuration authoring after its exact target Configuration exists. Both
Configuration and input drafts remain disabled. Current dependencies and grants
are checked again by those publication owners; a draft is neither authority nor
an executable-readiness certificate. Exact Configuration/Repository readiness is
7.2, and Console setup is later work.

## TDD evidence

Common command prefix (debug information and incremental compilation disabled):

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -j2 -p octacity-server-api-rest --test v1_handlers <filter> -- --nocapture
```

| Public HTTP behavior / filter | Observed Red | Observed Green |
| --- | --- | --- |
| `preset_discovery_advertises_typed_parameters` | Required technical kinds and typed parameters absent | Per-stage typed requirements and required technical kinds advertised |
| `presets_discovery_exposes_bounded_release_identity` | 404 instead of 200 | Bounded discovered release without execution installation |
| `preset_resolution_returns_reviewable_process_criteria` | 404 instead of 200 | Reviewed process criteria, exact missing paths, no mutations |
| `selected_preset_profiles_resolve` | 400 instead of 200 | Exact independently selected profiles in ordinary Flow/Configuration drafts |
| `factory_input_sources_document_the_actual_retained_record_views` | OpenAPI omitted domain-supported `input_payload` | All three retained-record views are documented |
| `preset_resolution_rejects_a_forged_repository_pin` | Forged digest returned no diagnostic | `/repository` unavailable, no drafts or mutation |
| `preset_resolution_checks_exact_context_metadata` | Forged document digest returned no diagnostic | Exact context path unavailable, no object-byte read |
| `preset_resolution_rechecks_project_visibility` | Complete private draft disclosed after Project revocation | Request rejected after asynchronous reads |
| `preset_resolution_reports_unavailable_source_selection` | Unsupported body returned 400 | Explicit safe `/input` dependency reason |
| `selected_source_and_literal_effects_resolve` | Installed selection still unavailable | Ordinary disabled input intent with exact target, Repository and effects |
| `preset_resolution_reports_the_unavailable_exact_profile` | 400 instead of actionable response | Exact stage profile path unavailable without private details |
| `preset_resolution_rejects_an_unpaired_connection_writer_pin` | Inconsistent writer pin accepted | Same invariant as ordinary Source publication |
| `preset_resolution_rechecks_individual_artifact_visibility` | First document's revocation was missed | Artifact reauthorization after metadata read |
| `selected_accepted_gate_effects_resolve` | Unsupported `effects` request returned 400 | Ordinary reporting child Flow, complete closure and normal confirmed Configuration publication |
| `preset_resolution_names_an_unavailable_factory` | 422 instead of dependency response | `/execution` unavailable without drafts |
| `preset_effects_cannot_resolve_against_another` | Two authorized but inconsistent connections accepted | Input/effect consistency checked without enabling intake |
| `preset_resolution_returns_a_safe_dependency` | Hidden Repository returned a generic 400 | Safe `/repository` unavailable reason without private details |
| `flow_identity_collisions` | Different children sharing one ID accepted; root collision pointed at `/profiles` | Duplicate prospective root/child IDs rejected at `/effects` |

The released composition test uses explicitly configured disposable PostgreSQL:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -j2 -p octacity-server --test factory_presets -- --ignored --nocapture
```

Its observed Red was 503 for the normal server's discovery route; Green exposes
list/resolution before Factory execution is installed. The endpoint uses the
normal application/REST composition and existing PostgreSQL metadata authority.

Existing publication/unsupported-field/page-limit contracts are qualified through
their ordinary HTTP boundaries, without inventing Red for already implemented
behavior: validate/create, exact replay with unchanged mutation evidence,
`If-Match` requirements, stale-version rejection, current installation withdrawal,
bounded cursors and unsupported parameter rejection.

## Quickstart handoff

`add-factory-bugfix-quickstart` must reuse this owner, operation family, ordinary
versioned publication and dependency vocabulary. A later bundled preset supplies
its own immutable release assets and explicit parameter contract. This task adds
no template registry, certification/provenance lifecycle, business dispatcher,
bugfix preset, text-intake certification or candidate/hidden-test pilot.

## Regression and review

- Full management HTTP regression: 206 passed, including 20 new preset tests,
  route/authorization inventory, closed schemas and dangling-reference checks.
- Full application regression: 861 passed. After extracting shared pure Source
  validation, the existing 33 Source-authoring tests and 14 reporting-composition
  tests passed again.
- The released PostgreSQL/HTTP composition test passes with Factory execution
  absent. It uses a task-owned disposable database and no object-byte operation.
- Clippy passes for the three changed Rust crates with all targets/features and
  warnings denied. Rustfmt and `git diff --check` pass.
- OpenAPI export, generated TypeScript declarations, Console typecheck and seven
  schema/Source constraint tests pass. Architecture remains 64 packages and 263
  dependency edges; strict OpenSpec validation passes.
- Independent spec and standards reviews found and drove the writer-pin,
  post-metadata Artifact authorization, accepted-gate reporting, input/effect
  correlation and prospective Flow identity corrections described above. Existing
  Source validation is shared instead of duplicated; installed policy remains the
  authority. No outstanding review finding remains.

Documentation has no executable behavior; its checks are strict OpenSpec
validation and repository diff/format checks. Task 7.2 and later tasks remain open.

Final regression commands (using the same Cargo profile environment above):

```sh
cargo test --locked -j2 -p octacity-server-api-rest --test v1_handlers
cargo test --locked -j2 -p octacity-server-application --lib
cargo test --locked -j2 -p octacity-server-application --lib source_configuration
cargo test --locked -j2 -p octacity-server-application --lib factory_flow_action_tests::result_reporting
cargo clippy --locked -j2 -p octacity-server-application -p octacity-server-api-rest -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-triage-pipeline --strict
pnpm --dir ui typecheck
node --test ui/scripts/factory-authoring-schemas.test.mjs ui/scripts/issue-source-constraints.test.mjs
git diff --check
```

The OpenAPI document was exported with the ordinary `export_openapi` example and
used by `ui/scripts/generate-api.mjs` before typecheck. Temporary PostgreSQL was
removed after qualification. Free disk space remained 48 GiB at completion. No
commits were created for 7.1.
