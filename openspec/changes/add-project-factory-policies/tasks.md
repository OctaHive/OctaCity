## 1. Durable Project policy contracts

Every executable slice below uses repository TDD: run its behavioral test to observe Red before implementation, then Green and the relevant regressions. Record commands and outcomes; do not substitute compilation for behavior tests.

- [x] 1.1 Add bounded typed launch-policy and access-profile definitions/references in the Factory core, reusing permission validation; verify digest/version identity, both manual/Source flags including deny-both, and supported permission narrowing through domain tests.
- [x] 1.2 Add authorized immutable policy/default operations through backend-neutral store ports and PostgreSQL migrations; verify real-store create/read/replace, cross-Project isolation, concurrent preconditions, audit/outbox atomicity and restart-safe idempotent replay using existing store-contract infrastructure.
- [x] 1.3 Add idempotent Factory Project provisioning and exact legacy-reference mapping; verify new/existing Projects, designated default vs no execution grants, repeated provisioning preserving edits, unresolved mappings and unchanged historical Configuration bytes/digests in migration/store tests. Reads must create no state.

## 2. API and authoritative execution

- [ ] 2.1 Expose typed Project-scoped list/detail/validate/publication/default-selection contracts through ordinary REST composition and generate OpenAPI/Console types; verify authorized HTTP round trips, safe validation, stale-version/key conflicts and explicit legacy/new representation compatibility in REST contract tests.
- [ ] 2.2 Resolve and pin Project policies through Configuration publication, choices, readiness and admission context; verify exact defaults, explicit overrides, unavailable references, effective constraints and no repinning after policy/default replacement through application and persisted Configuration tests.
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
