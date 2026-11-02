## 1. Durable Project policy contracts

Every executable slice below uses repository TDD: run its behavioral test to observe Red before implementation, then Green and the relevant regressions. Record commands and outcomes; do not substitute compilation for behavior tests.

- [x] 1.1 Add bounded typed launch-policy and access-profile definitions/references in the Factory core, reusing permission validation; verify digest/version identity, both manual/Source flags including deny-both, and supported permission narrowing through domain tests.
- [x] 1.2 Add authorized immutable policy/default operations through backend-neutral store ports and PostgreSQL migrations; verify real-store create/read/replace, cross-Project isolation, concurrent preconditions, audit/outbox atomicity and restart-safe idempotent replay using existing store-contract infrastructure.
- [x] 1.3 Add idempotent Factory Project provisioning and exact legacy-reference mapping; verify new/existing Projects, designated default vs no execution grants, repeated provisioning preserving edits, unresolved mappings and unchanged historical Configuration bytes/digests in migration/store tests. Reads must create no state.

## 2. API and authoritative execution

- [x] 2.1 Expose typed Project-scoped list/detail/validate/publication/default-selection contracts through ordinary REST composition and generate OpenAPI/Console types; verify authorized HTTP round trips, safe validation, stale-version/key conflicts and explicit legacy/new representation compatibility in REST contract tests.
- [x] 2.2 Resolve and pin Project policies through Configuration publication, choices, readiness and admission context; verify exact defaults, explicit overrides, unavailable references, effective constraints and no repinning after policy/default replacement through application and persisted Configuration tests.
- [x] 2.3 Enforce trusted manual/automatic origins through the common admission boundary for explicit Work, Issue execution and automatic Source input; verify allowed/denied combinations, imported-Issue origin, independent disabled Source/Configuration, denial before per-item preparation and no Work/Run/Build on denial. Verify accepted replay after restart returns the original operation without redispatch.
- [x] 2.4 Feed pinned access profiles into the existing permission intersection, compiler and protected-operation path; verify restricted host/tool/path/secret selections remain restricted, unsupported/broader profiles fail with limiting-scope diagnostics, and current installation revocation blocks recovery without switching to a newer profile or weaker backend.

## 3. Project settings and Configuration UI

- [x] 3.1 Add separate Launch policies and Access profiles catalogs/editors under Project settings → Factory using existing confirmed commands and typed permission controls; verify create/view/replace/default selection, permission errors, conflict-preserved drafts and no execution side effects in component tests.
- [x] 3.2 Connect visibly labeled Configuration selectors to exact Project defaults, rule/access summaries and View/Edit navigation; verify deliberate overrides, legacy provenance, explicit adoption after return, preserved mounted drafts and reachability of all existing advanced settings in component tests and English/Russian keyboard browser tests at 390 and 1280 pixels.

## 4. Stand compatibility and operator verification

- [x] 4.1 Update `tools/factory-stand` to designate reviewed installed default authority and provision policies through ordinary management commands, without business-name switches or overwriting retained Project settings; verify clean setup, operator edits, failed/retried provisioning and rebuild/resume preserving pipeline/Run/document identities in launcher and provisioning tests.
- [x] 4.2 Extend the durable Factory qualification journey with real HTTP, PostgreSQL and the qualified ordinary Agent path: demonstrate manual-only vs automatic-only admission, effective restricted access, policy replacement and restart, plus legacy Configuration/Run preservation. Verify accepted operation counts and retained references/results; use disposable fixtures rather than a personal stand or real paid model/provider writes.
- [x] 4.3 Document Project policy setup, Standard behavior, Configuration adoption and compatible stand upgrades; run focused Rust/REST/store/Console regressions, production bundle checks with unchanged budgets, formatting/lint, architecture checks and `openspec validate add-project-factory-policies --strict`, recording results and remaining installation prerequisites.

## Verification — 1.1 (2026-11-02)

Domain contracts only: persistence, authoritative ingress enforcement and UI remain pending.
Both publication types retain exact Project/identity/version/content digests, accept only
consecutive replacements, verify retained contents on deserialization and bound serialized
publication content to 256 KiB. Access profiles reuse `FactoryPermissionSet`; validation
rejects broader grants without silently modifying them. Names carry no authority.

TDD slices used `cargo test --locked -p octacity-server-factory --lib <filter>`:

| Filter | Observed Red before implementation | Observed Green |
| --- | --- | --- |
| `project_policy_launch_rules` | Missing typed launch rules/origin API | All four manual/Source flag combinations, including deny-both, passed |
| `project_policy_launch_publications` | Missing publication/identity/version API | Version progression, immutable originals and tampered retained content checks passed |
| `project_policy_access_profiles_reuse` | Missing typed access-profile API | Narrowed hosts, mounts, secrets and resources accepted; broader grants rejected |
| `project_policy_access_publications` | Missing replacement and deserialization API | Version progression and retained identity/permission tampering checks passed |

The combined `project_policy_` filter passed all six tests, including value bounds,
canonical permission ordering and rejection of a complete oversized profile whose
individual permission grants are valid.

Regression/check results:

