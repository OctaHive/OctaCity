## ADDED Requirements

### Requirement: Project-owned Factory role and profile management
The REST API SHALL expose bounded authorization-filtered starter template, role, profile, exact-version, and compatible installed-choice discovery; side-effect-free normalized validation; idempotent creation and immutable-version replacement of Project-owned roles/profiles; and narrow Project skill-folder import. Role/profile contracts SHALL expose name, uploaded raster avatar, description, ordered instructions and role-owned shared skills, agent concurrency, runtime/model/thinking level, environment values or logical secret references, CLI arguments, MCP and existing external integration bindings. Console AI-agent creation and editing SHALL use these role/profile contracts and identities, without another registration endpoint or reuse of CI/CD worker enrollment. Template instantiation SHALL be an explicit command with exact template provenance and resolved Project choices. Commands SHALL use existing management authorization, audit, stable safe errors, idempotency, and current-version preconditions. Historical definitions referenced by retained Runs SHALL remain addressable under their visibility and retention contracts. Literal provider/integration/MCP credentials SHALL NOT be accepted as role/profile content; secret environment settings SHALL use existing authorized logical references, and generic collections/audit SHALL expose safe metadata rather than raw instructions, environment values, CLI contents or skill bytes.

#### Scenario: Operator creates a custom role from a template
- **WHEN** an authorized client confirms a validated template copy with resolved compatible profiles and one idempotency key
- **THEN** the API publishes the Project-owned versions, returns their exact references, and replays the same receipt after response loss without duplicate definitions

#### Scenario: Replacement precondition is stale
- **WHEN** a client replaces a profile against an outdated current version
- **THEN** the API returns the existing precondition/conflict classification and publishes no version

#### Scenario: Reader cannot access another Project
- **WHEN** a client lists, reads, validates, or references a role/profile outside its authorized Project scope
- **THEN** authorization is applied before disclosure, visible counts/cursors exclude hidden definitions, and errors do not reveal their contents

#### Scenario: Validation is requested
- **WHEN** a client submits a bounded role/profile draft or stage-selection policy for validation
- **THEN** the API returns normalized exact selections and safe incompatibility reasons without publishing definitions, invoking a provider, resolving credentials, or starting execution

#### Scenario: Client imports a custom skill folder
- **WHEN** an authorized client submits a bounded reviewed relative-path manifest, exact folder-file bytes and an idempotency key
- **THEN** the API uses existing staged retained-content storage to publish one verified immutable skill bundle into the Project's authorized choices, replays its original receipt after response loss, and exposes no partial bundle on failure

### Requirement: Stage profile-selection controls are explicit and replay-safe
The REST API SHALL accept explicit stage profile overrides in Work admission and an authorized preconditioned command to select a profile for an exact eligible undispatched stage occurrence or pending selection. The command SHALL address the admitted stage/attempt and pinned profile reference, SHALL retain actor and immutable intent, and SHALL use the same atomic Run-version, cancellation, deadline, ownership, and replay checks as other Factory controls. A recorded selection intent SHALL NOT be represented as dispatched execution or successful stage completion. Commands SHALL reject choices after dispatch commitment, foreign/unpinned/incompatible references, and stale targets without silent retargeting or fallback.

#### Scenario: Operator resolves a pending choice headlessly
- **WHEN** a client submits a valid pinned profile for the exact pending target using the observed Run precondition and idempotency key
- **THEN** the API records one selection intent and returns a receipt, and only the fenced Flow owner later consumes it for ordinary Build dispatch

#### Scenario: Identical selection is retried after progress
- **WHEN** the same authorized selection intent is retried after its original acceptance and subsequent owner progress
- **THEN** the API replays its original receipt rather than creating another selection, Build, or version

#### Scenario: Admission contains an override for an undeclared stage
- **WHEN** a client supplies an override whose immutable Flow path or profile candidate is not admitted by the selected configuration
- **THEN** admission fails safely without creating partial Work or invoking automatic selection

#### Scenario: Explicit override conflicts with dispatch
- **WHEN** a stage's Build intent was committed before an override command's atomic eligibility check
- **THEN** the command returns a safe conflict and cannot modify that Build's execution package

### Requirement: Safe profile-selection diagnostics and API inventory
REST Factory diagnostics SHALL expose exact role/profile/selection identities, effective safe model/reasoning metadata, selection mode and policy version, matched rule or safe provider receipt linkage, reserve reason, dispatch linkage, and any current pending manual choice with eligible candidates and deadline. Sensitive task, rule-input, instruction, skill, or provider content SHALL require dedicated authorized content access and SHALL NOT appear in generic diagnostic, log, audit, or error payloads. New contracts, bounds, purposes, errors, preconditions, authorization mappings, and replay semantics SHALL be included in OpenAPI, fixtures, generated clients, and the management authorization inventory.

#### Scenario: Operator investigates reserve selection
- **WHEN** an authorized client reads a stage that used a reserve after a provider timeout
- **THEN** diagnostics identify the failed method, safe timeout code, exact reserve and package identities, and later Build linkage without exposing raw provider input

#### Scenario: No role feature is configured
- **WHEN** an existing client admits or reads a statically bound Factory Run
- **THEN** its previous request contract remains accepted and optional role-selection metadata does not become mandatory

### Requirement: Model connection and available-model management
REST SHALL expose Project-authorized operation-shaped configuration, availability and explicit supported model-discovery operations over existing registered runtime/credential mechanisms. Writes SHALL use current-version preconditions, scoped idempotency and audit; reads SHALL not expose credentials or invoke models. Available Runtime/Model choices SHALL feed existing role-profile capability validation rather than another profile catalog. Disabled/revoked configurations SHALL be rejected for new use while retaining exact history. Exact skill inventory/file reads SHALL use retained Project skill imports, no-store responses and current content authority.

#### Scenario: Catalog is configured without invoking a model
- **WHEN** an authorized operator configures a supported provider binding and selects its available Runtime/Model reference
- **THEN** the same exact choice is available for profile validation, configuration survives restart, and no model execution or credential disclosure occurs

### Requirement: Reviewed AI-agent deletion endpoint
The REST API SHALL expose `DELETE /api/v1/projects/{project_id}/factory-roles/{role_id}` with Project Delete authorization, `Idempotency-Key` and strong `If-Match`. It SHALL return a no-store receipt containing disposition, exact role reference and original deletion time. Stale current versions SHALL return 412. Replays SHALL reauthorize and return the original receipt without new audit/outbox evidence. Authorized exact-version history SHALL remain readable after deletion.

#### Scenario: Delete and replay a reviewed AI agent
- **WHEN** an operator submits and retries the same authorized version-fenced delete command
- **THEN** one deletion is committed, replay returns its original role/time, current catalogs omit the agent and exact historical reads still succeed
