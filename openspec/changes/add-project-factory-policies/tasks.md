## 1. Durable Project policy contracts

Every executable slice below uses repository TDD: run its behavioral test to observe Red before implementation, then Green and the relevant regressions. Record commands and outcomes; do not substitute compilation for behavior tests.

- [x] 1.1 Add bounded typed launch-policy and access-profile definitions/references in the Factory core, reusing permission validation; verify digest/version identity, both manual/Source flags including deny-both, and supported permission narrowing through domain tests.
- [x] 1.2 Add authorized immutable policy/default operations through backend-neutral store ports and PostgreSQL migrations; verify real-store create/read/replace, cross-Project isolation, concurrent preconditions, audit/outbox atomicity and restart-safe idempotent replay using existing store-contract infrastructure.
- [x] 1.3 Add idempotent Factory Project provisioning and exact legacy-reference mapping; verify new/existing Projects, designated default vs no execution grants, repeated provisioning preserving edits, unresolved mappings and unchanged historical Configuration bytes/digests in migration/store tests. Reads must create no state.

## 2. API and authoritative execution

- [x] 2.1 Expose typed Project-scoped list/detail/validate/publication/default-selection contracts through ordinary REST composition and generate OpenAPI/Console types; verify authorized HTTP round trips, safe validation, stale-version/key conflicts and explicit legacy/new representation compatibility in REST contract tests.
- [x] 2.2 Resolve and pin Project policies through Configuration publication, choices, readiness and admission context; verify exact defaults, explicit overrides, unavailable references, effective constraints and no repinning after policy/default replacement through application and persisted Configuration tests.
- [ ] 2.3 Enforce trusted manual/automatic origins through the common admission boundary for explicit Work, Issue execution and automatic Source input; verify allowed/denied combinations, imported-Issue origin, independent disabled Source/Configuration, denial before per-item preparation and no Work/Run/Build on denial. Verify accepted replay after restart returns the original operation without redispatch.
- [ ] 2.4 Feed pinned access profiles into the existing permission intersection, compiler and protected-operation path; verify restricted host/tool/path/secret selections remain restricted, unsupported/broader profiles fail with limiting-scope diagnostics, and current installation revocation blocks recovery without switching to a newer profile or weaker backend.

## 3. Project settings and Configuration UI

- [ ] 3.1 Add separate Launch policies and Access profiles catalogs/editors under Project settings → Factory using existing confirmed commands and typed permission controls; verify create/view/replace/default selection, permission errors, conflict-preserved drafts and no execution side effects in component tests.
- [ ] 3.2 Connect visibly labeled Configuration selectors to exact Project defaults, rule/access summaries and View/Edit navigation; verify deliberate overrides, legacy provenance, explicit adoption after return, preserved mounted drafts and reachability of all existing advanced settings in component tests and English/Russian keyboard browser tests at 390 and 1280 pixels.

## 4. Stand compatibility and operator verification

- [ ] 4.1 Update `tools/factory-stand` to designate reviewed installed default authority and provision policies through ordinary management commands, without business-name switches or overwriting retained Project settings; verify clean setup, operator edits, failed/retried provisioning and rebuild/resume preserving pipeline/Run/document identities in launcher and provisioning tests.
- [ ] 4.2 Extend the durable Factory qualification journey with real HTTP, PostgreSQL and the qualified ordinary Agent path: demonstrate manual-only vs automatic-only admission, effective restricted access, policy replacement and restart, plus legacy Configuration/Run preservation. Verify accepted operation counts and retained references/results; use disposable fixtures rather than a personal stand or real paid model/provider writes.
- [ ] 4.3 Document Project policy setup, Standard behavior, Configuration adoption and compatible stand upgrades; run focused Rust/REST/store/Console regressions, production bundle checks with unchanged budgets, formatting/lint, architecture checks and `openspec validate add-project-factory-policies --strict`, recording results and remaining installation prerequisites.

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