- `cargo test --locked -p octacity-server-factory --all-features`: passed (341 unit tests, integration tests and doc-tests).
- `cargo clippy --locked -p octacity-server-factory --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `python3 tools/check_architecture.py`: passed (64 packages, 263 dependency edges).
- `openspec validate add-project-factory-policies --strict`: passed.
- `git diff --check`: passed.

No stand services or retained operator data were changed. Final available disk space: 12 GiB.

## Verification — 1.2 (2026-11-02)

Added the backend-neutral `FactoryProjectPolicyStore` and additive PostgreSQL migration 0113.
The two typed policy kinds share publication mechanics, not permission semantics or identity.
Exact reads and bounded current-summary pages are Project-scoped. Publication and explicit
default selection append history with optimistic preconditions, accepted management attribution,
security-scoped idempotency receipts, audit and outbox in the existing mutation transaction.
Reads create no defaults. Policy updates do not repin defaults; clearing a default is explicit.

Authorization follows existing store architecture: application owners must authorize the Project
before reads, commands and replay; mutation inputs require `ManagementMutation`. This slice
does not add application/REST handlers or admission enforcement. Those remain in section 2.

TDD used `cargo test --locked -p octacity-server-store-postgres --test factory_project_policies <filter> -- --ignored`
against a separately created disposable PostgreSQL instance:

| Slice/filter | Observed Red before implementation | Observed Green |
| --- | --- | --- |
| `publications_match` | Missing shared store/publication API | Exact publication/read, scoped receipt and audit/outbox contract passed |
| `publication_rejects_skipped` | Store incorrectly accepted a first publication at v2 | Initial v1 and consecutive replacement validation passed without orphan evidence |
| `defaults_retain` | Missing explicit default-selection/read API | Exact pins, deliberate clearing, stale versions and historical replay passed |
| `catalog_pages` | Missing bounded policy catalog API | Current owned summaries, stable pagination, page limits and foreign cursor refusal passed |

All nine policy integration tests passed. Additional cases cover both policy kinds, authenticated
actor preservation, owner/digest substitution, independent-connection replacement/default races,
changed-body conflicts and receipt isolation between management scopes. After later revisions,
reopened independent pools/store instances return original receipts without new evidence.
Fault injection at outbox insertion rolls back content/defaults, audit and replay reservations;
the same keys then apply successfully after the fault is removed.

Regression/check results:

- `cargo test --locked -p octacity-server-store-postgres --test factory_project_policies --test factory_runtime_settings --test factory_role_profiles --test migrations -- --ignored`: policy 9, Runtime 3 and role/profile 9 passed; migration tests first exposed the outdated exact table inventory. After adding the two new tables to that inventory, `cargo test --locked -p octacity-server-store-postgres --test migrations -- --ignored` passed all 8, including reentrant migration and previous-schema snapshot restoration.
- `cargo test --locked -p octacity-server-store --lib --all-features`: 112 passed.
- `cargo clippy --locked -p octacity-server-store -p octacity-server-store-postgres --lib --all-features -- -D warnings`: passed. The larger access-record variant uses indirection instead of suppressing the enum-size lint.
- `cargo clippy --locked -p octacity-server-store-postgres --test factory_project_policies --test migrations -- -D warnings`: passed.
- `cargo fmt --all -- --check`, `git diff --check`, `python3 tools/check_architecture.py` and `openspec validate add-project-factory-policies --strict`: passed.

Only the disposable test PostgreSQL container/data were removed. The personal stand and its
volumes were untouched; no commits were made. Available host disk space at completion: about 8 GiB.

## Verification — 1.3 (2026-11-02)

Added an explicit management-context-bound initialization/backfill operation to the existing
policy store. It atomically retains ordinary Standard launch/access records, exact defaults,
initialization provenance, compatibility mappings, scoped receipt, audit and outbox. Standard
names do not identify initialized state. Preexisting selections, later edits and deliberate
clearing remain unchanged across independent calls and installation changes. Reads create no state.

Only an explicitly designated exact installed permission ceiling supplies initial access
declarations. Missing designation, missing declarations, unavailable references or ambiguous
declarations yield no execution grants and a retained prerequisite. Declarations initialize
editable policy data; independent execution authority and backend enforcement remain unchanged.

Backfill accepts at most 256 distinct typed legacy references per command. Complete kind,
identity, version and digest form each Project-scoped mapping key. Imported definitions use
the same immutable policy journal and shared append primitive as ordinary publications.
Unresolved inputs are retained in the receipt without creating a binding or selecting Standard.
A later explicit command may resolve them; replay still returns the original receipt.

TDD used `cargo test --locked -p octacity-server-store-postgres --test factory_project_policy_provisioning <filter> -- --ignored`
against a separately created disposable PostgreSQL instance:

| Filter | Observed Red before implementation | Observed Green |
| --- | --- | --- |
| `clean_project` | Missing provisioning command/read contracts | Explicit initialization, Standard rules, deny-all access, read purity and exact replay passed |
| `designated_default` | Initial access incorrectly remained deny-all despite the explicit qualified reference | Only the selected exact declarations were copied; undifferentiated inventory supplied no implicit default |
| `existing_configuration` | Missing exact compatibility mapping/read contracts | Verified legacy contents were imported; unknown digest stayed unresolved; old Configuration remained unchanged |
| `provisioning_rejects` | Duplicate backfill references were accepted | Duplicate and 257-entry batches were rejected without partial state or evidence |

All 8 provisioning tests passed. Additional regressions cover operator edits/defaults,
preexisting custom policies named Standard, independent-pool receipt recovery, parallel
initialization, unresolved-to-resolved backfill, and outbox interruption rolling back every
new definition, mapping, default, provenance record and replay reservation. The historical
Configuration test creates its original row on schema 113, migrates to 114, then verifies
both the stored JSON text/digest and public Configuration read after backfill.

This completes the durable contract/lifecycle slice. Application/REST composition, automatic
entry-path enforcement, Console and stand invocation remain in sections 2–4; no GET or SQL
migration performs operator provisioning or invents missing installed authority.

Final regression/check commands and observed results:

- `cargo test --locked -p octacity-server-store-postgres --test factory_project_policy_provisioning --test factory_project_policies --test factory_runtime_settings --test factory_role_profiles --test migrations -- --ignored`: 37 passed (8 provisioning, 9 policies, 3 Runtime, 9 role/profile, 8 migrations). The exact migration table inventory was extended for the two additive provenance/mapping tables after observing the original inventory checks fail.
- `cargo test --locked -p octacity-server-store --lib --all-features`: 112 passed.
- `cargo clippy --locked -p octacity-server-store -p octacity-server-store-postgres --lib --all-features -- -D warnings`: passed.
- `cargo clippy --locked -p octacity-server-store-postgres --test factory_project_policy_provisioning --test factory_project_policies --test migrations -- -D warnings`: passed.
- `cargo fmt --all -- --check`, `git diff --check`, `python3 tools/check_architecture.py` and `openspec validate add-project-factory-policies --strict`: passed.

The disposable PostgreSQL container and its own volume were removed. Personal stand services,
volumes and operator data were untouched. No commits were made; available host disk space
at completion was approximately 5 GiB.


## Verification — 2.1 (2026-11-02)

Added seven ordinary Project-scoped management operations: current policy metadata catalog,
exact typed policy read, read-only validation, immutable publication, defaults read and explicit
selection, and read-only exact legacy mapping resolution. Launch/access drafts carry editable
contents and an exact previous head; the server computes the next version and content digest.
Legacy installation references and Project-owned references have separate closed wire schemas.
Mixed or unknown fields are rejected without echoing submitted private values.

All operations use existing management authorization. Writes retain the shared policy journal,
Project lock, scoped idempotency, audit and outbox transaction. Validation reuses the publication
head check without writing or reserving a key. Publication recovers accepted receipts before
current installation qualification. Access requests must fit one whole visible installed ceiling
and use selectable dependencies; declarations from multiple ceilings are never unioned. Deny-all
profiles remain editable without installation grants. Existing permission validation and safe
limiting-source diagnostics are reused. The internal choice filter now retains visible ceiling
declarations for qualification; ordinary discovery still applies its separate safe projection.

TDD used `cargo test --locked -p octacity-server --test factory_project_policy_http <filter> -- --ignored`
against a disposable PostgreSQL instance and real released HTTP listeners:

| Filter | Observed Red before implementation | Observed Green |
| --- | --- | --- |
| `launch_policy_http_publication` | Publication route returned 404 | Publication and exact digest/version read passed with Factory execution disabled |
| `launch_policy_http_replacement` | Replacement candidate returned 400 | Consecutive replacement, 412 stale head, 409 changed-key body and original replay passed; final regression also restarted the server on the same database |
| `policy_validation` | Validation route returned 404 | Read-only review, unchanged exact read, reusable mutation key, stale precondition and safe unknown-field rejection passed |
| `policy_http_catalog` | Defaults route returned 404 | Bounded stable catalog pages, explicit pinned defaults, deliberate clearing, stale selection and historical replay passed |
| `access_policy_http_qualification` | Typed access draft was rejected as an unknown variant | Exact declared narrowing, no ceiling union, no key reservation on failure, deny-all without authority and accepted replay after revocation passed |
| `policy_http_legacy` | Legacy resolution route returned 404 | Existing lifecycle-command mappings resolve exactly; unknown digests remain unmapped and Project references cannot be interpreted as legacy references |
| `policy_http_wire` | Required OpenAPI component was absent | Actual HTTP responses match typed schemas; foreign Project reads/pins/replacements and mixed legacy/new input are rejected |
| `access_policy_http_rejects_unselectable` | Validation incorrectly accepted an unavailable plugin from a ceiling declaration (200 instead of 400) | Shared selectable-dependency checks reject both validation and publication with safe field diagnostics and no published policy |

The existing authorization-before-dispatch contract initially failed its complete route inventory
(219 requests versus 226 operations). The common request corpus was extended for all seven new
operations; both allowed dispatch and denial-before-handler/protected-effects contracts passed.
No separate authentication mechanism or execution-state owner was introduced.

Final verification:

- `cargo test --locked -p octacity-server --test factory_project_policy_http -- --ignored`: 8 passed.
- `cargo test --locked -p octacity-server-api-rest --test v1_handlers --test v1_contract`: 252 handler tests and 29 contract tests passed.
- Application library filters `factory_configuration`, `factory_runtime_settings_tests` and `factory_role_profiles_tests`: 52, 14 and 9 passed respectively.
- PostgreSQL tests `factory_project_policies`, `factory_project_policy_provisioning` and `migrations` with `-- --ignored`: 9, 8 and 8 passed respectively, including unchanged historical Configuration data and transaction/replay regressions.
- `cargo test --locked -p octacity-server-factory --lib project_policy_`: 6 passed.
- `cargo test --locked -p octacity-server-store --lib --all-features`: 112 passed.
- `cargo clippy --locked -p octacity-server-application -p octacity-server-api-rest -p octacity-server-store-postgres -p octacity-server --lib --test factory_project_policy_http --test v1_handlers --test v1_contract --all-features -- -D warnings`: passed. The editable access draft uses indirection for its larger variant; no lint suppression was added.
- Exported OpenAPI through the ordinary `export_openapi` example, generated Console types with `OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-project-policy-openapi.json pnpm api:generate`, and ran `pnpm typecheck`: passed. Generated files follow the existing ignored-artifact convention; no Console screen or bundle budget changed.
- `cargo fmt --all -- --check`, `git diff --check`, architecture validation and strict OpenSpec validation: passed.

Rust commands used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`
to bound build storage. Only unused host incremental caches were removed; personal stand data,
services and native staging artifacts were untouched. The disposable PostgreSQL service and its
own volume were removed after verification. No commits were made. Approximately 9 GiB remained
available on the host. Configuration adoption, admission-origin enforcement and final execution
intersection remain in 2.2–2.4; Project editors and stand integration remain in sections 3–4.

