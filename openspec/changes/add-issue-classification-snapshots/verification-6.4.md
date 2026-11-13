# Task 6.4: Project field reuse and Issue snapshot authoring

Post-review corrections and their final qualification are recorded in
[the review verification](verification-6.4-review.md).

## Behavior and ownership

The existing authoring-capabilities API exposes optional `issue_fields` from one
independently authorized current Project catalog. It includes individual field
shapes, source identities, display names, required-field availability and the
supported local-classification target mask. No Issue values, bodies, producing
Runs or writer installation are needed to discover snapshot fields. The local
classification choices and field contracts share the same catalog publication.
The shared Rust field compiler owns types, enums and bounds; a combined preview
exceeding the Flow schema budget does not hide individual supported contracts.

The result editor can explicitly reuse selected Project classification contracts.
The trusted action editor authors typed target masks without legacy size labels,
projects only selected accepted-record fields and retains declared
`assessment_inputs` exactly. It can update its dedicated projection without
replacing unrelated inputs. Existing label-based actions retain their mode.
Publication reviews typed targets against the current installed writer/catalog
and keeps legacy label review available.

The snapshot editor selects built-ins, ordinary properties and classification
bindings with explicit aliases and required/optional flags. Built-in/custom alias
collisions stay distinct. Materials are opt-in, with a selected-reference policy,
entry/byte bounds and required/optional status. Each step explicitly chooses its
own projections. Missing optional fields are omitted rather than invented.
A complete snapshot projection uses the full declared Incoming contract; a
subset uses bounded conditional mapping. OpenAPI exposes the domain's mapping
and input-binding byte ceilings so the browser does not duplicate those limits.

Schema normalization uses the existing read-only API. Async results cannot
overwrite a changed graph/catalog; leaving the form aborts its request. An open
snapshot draft blocks publication through the existing editor readiness fence.
No additional permissions are granted. Publication reviews snapshot version,
requirements, material policy and step projections alongside existing access
review. Exact producer-result authoring remains in pipeline settings, including
historical versions. Removing a snapshot clears only its exclusive Incoming
bindings, retaining unrelated accepted-record bindings and registered schemas.
Mixed inputs are preserved and require explicit resolution.
If a newer catalog cannot supply a previously required field, application is
blocked until the author explicitly makes it optional or resolves the selection.
The form never silently downgrades the published requirement.

The visual material form authors a reusable bounded `selected` policy. Existing
admission/evidence/arbitrary exact-reference policies remain executable and
reviewable; the form preserves them and directs maintenance to the management
API instead of overwriting them. Issue-specific material selection/preview and
start controls remain tasks 6.5 and 6.8–6.10. No stand rollout, native provider
qualification or external repository effect is claimed by this slice.

## Observed Red/Green

Tests use application queries, actual REST responses/OpenAPI and existing React
editor/publication boundaries. Fixture compilation/setup mistakes were corrected
and were not counted as behavioral Red. Each production behavior below followed
its observed failing assertion.

| Behavior | Observed Red | Green evidence |
| --- | --- | --- |
| Independent field discovery | `issue_fields.definition_version` was missing | Application capability test passed |
| Public field discovery wire | Response had an undocumented `issue_fields` property | REST capability response passed full schema validation |
| Available classification targets | Classification selection mask was missing | Exact catalog mask/property assertions passed |
| Result contract reuse | No **Reuse Project classification fields** control | Result normalized selected Project enums without manual typing |
| Typed local classification | No selectable Project component target | Selected mask and exact dependency input passed without size labels |
| Snapshot field/material authoring | No **Configure Issue snapshot** control | Required component, optional custom field, bounded materials and different step inputs passed |
| Authoritative mapping bounds | OpenAPI byte ceilings were null | Both actual 16 KiB contract ceilings passed |
| Typed publication review | Selected semantic target was absent | Typed target, snapshot requirement/material/projection review and confirmation readiness passed |
| Oversized subset projection | No bounded rejection before normalization | Oversized optional subset is rejected without an API normalization call |
| Source alias collision | Selecting a custom `component` deselected the built-in | Both selections and distinct editable aliases passed |
| Mode removal | No **Remove Issue snapshot** action | Exclusive inputs cleared; exact record input and schemas preserved |
| Typed action re-edit | A dedicated existing input disabled the new selection | Target mask and input could be explicitly updated |
| Mixed input preservation | Reassigning projections enabled replacement of mixed sources | Apply stayed disabled and no graph mutation occurred |
| Complete optional input | All optional fields produced no updated draft | One normalized schema and direct complete Incoming projection passed |
| Complete projection review | Review showed `*` instead of declared fields | Review expanded the exact declared field/material aliases |
| Lost required-field availability | A required selection stayed applicable after its choices disappeared | Apply is blocked; the author can explicitly choose optional |

