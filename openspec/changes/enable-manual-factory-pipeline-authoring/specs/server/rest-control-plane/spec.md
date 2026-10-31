## MODIFIED Requirements

### Requirement: Bounded Work admission commands
The management REST API SHALL accept a manual provider-neutral Work Submission containing a visible Factory Configuration, scoped external identity, explicit repository-backed or repository-free subject, task and process-completion/acceptance inputs, optional specification references, bounded priority and risk, and bounded metadata. Repository-backed inputs SHALL retain an allowed base reference; repository-free inputs SHALL require no synthetic Repository. Step resources SHALL be explicit exact references in the reviewed configuration or supported admitted selections. The command SHALL use an idempotency key and return the existing logical admission result for an exact scoped replay before resolving mutable source state again. Legacy repository-backed requests and replay identities SHALL retain their meaning.

#### Scenario: Manual Work is admitted
- **WHEN** an authorized client submits valid Work to an enabled Factory Configuration
- **THEN** the API returns immutable Work/Run identities, the exact selected subject/resources or explicit source absence, current state and safe request correlation without source or model credentials

#### Scenario: Manual Work replay changes content
- **WHEN** a client reuses an admission idempotency identity with a different subject, task, base reference, criteria, selected resources or configuration
- **THEN** the API rejects the mismatch and neither mutates the existing Factory Run nor creates another one

## ADDED Requirements

### Requirement: Headless authoring can create the first executable pipeline
Project-visible typed authoring choices SHALL expose compatible installed execution/result bindings, gate policy parameter/result contracts, exact Role/Profile references, supported resource modes and safe prerequisite reasons without consulting an existing Flow as the source of those bindings. Shared normalized validation SHALL support an empty starting draft and operator-authored schemas, goals/criteria, steps, projections and effects. It SHALL accept the pipeline's available code source declarations and each step's explicit attachment separately, enforce declared ceilings and current authorization, and reject undeclared, dangling or overbroad attachments. Declaring availability SHALL cause no checkout or implicit step attachment. It SHALL perform no model/MCP/provider call, credential-value read or publication. Flow/Configuration publication, Source preview and activation SHALL retain their ordinary independent versioned commands. No triage-shaped request SHALL be necessary.

#### Scenario: Client has no previous Flow reference
- **WHEN** a headless client discovers capabilities in a new Project and constructs two reasoning steps and their configured gates
- **THEN** it can validate and publish them with the same exact bindings and safety checks as Console, without a preset or a copied execution node

#### Scenario: Choice changes before publication
- **WHEN** a selected capability, Profile, document version, repository permission or Reporter grant becomes unavailable after validation
- **THEN** publication or dispatch rejects the stale/unauthorized selection without silently repinning, exposing secrets or enabling a partial configuration

#### Scenario: Client edits a task while retaining its executor
- **WHEN** a headless client submits changed step instructions/result contracts with unchanged Role/Profile references
- **THEN** common validation and immutable Flow publication retain the explicit step assignment independently of those references, enforce compatibility and policy, and do not rewrite the reusable agent or existing Runs

### Requirement: Authoring validation explains effective execution constraints
Shared read-only authoring validation SHALL expose effective budget, concurrency and permission constraints with applicable scopes and safe origin references/explanations using existing policy owners. Results SHALL distinguish requested settings, enclosing ceilings, cumulative budgets, inherited values and unresolved/unsupported enforcement rather than invent a single unscoped minimum. Console and headless clients SHALL receive the same authoritative normalization. Returning these explanations SHALL not grant permissions, read secret values, invoke providers or replace current publication/dispatch authorization checks.

#### Scenario: Client requests a limit above an enclosing bound
- **WHEN** a draft exceeds an applicable Project/Configuration/Flow/Profile bound
- **THEN** validation identifies the affected setting, requested value, applicable bound and origin without silently changing policy or publishing the draft

### Requirement: Project document management remains separate from attachment
The API SHALL reuse Project input document discovery, exact authorized content reads and immutable publication for Project settings and step selectors. Metadata collections SHALL remain bounded and content-free; body reads SHALL enforce current Project/content authorization, retention and no-store behavior. Publication SHALL not attach the result to any step, replace process criteria, enable intake or mutate accepted Runs. New criteria content SHALL be frozen through existing managed-input publication with stable replay before admission.

Common authoring SHALL accept a bounded collection of exact Project document references with explicit instruction/reference roles for each step and normalize them to existing typed Context bindings. It SHALL require no fixed goals/rules/supporting-document slots. Document names or content SHALL not infer instruction authority. Selected instructions SHALL retain existing controlling-policy semantics; reference material SHALL remain data, and neither role SHALL grant execution permissions. Explicit Profile requirements and independently required Project policy SHALL remain enforced.

#### Scenario: Client submits a unified step document selection
- **WHEN** an authorized client validates multiple exact same-Project document selections with instruction/reference roles for one step
- **THEN** normalization retains those references and roles only for that step using existing Context contracts, without publishing new documents, inferring authority from their names or changing sibling inputs

#### Scenario: Document-guided assessment has no vocabulary enum
- **WHEN** a client attaches exact component/priority guidance documents and a bounded structural result schema without copied classification enums
- **THEN** authoring accepts that representation subject to ordinary compatibility/policy checks, performs no implicit prose-to-enum conversion and requires Reporter conditions/effects to be explicitly configured separately

#### Scenario: Project document is edited
- **WHEN** an operator publishes changed content through Project document management
- **THEN** the API returns an immutable exact reference and existing pipeline/Run selections remain unchanged until explicit selection and versioned publication

#### Scenario: A document read is denied
- **WHEN** the caller lacks current access or the selected exact content is unavailable
- **THEN** the API reports the safe reason without returning another document or treating the missing body as empty content