## Verification — 2.2 (2026-11-02)

Configuration publication now distinguishes legacy aliases/references from typed
`project_policy` selections. New publications retain exact Project launch/access pins;
legacy clients require an exact retained mapping rather than a default or name lookup.
Both policy fields must use the same representation. The old serialized representation
and immutable Configuration bytes/digests remain unchanged, and an older reader cannot
interpret a Project object as an installation alias.

The existing policy store supplies one exact resolver shared by Configuration management,
readiness and persisted admission contexts. Configuration choices expose authorized exact
Project defaults without provisioning or changing a mounted selection. Publication validates
policy ownership, version/digest availability and the current visible configured authority.
Access qualification reuses the profile-publication rule: a request must fit one complete
verified ceiling, never a union. Authoring diagnostics retain the requested permissions,
show their Project-profile intersection and identify the limiting owner. Readiness observes
retained policy contents separately from current permission availability. Admission contexts
retain either the exact resolved contents or a typed missing/unmapped prerequisite, without
consulting current defaults.

Console transport compatibility preserves exact pins while editing unrelated settings.
New policy diagnostics have English/Russian explanations, and the HTTP error adapter keeps
only the closed public codes/scopes while continuing to remove private diagnostic data.
Full Project catalogs and Configuration policy-selection UX remain in 3.1–3.2. Trusted-origin
enforcement and final compiler/protected-operation intersection remain in 2.3–2.4; this slice
does not claim those execution behaviors are complete. Step input/result isolation retains
its existing owners.

TDD observations (all Rust commands used `--locked`; integration tests used a disposable
PostgreSQL service via `OCTACITY_POSTGRES_URL`):