Representative command boundaries:

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-application --lib \
  authoring_offers_project_issue_field_contracts_without_a_classification_writer_or_body_reads
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest --test v1_handlers \
  installed_command_compiler_facts_match_the_public_authoring_contract
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest --test v1_contract \
  factory_authoring_contract_exposes_schema_and_mapping_nesting_limits
cd ui
pnpm exec vitest run src/features/factory/IssueFieldAuthoring.test.tsx
pnpm exec vitest run src/features/factory/FactoryPublication.test.tsx \
  -t 'reviews selected typed'
```

Local cycle logs are `/tmp/octacity-64-{field-contract,field-targets,field-wire,
result-contract,typed-action,snapshot-editor,mapping-bound,review,projection-bound,
modes-aliases,typed-reedit,mixed-input,complete-input,complete-review}-red.log`.
The availability cycle is `/tmp/octacity-64-required-availability-{red,green}.log`.
The complete-input editor/publication Green passed **37 tests**, followed by
**10 passing editor tests** including required-field availability. An earlier focused
graph/settings/publication regression passed **72 tests**, including exact
producer-result creation and historical version selection.

## Qualification

- Full UI unit run: **44 script tests and 1,127 Vitest tests**, 131 files, passed.
- Affected browser regression: **18 passed** across Factory canvas, existing
  delivery/triage-result journeys and Issue classification. After the complete
  projection refinement, both new EN/RU 390 px keyboard journeys passed again.
  They assert declared step inputs and no horizontal overflow. The Russian
  snapshot screenshot was inspected.
- Application snapshot regression: **24 passed**; explicit field discovery and
  existing installed-command compiler discovery also passed individually.
- REST regression: **32 `v1_contract` and 273 `v1_handlers` tests passed**.
- API regeneration, TypeScript, ESLint, Prettier and production build with
  frontend architecture/bundle budgets passed.
- Clippy for affected application/REST libraries and tests passed with warnings
  denied. Rust formatting passed.
- Repository architecture: **64 packages, 263 dependency edges**. Strict OpenSpec
  validation and diff whitespace checks passed.

```sh
CARGO_INCREMENTAL=0 cargo run --offline --locked -p octacity-server-api-rest \
  --example export_openapi > /tmp/octacity-64-openapi.json
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-application --lib issue_snapshot
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-application --lib \
  installed_command_authoring_exposes_exact_compiler_facts_without_a_saved_flow_or_agent_assignment
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest \
  --test v1_contract --test v1_handlers
CARGO_INCREMENTAL=0 cargo clippy --offline --locked -p octacity-server-application \
  -p octacity-server-api-rest --lib --tests -- -D warnings
cargo fmt --all --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
cd ui
OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-64-openapi.json pnpm api:generate
pnpm test:unit
pnpm exec playwright test tests/browser/factory-canvas.spec.ts \
  tests/browser/factory-ticket-delivery.spec.ts tests/browser/factory-triage-results.spec.ts \
  tests/browser/issue-classification.spec.ts
pnpm exec playwright test tests/browser/factory-canvas.spec.ts -g 'bounded Issue snapshot'
pnpm typecheck
pnpm lint
pnpm format
pnpm build
```

UI browser qualification uses the existing mocked HTTP fixture boundary. Rust
tests qualify the native application/HTTP contracts independently. The broad
optional `classification` application-name filter was stopped after a long-running
case outside the focused scope; it is not reported as passed. The focused checks
above completed. Documentation-only edits have no executable behavior requiring
an additional TDD cycle.
