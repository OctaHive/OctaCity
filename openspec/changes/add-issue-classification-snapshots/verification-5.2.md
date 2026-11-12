# Task 5.2: Issue-based ticket-delivery release

Review corrections and subsequent qualification are recorded in
[the 5.1–5.2 review report](verification-5.1-5.2-review.md). That report supersedes
the discovery and cursor behavior described below.

## Result

Current preset discovery advertises `ticket-delivery/v2`. Exact retained `v1`
references remain resolvable through the same resolver; discovery continues to
list one current version per identity, preserving its identity-based cursor.
Existing publications and Runs are not republished or upgraded.

The new example accepts `issue_snapshot.definition_version` and a reviewed
admission policy instead of an accepted triage predecessor. Its released field
selections compile through the existing Issue snapshot type owner: built-in
priority/component/type/size, dependency references and Project-bound risk,
severity, recommended route and reproducibility. The test Project includes an
additional priority and type, proving that definitions remain Project-owned.
The entry and requirements gates consume explicit projections of one frozen
Incoming schema; neither requires a producer Flow, terminal, schema or Run.

Work scheduling/risk mappings are mandatory for this example and remain separate
from Issue values. The type allowlist must explicitly select a nonempty subset
of the release's supported routes; a new Project type gets no inferred route.
The requirements policy uses choice IDs rather than legacy numeric priority.
The default human-review rule selects component `api`, size `medium`/`large`
and priority `blocker`/`high`; authors review/edit these literal policy choices
in the ordinary Flow draft.

Public application tests resolve and publish the Flow and disabled Configuration
through ordinary authorized commands. A Flow-only resolution returns the same
editable graph. Resolution leaves mutation evidence unchanged. No producing-Flow
reader is installed on the new resolver, and Flow publication has no predecessor
fixture. Required profiles, exact installed commands, trusted publication target,
access ceilings and Project field bindings retain their guards. Competing input
modes are rejected. No intake or provider effect is activated.

The additive OpenAPI contract includes the preset selection and shared snapshot
input wire in Flow/Configuration drafts and responses. Generated client types,
runtime schema generation and existing editor tests pass. This is API authoring;
the visual Project-field controls remain task 6.4. Stage execution, frozen
routing after later Issue edits and branch-specific runtime qualification remain
task 5.3; no claim is made that those new end-to-end tests ran here.

Only new versioned input/requirements assets are added; protected tests,
candidate review, verification and publication reuse existing parts. SHA-256
comparison confirmed all **161 pre-existing JSON assets** under `config/factory`
are byte-for-byte unchanged. Docker allowlisting adds only four named v2 assets;
context preflight checks current and historical manifests and keeps private
configuration excluded.

## Observed Red/Green

All Cargo commands used `CARGO_INCREMENTAL=0`, `--offline` and `--locked`.
The public resolver/publication seams reuse existing in-memory owner fixtures;
no disposable database, object store, provider credentials or live forge was
needed.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib \
  delivery_discovery_offers_issue_snapshot_v2_without_a_required_producer -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib \
  issue_delivery_requires_project_input_contract_instead_of_a_producer -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib \
  issue_delivery_resolves_and_publishes_project_contract_without_any_producer -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib \
  issue_delivery_requires_reviewed_admission_and_supported_kind_allowlist -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers \
  ticket_delivery_v2_contract_accepts_project_input_without_a_predecessor -- --nocapture
python3 -m unittest tools.tests.test_verify_local_stand_context.LocalStandContextTests.test_issue_delivery_v2_assets_survive_the_filtered_build_context
```

Each test reached a behavioral Red before its implementation:

1. Discovery returned `v1`, not the required current `v2`.
2. Missing-input review requested `/accepted_predecessor`, not `/issue_snapshot`.
3. Resolution returned a draft with no Issue snapshot. After binding Project
   types and the shared input, Flow and Configuration publication passed (0.42s).
4. A missing admission mapping still produced a Flow. Required reviewed policy
   and supported-type selection then passed together with the positive path.
5. Closed OpenAPI rejected `issue_snapshot` as an undocumented request property.
   The additive input/output contract then passed (0.07s).
6. Build preflight omitted all four new assets; the Docker filter excluded them.
   After the exact allowlist/preflight change the test passed (0.44s).

Compile/fixture corrections are not counted as Red. The existing v1 publication
test passed before extracting its shared installed-capability fixture and stayed
passing. Additional rejection cases characterize preserved dependencies and
field-specific failures; they are not counted as invented Red cycles.

## Regression and checks

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::authoring -- --quiet
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers -- --quiet
python3 -m unittest tools.tests.test_verify_local_stand_context
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-application -p octacity-server-api-rest -p octacity-server \
  --all-targets --all-features -- -D warnings
CARGO_INCREMENTAL=0 cargo run --offline --locked --quiet \
  -p octacity-server-api-rest --example export_openapi > /tmp/octacity-52-openapi.json
(cd ui && OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-52-openapi.json pnpm api:generate)
(cd ui && pnpm typecheck)
(cd ui && pnpm exec vitest run src/api/factorySetupContracts.test.ts \
  src/features/factory/FactoryAuthoring.test.tsx \
  src/features/factory/FlowGraphEditor.test.tsx \
  src/features/factory/FactoryPublication.test.tsx)
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

| Unique regression suite | Passed |
| --- | ---: |
| Application authoring, including retained v1 and new v2 | 86 |
| REST handlers and OpenAPI | 259 |
| Build-context policy/preflight | 28 |
| Client contracts and existing editor behavior | 67 |
| **Total** | **440** |

All listed suites and checks passed. Repeated focused runs are counted once.
Architecture remains 64 workspace packages and 263 dependency edges. Local
documentation targets were checked separately. Documentation/comment-only
changes have no executable behavior and require no TDD cycle. No commit was
requested or created.