| Public seam / filter | Observed Red before implementation | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-factory --lib configuration_retains_explicit_project_policy_pins` | A Project object was rejected as an alias string | Exact typed pins round-trip; mixed representations are rejected |
| HTTP `configuration_publication_requires_existing` | Fabricated pins were accepted with 201; a foreign owner was misreported as `/flow` | Missing/digest/version/owner substitutions identify the policy field; failed requests reserve no publication key |
| HTTP `configuration_choices_expose_exact` | Choices omitted Project defaults | Exact explicit defaults are returned without creating another selection |
| HTTP `persisted_configuration_and_admission_context` | The required policy-content context API was missing | Explicit overrides, saved references and original rules survive policy/default replacement and a real server restart |
| HTTP `readiness_observes_exact` | Project pins were checked as installed aliases; no policy observation existed | Exact retained policies and independently compatible Flow installation are observed |
| HTTP `configuration_constraints_explain` | Both requested hosts were shown as effective; revoked authority still validated with 200; readiness remained ready after revocation | Only the profile-allowed host remains effective, original requests remain visible, fresh validation rejects revoked authority, and readiness reports the missing permission prerequisite |
| HTTP `legacy_configuration_publication_requires` | Unmapped aliases were accepted with 201 | Unmapped publication fails explicitly; after authorized exact backfill the same key publishes the mapped Project pins |
| HTTP/OpenAPI response validation | The Configuration response required a legacy identity; diagnostic source cardinality excluded Project policy | Closed legacy/Project pair variants, policy diagnostic codes/scopes and effective-source bounds match actual responses |
| `pnpm exec vitest run src/features/factory/ConfigurationEditor.test.tsx` | Exact Project pins appeared as None | Their version is visible and unrelated edits preserve the original exact pins |
| `pnpm exec vitest run src/features/factory/FactoryPublication.test.tsx -t authoritative` | New policy codes had no presentation mapping | Policy failures have explicit operator explanations and publication remains disabled |
| `pnpm exec vitest run src/api/client.test.ts` | The client dropped both policy codes and installation/runtime limiting scopes | Closed public facts are retained; unknown/private metadata remains redacted |

HTTP filters above used:
`cargo test --locked -p octacity-server --test factory_project_policy_http <filter> -- --ignored`.
The combined suite passed all 14 tests, including the eight prior policy-management cases.
Existing historical-store verification additionally checks that an unmapped Configuration
reports its prerequisite, exact backfill supplies the recorded policy contents, and original
stored Configuration bytes/digests remain unchanged. Type-only fixture adaptations and
formatting introduce no executable behavior; their checks are compilation/typechecking and
format validation rather than an invented Red cycle.

Regression/check results:

- `cargo test --locked -p octacity-server-application --lib --all-features factory_`: 711 passed.
- `cargo test --locked -p octacity-server-api-rest --test v1_handlers --test v1_contract`: 252 and 29 passed. The closed-graph contract assertion now checks both closed policy representations, retaining the required Flow checks and legacy fixture validation.
- `cargo test --locked -p octacity-server-factory -p octacity-server-store --all-features`: Factory 342 unit tests, 46 integration tests and its doc-test passed; store 112 unit tests and its doc-test passed.
- `cargo test --locked -p octacity-server-store-postgres --test factory_project_policies --test factory_project_policy_provisioning --test configurations --test migrations -- --ignored`: 9, 8, 2 and 8 passed; `--test factory_flow_management -- --ignored`: 2 passed.
- `cargo test --locked -p octacity-server --test factory_rest_journey -- --ignored`: passed against disposable PostgreSQL/MinIO. The OpenAPI-only client now publishes ordinary Project policies before Configuration publication, and still completes the nested retained-results journey through the deterministic installed executor. This is not the real-Agent qualification reserved for 4.2.
- `cargo clippy --locked -p octacity-server-application -p octacity-server-api-rest -p octacity-server-store-postgres -p octacity-server --lib --test factory_project_policy_http --test v1_handlers --test v1_contract --all-features -- -D warnings`: passed.
- Exported the authoritative OpenAPI example; `OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-project-policy-openapi.json pnpm api:generate` and `pnpm typecheck`: passed.
- Focused Console Vitest regression over the HTTP client, Configuration editor, authoring, definitions, publication, draft validation and graph editor: 104 tests across 7 files passed.
- ESLint over changed Console files, Prettier, and `pnpm build`: passed. Production JS/CSS/static budgets remain unchanged and pass.
- `cargo fmt --all -- --check`, `git diff --check`, `python3 tools/check_architecture.py` (64 packages / 263 edges), and `openspec validate add-project-factory-policies --strict`: passed.

Rust builds used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`.
Only disposable test services and their own volumes were removed after verification; the
personal stand, retained pipelines and native staging data were not modified. No commits
were made. Approximately 34 GiB remained available after cleanup.

## Review follow-up — policy diagnostics (2026-11-02)

Removed the three policy diagnostic call sites that destructured a generic application
error with `unreachable!()` and rewrote `errors[0]`. A shared field-issue constructor now
accepts the final code and limiting scope directly; unavailable prerequisites retain their
original unavailable-reference marker. Existing invalid/unavailable helpers reuse that
constructor. HTTP fields, codes, scopes and redaction remain unchanged.

This is a behavior-preserving refactor, so repository TDD uses existing behavioral tests
before and after the change rather than an artificial failing test of private structure:

- `cargo test --locked -p octacity-server --test factory_project_policy_http -- --ignored`:
  14 passed before the refactor and 14 passed after it against disposable PostgreSQL.
  These cover unmapped legacy prerequisites, unavailable exact references, installation
  qualification and revoked Runtime authority through real HTTP.
- `cargo test --locked -p octacity-server-application --lib --all-features factory_configuration`:
  52 passed after the refactor.
- `cargo clippy --locked -p octacity-server-application --lib --all-features -- -D warnings`:
  passed.
- Formatting, diff checks, architecture validation and strict OpenSpec validation passed.

Rust commands retained debug/incremental storage limits from 2.2. The disposable test
PostgreSQL container and its own volume were removed; personal stand data was untouched.
No commits were made. Available disk space remained approximately 34 GiB.

## Verification — 2.3 (2026-11-02)

Fresh admission now checks the exact Configuration-pinned launch rules at the common
application boundary. Trusted manual Work/Project text and explicit Issue execution
use `Manual`, including an Issue imported through Source. Installed Source ingress and
resumed candidate preparation use `AutomaticSource`. Provider names, imported links and
caller metadata cannot select the origin. No client origin field or policy fallback was
added. Missing legacy mappings remain actionable prerequisites.

Fresh Source recovery checks the rule before reserving per-item preparation; resumed
preparation checks it before freezing its mapping or publishing bytes. Manual text checks
it before reservation, VCS and artifact publication. Disabled Configuration/Source and
current dependency authorization remain independent. A launch denial has the closed
`launch_policy_denied` diagnostic and durable Source `policy_denied` state: blocked,
without spending a provider retry. Existing Console error/progress surfaces explain this
in English and Russian; policy editors remain in section 3.

Accepted replay still precedes fresh policy resolution. Real HTTP/PostgreSQL verification
admits and executes one deterministic operation, replaces policy/default selections,
restarts the server, then observes the original Work/Run with exactly one executor call.
Denial creates no Run or Build and does not reserve its admission key. Source regression
also recovers its original receipt after an input moves to a deny-both Configuration,
while a genuinely new input is rejected. These use controlled external executor/provider
ports; real Agent qualification remains in 4.2.

Test fixtures explicitly publish/pin policies or seed exact resolved contexts; creating
a fixture Configuration alone grants nothing. The pre-0093 migration rehearsal uses the
existing frozen historical Work writer and retains its Project-text receipt, rather than
making current admission bypass missing policy tables. After upgrade, the same intake
returns the original receipt even without a legacy policy mapping. The status-action
replacement fixture retains its original exact Project policy pins.

Observed TDD (Rust commands used `--locked`, with storage flags below):

| Public seam / test filter | Observed Red | Observed Green |
| --- | --- | --- |
| application `unmapped_launch_policy_blocks_manual` | An unmapped Configuration admitted Work/Run | Explicit prerequisite, no VCS or mutation |
| application `pinned_launch_rules_govern_manual` | Manual-disabled rule admitted Work | All four flag combinations; misleading origin metadata cannot change ingress |
| application `pinned_launch_rules_govern_automatic` | Fresh recovery returned None under an automatic-disabled rule | All four combinations; fresh recovery/admission reject before resolution |
| application `automatic_policy_denial_prevents_resumed` | Preparation published both input documents | No frozen policy, materialization or byte publication |
| application `manual_policy_denial_prevents_project_text` | Text submission created Work/Run | No reservation, VCS or byte writes |
| application `explicit_execution_of_an_imported` | Automatic-only policy accepted explicit imported-Issue execution | Manual rule governs; denial leaves execution history unchanged |
| application `source_launch_denial_is_a_blocked` | Denial became invalid-configuration/provider attempt | Distinct blocked state, zero dependency attempts |
| HTTP `manual_policy_denial_and_accepted_replay` | Actual denial code was outside OpenAPI | Closed diagnostic, no Run/Build on denial, accepted replay after real restart |
| REST `source_diagnostics_document_launch_denial` | Generated-client contract omitted the durable category | Source diagnostics retain the distinct policy block |
| Console client/publication/progress | Client discarded the code; guidance/progress lacked its explanation | Closed code retained and operator guidance rendered |

Regression and required checks:

- `cargo test -p octacity-server-application --lib --all-features`: 975 passed.
- `cargo test -p octacity-server --lib work_source_tests`: 92 passed, 48 service-dependent cases ignored. Includes the real controlled provider process and denial before candidate reservation.
- `cargo test -p octacity-server --test factory_project_policy_http -- --include-ignored --test-threads=4`: 15 passed against disposable PostgreSQL.
- `cargo test -p octacity-server --test factory_work_source_pilot -- --include-ignored --test-threads=4`: 5 passed, including PostgreSQL receipt/delivery recovery and imported-card association.
- `cargo test -p octacity-server --test project_text_intake -- --include-ignored --test-threads=4`: 14 passed with disposable PostgreSQL/MinIO, including killed publishers, historical migration and Issue HTTP restart/replay.
- `cargo test -p octacity-server-api-rest --test v1_handlers --test v1_contract -p octacity-server-store --lib --all-features`: REST library 13, handlers 252, contracts 30 and store 112 passed.
- `cargo test -p octacity-server-application --lib --all-features input_replacement_and_rotated_installation`: passed after strengthening the replacement-policy replay assertion.
- Exported authoritative OpenAPI; `OCTACITY_OPENAPI_SCHEMA_FILE=/private/tmp/octacity-23-openapi.json pnpm api:generate` and `pnpm typecheck`: passed. Generated artifacts retain the existing ignored-file convention.
- Focused Console Vitest over client/publication/Source progress: 69 passed. ESLint, Prettier and `pnpm build` passed with unchanged bundle budgets.

Rust checks use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`.
Integration commands use `OCTACITY_POSTGRES_URL` and, where required, `OCTACITY_MINIO_ENDPOINT`,
`OCTACITY_MINIO_ACCESS_KEY` and `OCTACITY_MINIO_SECRET_KEY` for disposable local peers.
Policy denial is not classified from infrastructure setup failures. No commits were made.
Pinned access enforcement/compilation remains 2.4; no additional authoring controls were
removed or added in this slice.

Final checks: scoped Clippy over application/store/REST/server libraries and the affected
integration contracts passed with `--all-features -- -D warnings`. `cargo fmt --all -- --check`,
`git diff --check`, architecture validation (64 packages / 263 edges), and
`openspec validate add-project-factory-policies --strict` passed. Only the two disposable
policy-origin test containers and their own volumes were removed. The four personal stand
volumes remain intact. Approximately 32 GiB is free; no commits were created.

## Verification — 2.4 (2026-11-03)

The accepted `AdmittedFlow` now retains the exact access-profile publication. Migration
`0115_factory_run_access_profile.sql` adds its bounded nullable snapshot to the existing Run
owner; NULL preserves historical execution contracts. Configuration bytes and old receipts
are unchanged. Admission transactions verify the exact Project pin or recorded legacy mapping,
without a mutable default lookup or acquiring another database connection inside the transaction.

One intersection over the retained ceiling feeds ordinary Build intents (including selected
AI Profiles), server actions, Decision Signal nodes, profile selectors and deferred Reporter
requests. The existing compiler, signed JobSpec permissions, protected tool gate and current
installation/Agent/backend checks continue to apply. This adds no permission language,
credential owner or alternative runtime. Policy references attach no documents or credentials.

TDD commands used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`.
For application rows below, the command was
`cargo test --locked -p octacity-server-application --lib --all-features <filter>`:

| Filter / public boundary | Observed Red | Observed Green |
| --- | --- | --- |
| `admission_retains_the_exact_access_profile` | Accepted Run serialized no access profile | Exact publication retained in the Run and receipt replay |
| `retained_access_profile_blocks` | Server action reached its connector after the profile removed the plugin | Removed plugin/host/secret blocks delivery |
| `installed_signal_node_uses_shadow_baseline` | Restricted signal still proceeded after plugin removal | Missing plugin/host/credential blocks provider reservation and dispatch; original permitted replay still works |
| `optional_status_reporting_is_queued` | Deferred request validation reconstructed broader node permissions | Retained profile limits the reconstructed request |
| `project_access_removes_selector` | Fenced selection continued after its credential was removed | Restricted selection writes no provider request; the same permitted graph proceeds |
| `admission_store_refuses` | Store accepted a substituted resolved profile | Rejected before idempotency/audit/Run publication |

Additional Red/Green boundaries:

- `cargo test --locked -p octacity-server-factory --lib retained_project_access`:
  Red retained broader tools, hosts, paths and secrets; Green emits exactly the reviewed
  subset, preserves it through serialization/intent restoration, and refuses the earlier
  broad intent. Legacy Runs retain their original permissions.
- `cargo test --locked -p octacity-server --test factory_project_policy_http manual_policy_denial_and_accepted_replay -- --include-ignored`:
  Red lost the access profile when reopening PostgreSQL state; Green preserves the exact
  original publication after access/default replacement and a server restart, without redispatch.

Existing compiler and recovery behavior was characterized through
`accepted_project_access_reaches` and `revoked_installation_blocks`: signed ordinary Jobs
contain the narrowed permissions; current installation revocation blocks rematerialization
and retained Build output recovery even when another profile is installed. Original operation
identities, private publications and Run state remain unchanged. Existing tests also verify
unenforceable backends, out-of-envelope tool proposals and installation-scoped diagnostics
for broader or unselectable access profiles.

Regression results:

- `cargo test --locked -p octacity-server-application --lib --all-features`: 980 passed.
  After adding the recovery characterization, its focused command passed; after factoring
  explicit fixture grants, `cargo test --locked -p octacity-server-application --lib --all-features factory_admission_tests`
  passed all 145 admission/Source cases.
- `cargo test --locked -p octacity-server-factory -p octacity-server-store --lib --all-features`:
  343 core and 112 store tests passed.
- `cargo test --locked -p octacity-server --test factory_project_policy_http -- --include-ignored --test-threads=4`:
  all 15 HTTP/persisted-policy tests passed.
- `cargo test --locked -p octacity-server-store-postgres --test migrations --test factory_project_policies -- --ignored --test-threads=4`:
  8 migration and 9 policy-store tests passed, including historical-schema preservation.
- `cargo test --locked -p octacity-server --test factory_work_source_pilot --test project_text_intake -- --include-ignored --test-threads=4`:
  the Source pilot passed all 5 cases. Project text initially exposed two fixtures retaining
  a deny-all profile for newly configured Reporter actions; after explicitly publishing the
  reviewed fixture grants, its standalone rerun passed all 14 cases, including historical
  backfill, killed publishers and restart recovery. The same explicit fixture helper fixed
  the release journeys and Source pilot without relaxing production enforcement or assertions.

Integration runs used separately created disposable PostgreSQL and MinIO services via
`OCTACITY_POSTGRES_URL` and the `OCTACITY_MINIO_*` environment variables. No external provider
or personal stand was used. Clippy over the changed core/store/PostgreSQL/application libraries
and application test targets passed with `--all-features -- -D warnings`.
Only section 2.4 is completed here; Project policy editors and stand provisioning remain in
sections 3 and 4. No commits were created.

Final `cargo fmt --all -- --check`, `git diff --check`, architecture validation (64 packages /
263 edges), and `openspec validate add-project-factory-policies --strict` passed. The two
disposable access-policy test containers and their anonymous volumes were removed; all four
personal stand volumes remain present. Available disk space: approximately 32 GiB.

## Verification — 3.1 (2026-11-03)

Project settings now open separate, lazy-loaded launch-policy and access-profile
catalogs. Exact records and defaults use the typed management API. Publication
and default selection require independent confirmation. Access settings reuse the
typed permission editor with one installed declaration at a time. The server
validates the requested ceiling before review and identifies the limiting scope
when it rejects a request. Conflicts during validation or publication keep the
draft and require an explicit refresh and new review.

From `ui`, the focused TDD commands used
`./node_modules/.bin/vitest run <path> -t '<scenario>'`:

| Test file | Observed Red | Observed Green |
| --- | --- | --- |
| `FactoryPolicies.test.tsx` | Missing catalogs, create/edit actions and explicit default selection | Exact read, deny-both creation, replacement and exact default adoption with independent confirmation |
| `FactoryPolicies.test.tsx` | Missing typed access controls and limiting-scope field feedback | Narrowed access preserves other grants and rejected validation focuses the access field |
| `FactoryPolicies.test.tsx` | An empty enforcement list disabled installed grants. After fixing that, native validation rejected legal zero resource values | Selecting an installed host and reviewing the profile with zero resource limits succeeds |
| `FactoryPolicies.test.tsx` | A stale validation response offered no refresh action | Both validation and publication conflicts preserve edits and require renewed review |
| `policyApi.test.ts` | Missing typed management transport and missing digest on exact GET | Validated publication, idempotency headers, exact digest and independent default selection |
| `ProjectFactorySettings.test.tsx` | Missing Factory settings entry | Open lazily and retain the draft across collapse and reopen |

The component regressions also cover forbidden publication and read-only
inspection. `./node_modules/.bin/playwright test tests/browser/project-documents.spec.ts tests/browser/factory-canvas.spec.ts tests/browser/factory-inputs.spec.ts`
passed 24 tests. Six Linux Playwright snapshots passed against the production
bundle after updating the two Project views for the new settings button.

`./node_modules/.bin/tsc --noEmit`, scoped ESLint/Prettier, and
`node --test scripts/*.test.mjs` (41 tests) passed. Production `vite build` and
`node scripts/verify-architecture.mjs --dist dist` passed with the checked-in
budgets after grouping shared Factory authoring controls and their stylesheet in
one lazy chunk. The installed package binaries were used directly from `ui/node_modules`.

The final `./node_modules/.bin/vitest run` passed all 1,040 tests across 125 files.
`openspec validate add-project-factory-policies --strict` and `git diff --check`
passed. Available disk space was approximately 32 GiB.

## Verification — 3.2 (2026-11-03)

New Configuration drafts initialize from exact Project defaults in the authorized
choices response. Catalog selection, a change to Project defaults and policy
publication are separate operations. The Configuration editor stays mounted
during the Project settings handoff. The operator can adopt a published version
explicitly on return, and focus returns to the initiating control. The same
editor handles replacements with the original queue, budget and Flow settings.

Published Configuration views and replacement drafts resolve historical bindings
through the exact legacy lookup. Original references are inspectable under
Details. Missing or inaccessible bindings require explicit Project choices before
validation. The Configuration request contains both exact Project references.

Red and Green were verified from `ui` using
`./node_modules/.bin/vitest run <file> -t '<scenario>'`.

| File and filter | Observed Red | Observed Green |
| --- | --- | --- |
| `ConfigurationPolicies.test.tsx` / `historical version` | Launch selector was absent. The selected access profile later lacked a readable permission summary | Both exact defaults are named, the access counts reflect the selected profile, and validation sends the deliberate launch override alongside the exact access pin |
| `ConfigurationPolicies.test.tsx` / `mounted draft` | No View / Edit action | Editing and publishing a policy preserves queue changes and the old pin. Explicit adoption changes the pin and focus returns on closing settings |
| `policyApi.test.ts` / `retained legacy` | Legacy lookup was missing from Console transport | The read-only resolver receives the full original immutable reference without mutation headers |
| `ConfigurationPolicies.test.tsx` / `mapped legacy rules` | Published Configuration showed no mapped rules | Exact mapped rules are visible. An unmapped access binding blocks validation until the operator chooses a replacement |
| `ConfigurationPolicies.test.tsx` / `cannot be read` | A forbidden legacy read blocked opening the editor | Draft opens with actionable policy choices and retained settings |
| `ConfigurationEditor.test.tsx` / `retained root Flow` | Selector appeared to select the newer catalog version while the historical Flow was pinned | The exact retained root is shown until a replacement is selected |

Regression results from `ui`:

- `./node_modules/.bin/vitest run`: 1,045 tests across 126 files passed. The
  expanded handoff test also passed a focused rerun with an explicit default
  change during the handoff.
- `./node_modules/.bin/playwright test tests/browser/factory{,-definitions,-canvas,-inputs,-triage,-triage-results}.spec.ts`:
  42 passed, including the English/Russian keyboard matrix at 390 and 1280 px.
  The shared fixture distinguishes the exact legacy read endpoint from commands.
- Six Linux Playwright snapshots passed against the production bundle.
- Type checking, scoped ESLint/Prettier, production Vite build,
  `node scripts/verify-architecture.mjs --dist dist` and
  `node --test scripts/*.test.mjs` (41 tests) passed under the existing budgets.
- `openspec validate add-project-factory-policies --strict` and
  `git diff --check` passed. Available disk space was approximately 32 GiB.

## Review follow-up — 3.1–3.2 (2026-11-03)

A fresh Project-defaults request initializes each new Configuration draft. The
draft then owns its original policy pins and root Flow and keeps them available
for reselection. An incomplete selection has no publication payload. A policy
replacement conflict retains the proposed edits and requires a review against
the newly read current policy.

The existing read-only policy validation returns the exact qualifying installed
ceiling for a nonempty access profile. Installed editor options guide selection.
The server checks resource limits and decides which complete ceiling qualifies.
The qualifying declaration is shown only when its dependencies are visible to
the operator, using the same visibility rule as the choices catalog.

The UI Red → Green checks used `./node_modules/.bin/vitest run <file> -t '<filter>'`
from `ui`. The already-open workbench pinned an obsolete default, the old policy
and Flow options disappeared after switching, the re-review lacked the current
rules, an incomplete draft produced invalid errors for a selected policy, and
native bounds blocked validation of a retained resource grant. Each behavior
passed after its correction. Existing catalog tests cover the shared replacement
draft initialization.

The HTTP Red → Green check used
`cargo test --locked -p octacity-server --test factory_project_policy_http access_policy_http_qualification_never_unions -- --include-ignored`
with disposable PostgreSQL. The original response omitted the qualifying
ceiling. The first preview exposed a nonselectable dependency and failed the
visibility assertion. The final projection covers both a visible declaration
and a declaration with hidden dependencies while keeping qualification exact.

The Console regression run passed 1,047 Vitest tests, 42 Factory Playwright
tests and six Linux visual checks. The production bundle met the existing
budgets and all 41 script tests passed.

All 15 HTTP regressions passed with the final visibility projection using
`cargo test --locked -p octacity-server --test factory_project_policy_http -- --include-ignored --test-threads=4`.
OpenAPI generation, Console type checking, scoped lint and formatting, strict
OpenSpec validation, and `git diff --check` passed. The disposable database
was stopped. Available disk space was approximately 32 GiB.

## Verification — 4.1 (2026-11-03)

The default stand designates the exact reviewed installed access ceiling and
uses the authorized Project provisioning command. Fresh seed Configurations pin
the two policy references returned by the command. Rebuild reads the retained
Configuration and supplies its exact legacy bindings for backfill. Startup
recovery includes provisioning before Agent startup, so an interrupted VCS
installation update can retry provisioning and its existing Source recovery.
If the final stand receipt was lost after Configuration publication, startup
uses the retained Flow and Configuration and completes the existing Source
publication with its stable key.

Red and Green:

| Command or focused test | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test --locked -p octacity-server --test factory_project_policy_http policy_http_legacy_resolution -- --include-ignored` | HTTP 404 on the missing provisioning route | Exact designated authority, defaults and mappings are returned. Restart replays the original receipt. Duplicate inputs and a changed replay body are rejected. |
| `PYTHONPATH=tools:tools/tests python3 -m unittest test_factory_stand_triage.FactoryStandTriageTests.test_triage_selection_pins_current_source_access_and_unlabeled_scope_without_reporter_writes` | Selection could not receive Project defaults | The Configuration draft pins both exact references and retains the reviewed Source scope. |
| `test_factory_stand_recovery.FactoryStandRecoveryTests.test_first_publication_starts_with_the_same_pinned_vcs_bundle_as_its_bootstrap` | Clean setup issued no provisioning command | Clean setup obtains both exact defaults before Configuration publication. |
| `test_factory_stand_recovery.FactoryStandRecoveryTests.test_rebuild_retries_exact_legacy_provisioning_without_republishing_retained_state` | Rebuild skipped provisioning and returned success after the injected failure | Rebuild retries the same exact intent and preserves the receipt and operator defaults. |
| `test_factory_stand_triage.FactoryStandTriageTests.test_publishes_the_resolved_two_stage_pipeline_without_starting_intake_or_work` | Publication could not recover a retained Configuration after loss of the final receipt | The retained Flow and Configuration identities survive and only the Source command is replayed. |
| `test_factory_stand_recovery.FactoryStandRecoveryTests.test_vcs_upgrade_retries_exhausted_enabled_source_through_versioned_publication` | A provisioning failure consumed the VCS upgrade and lost the pending Source recovery | The update retries after the failure and recovers only the eligible existing Source. |

The Python filters use the same `PYTHONPATH` and `unittest` invocation as the
first launcher row. Real setup and restart passed with a current Server binary,
disposable PostgreSQL and MinIO using `python3 -m unittest
test_factory_triage_setup_rest.FactoryTriageSetupRestTests`. This test verifies
the clean policies, exact disabled Configuration, changed operator default and
original/edited Project documents across restart. The PostgreSQL HTTP suite
also verifies that provisioning preserves a retained completed Run and its
accepted Work receipt and does not dispatch again.

- `cargo test --locked -p octacity-server --test factory_project_policy_http -- --include-ignored --test-threads=4`: 15 passed.
- `cargo test --locked -p octacity-server-store-postgres --test factory_project_policy_provisioning -- --include-ignored --test-threads=4`: 8 passed, including legacy Configuration bytes/digest and interrupted transaction recovery.
- `cargo test --locked -p octacity-server -p octacity-server-api-rest --test v1_handlers`: 252 passed, including OpenAPI and authorization inventory.
- `PYTHONPATH=tools:tools/tests python3 -m unittest test_bootstrap_local_stand test_local_stand_launcher test_factory_stand_recovery test_factory_stand_triage test_factory_stand_native test_factory_triage_setup_rest`: 72 passed, 3 opt-in tests skipped.
- `cargo fmt --all -- --check`, `python3 tools/check_architecture.py`,
  `openspec validate add-project-factory-policies --strict` and `git diff --check` passed.

The opt-in native stand journey requires packaged development inputs. The
ordinary Agent qualification remains in 4.2.

## Verification — 4.2 (2026-11-03)

- With the packaged macOS ARM64 Server and Agent, Microsandbox, the pinned guest
  image, and disposable PostgreSQL and MinIO, follow the
  [native policy qualification procedure](../../../docs/operations/factory.md#qualify-project-policies-with-the-native-agent).
  The fixture uses local GitHub and model peers. The first policy run reached the
  Source denial and rejected the fixture's duplicate task and acceptance
  artifact before manual policy validation. The corrected fixture uses
  distinct artifacts and unique operation keys per qualification. The full
  scenario passed in 150 seconds on a clean disposable database.
- Manual admission to the automatic-only Configuration and Source admission to
  the manual-only Configuration produced no Run or Build. One Source operation
  was accepted and executed by the ordinary Agent in two Builds, with four
  retained node completions. Both managed Job templates recorded the smaller
  descendant limit from the exact Project access profile. A manual operation
  and a legacy operation brought the total to three. Restart after replacing
  both policies and selecting the replacements as defaults preserved the
  original Configuration, Run pins, results and accepted operation receipts.
- The release harness passed 27 regular tests, with 21 opt-in tests skipped in
  that invocation. The Python fixture suite passed 4 regular tests, with 6
  opt-in tests skipped. The disposable PostgreSQL checks passed 15 HTTP policy
  tests and 8 provisioning tests.

## Verification — 4.3 (2026-11-03)

Updated [Factory operations](../../../docs/operations/factory.md#project-launch-policies-and-access-profiles),
[pipeline authoring](../../../docs/operations/factory-pipeline-authoring.md#review-purpose-limits-and-publication),
and the [retained stand upgrade procedure](../../../docs/operations/local-stand.md#upgrade-a-retained-factory-stand).
This slice changes documentation only, so no executable behavior required a
new Red/Green cycle.

- Ran the existing `target/debug/deps` executables for
  `octacity_server_factory`, `v1_handlers`, `v1_contract`,
  `factory_project_policy_http`, `factory_project_policies`,
  `factory_project_policy_provisioning` and `migrations` with
  `--include-ignored --test-threads=4` and a fresh disposable PostgreSQL
  database. All 665 tests passed.
- `cargo clippy --locked -j2 -p octacity-server-factory -p octacity-server-application -p octacity-server-api-rest -p octacity-server-store-postgres -p octacity-server --lib --test factory_project_policy_http --test factory_project_policies --test factory_project_policy_provisioning --test v1_handlers --test v1_contract --all-features -- -D warnings`
  passed.
- From `ui`, `pnpm test:unit` passed 41 script tests and 1,047 Vitest tests.
  `pnpm test:browser tests/browser/factory{,-definitions,-canvas,-inputs,-triage,-triage-results}.spec.ts`
  passed all 42 scenarios, including keyboard selection in English and Russian.
  `pnpm lint`, `pnpm format` and `pnpm build` passed. Production output was
  1,578,222 bytes of JavaScript, 87,332 bytes of CSS and 1,666,141 bytes of
  static output under the existing budgets.
- `PYTHONPATH=tools:tools/tests python3 -m unittest test_factory_stand_recovery test_factory_stand_triage test_bootstrap_local_stand test_local_stand_launcher`
  passed all 72 tests.
- `cargo fmt --all -- --check`, `python3 tools/check_architecture.py`,
  `openspec validate add-project-factory-policies --strict` and
  `git diff --check` passed.

Installation still requires a compatible Server/Agent and a qualified
installed ceiling for Standard access. Retained legacy bindings require
exact installed declarations. Source activation and provider credentials
remain independent setup steps. The temporary PostgreSQL was stopped.
Available disk space was 6.1 GiB.

## Review follow-up — 4.1–4.3

- `cargo test --locked -j2 -p octacity-server-application --lib factory_admission_tests`
  passed 146 tests. Before the change, the focused
  `source_waits_for_legacy_policy_backfill_without_exhausting_its_scan` test
  observed `InvalidConfiguration`. The updated schedule waits at the configured
  polling cadence without spending provider attempts and clears on success.
- `PYTHONPATH=tools:tools/tests python3 -m unittest test_factory_stand_recovery test_factory_stand_triage test_factory_stand_native test_local_stand_launcher test_bootstrap_local_stand`
  passed 74 tests with one opt-in test skipped. The focused Red runs observed
  stale defaults after provisioning replay, successful startup with an
  unresolved mapping, a name-only digest, and an update that accepted different
  grants under the same ceiling reference. The updated launcher reads current
  defaults and rechecks unresolved mappings. Generated ceilings bind the
  declarations by digest, and installation updates reject redefining an exact
  ceiling.
- `test_factory_triage_setup_rest.FactoryTriageSetupRestTests` passed with
  disposable PostgreSQL and MinIO. The revised
  [qualification instructions](../../../docs/operations/factory.md#qualify-project-policies-with-the-native-agent)
  list the required packages, service endpoints and environment variables.
  The documentation-only corrections did not require a separate Red/Green
  cycle.
- `cargo clippy --locked -j2 -p octacity-server-factory -p octacity-server-application -p octacity-server-api-rest -p octacity-server-store-postgres -p octacity-server --lib --all-features -- -D warnings`
  passed. Formatting, the architecture check and strict OpenSpec validation
  passed.
- `OCTACITY_TEST_PACKAGED_TRIAGE=1 python3 -m unittest -v tools.tests.test_factory_triage_pipeline.PackagedTriagePipelineTests.test_project_policies_survive_agent_execution_and_server_restart`
  first failed with an empty list of guest observations. The updated probe
  completed both Builds with a kernel process limit of 96 on all six model
  requests and verified that the guest could not raise it. The policy matrix
  retained three admitted operations and their pinned state across restart.
  The final native run passed in 150 seconds using the staged Server and Agent
  packages. The Source scheduling change was verified separately by the
  application regression test above.
