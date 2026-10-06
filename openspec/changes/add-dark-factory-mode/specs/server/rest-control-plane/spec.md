## MODIFIED Requirements

### Requirement: REST-only initial product surface
Every operation required to manage project hierarchies, pipelines, build configurations, triggers, builds, attempts, jobs, agent pools, agents, outputs, repository integrations, Factory Configurations, Work admission, Factory Runs, stage attempts, candidates, evidence, evaluations, decisions, escalations, delivery-for-review, follow and search build logs, diagnose execution, drain capacity, and maintain the server SHALL be available through REST; no CI/CD or Dark Factory workflow SHALL require a Web UI.

#### Scenario: Headless operation
- **WHEN** an operator uses only an HTTP client and the published OpenAPI contract
- **THEN** the operator can complete the documented server and agent lifecycle

#### Scenario: Headless CI/CD operation
- **WHEN** an operator uses only an HTTP client and the published OpenAPI contract without configuring Factory mode
- **THEN** the operator can complete the documented server, Agent, and ordinary Build lifecycle unchanged

#### Scenario: Headless Factory operation
- **WHEN** an operator uses only an HTTP client and the published OpenAPI contract for an enabled Factory Configuration
- **THEN** the operator can admit Work, inspect and control its Factory Run, resolve escalation, request delivery for review, and correlate every action to Builds and audit evidence

## ADDED Requirements

### Requirement: Versioned Factory Configuration management
The management REST API SHALL expose typed authorization-safe commands and queries to create, inspect, list, validate, and publish replacement Factory Configuration versions owned by a Project. It SHALL expose bounded authorization-filtered choice collections for selectable Build Configurations, execution policies and targets, permission profiles and semantic enforcement capabilities, Decision Signal adapters, exact models, question kinds and profile policies, criterion packs, evaluator connectors, secret and workload-identity profile references, delivery adapters, and other logical references needed by a management client. Requests SHALL use bounded structured admission, stage, Build Configuration, budget, WIP, permission, optional Decision Signal routing and tool-risk rollout, evaluation, rework, and delivery policy; SHALL reject unknown provider payloads; SHALL require idempotency and current-version preconditions for mutations; and SHALL expose logical adapter and policy identities without credentials.

#### Scenario: Operator creates a Factory Configuration
- **WHEN** an authorized client submits a valid configuration using visible Project, Build Configuration, criterion-pack, connector, and policy identities
- **THEN** the API returns one immutable version and an exact replay returns the same logical result

#### Scenario: Management client discovers configuration choices
- **WHEN** an authorized client prepares a Factory Configuration for a visible Project
- **THEN** bounded choice collections contain only references selectable under current Project and deployment policy plus safe capability and compatibility metadata, without credentials, hidden resources, or manually constructed provider payloads

#### Scenario: Client validates before publication
- **WHEN** a client submits a bounded configuration candidate for validation without publishing it
- **THEN** the API returns normalized field-level errors, effective permission narrowing, required Agent and backend capabilities, unavailable references, and safe warnings without allocating a configuration version or accepting a lifecycle mutation

#### Scenario: Configuration contains unavailable authority
- **WHEN** a request names a permission, secret profile, execution target, connector, or delivery operation unavailable under the Project and deployment policy
- **THEN** the API returns a stable validation or conflict response and publishes no usable configuration version

#### Scenario: Client selects a Decision Signal provider
- **WHEN** a client configures JEV or another compatible decision-model adapter for routing or tool risk
- **THEN** it selects a server-published exact adapter/model/profile choice with supported capabilities, purpose-specific rollout mode, thresholds, fallback and policy version rather than posting provider credentials or an arbitrary wire payload

#### Scenario: Mutable model alias is selected
- **WHEN** a client selects a supported provider alias while preparing a configuration
- **THEN** authoritative validation resolves and displays the exact model identity that publication will freeze, or rejects publication when exact resolution is unavailable

### Requirement: Bounded Work admission commands
The management REST API SHALL accept a manual provider-neutral Work Submission containing a visible Factory Configuration, scoped external identity, Repository subject, allowed base reference, task and acceptance artifacts, optional specification references, bounded priority and risk, and bounded metadata. The command SHALL use an idempotency key and SHALL return the existing logical admission result for an exact scoped replay before resolving mutable source state again.

#### Scenario: Manual Work is admitted
- **WHEN** an authorized client submits valid Work to an enabled Factory Configuration
- **THEN** the API returns the immutable Work Envelope and Factory Run identities, exact base revision, current state, and safe request correlation without exposing source or model credentials

#### Scenario: Manual Work replay changes content
- **WHEN** a client reuses an admission idempotency identity with a different repository, task, base reference, or configuration
- **THEN** the API rejects the mismatch and neither mutates the existing Factory Run nor creates another one

### Requirement: Authorization-safe Factory Run discovery
The management REST API SHALL expose project-scoped and global authorization-safe Factory Run collections with deterministic newest-first cursor pagination and bounded filters for Factory Configuration, source kind, lifecycle state, decision, delivery state, and time. A summary SHALL expose safe Work identity, Project, configuration version, exact base and current candidate identities when available, current stage and attempt, state, budget status, latest decision, escalation and delivery summaries, and timestamps without embedding full traces, raw task content, findings, credentials, or private provider links.

#### Scenario: Client lists active Factory Runs
- **WHEN** a client requests a bounded page filtered to visible non-terminal runs
- **THEN** ordering, filters, counts, and continuation cursor are computed over only authorized matching runs

#### Scenario: Hidden runs surround a page boundary
- **WHEN** non-visible Factory Runs sort before, within, or after the requested page
- **THEN** returned items and cursor reveal nothing about hidden runs and retain deterministic page bounds

### Requirement: Factory Run diagnostics
The management REST API SHALL expose one Factory Run and bounded cursor-paginated views of its immutable Work Envelope summary, Stage Attempts, linked Builds, Task Envelope metadata, Decision Signal Requests and Receipts, ChangeSets, Evidence Manifests, Evaluation Plans, Assessments, Decisions, budget consumption, escalations, delivery attempts, reporter status, and audit correlation. Responses SHALL use logical identities and safe digest/provenance metadata and SHALL NOT expose raw hidden reasoning, unredacted prompts or command text, raw provider payloads, secrets, permanent object locations, or provider and forge credentials.

#### Scenario: Client diagnoses a rework decision
- **WHEN** a visible Factory Run has produced a rework Decision
- **THEN** the API links the exact candidate, deterministic failures, required Assessments, policy reasons, prior Stage Attempt, and next bounded action without requiring the client to infer control flow

#### Scenario: Evaluation branch is still active
- **WHEN** some required evaluator Stage Attempts are terminal and another remains active
- **THEN** the response distinguishes branch states and the pending deterministic join without claiming a final Decision

#### Scenario: Client diagnoses a signal-influenced route
- **WHEN** bounded control consumed a routing or tool-risk Decision Signal
- **THEN** the API distinguishes the non-authoritative typed signal and confidence semantics from the deterministic policy disposition and links their exact provider, model, question-set, input and policy digests

### Requirement: Idempotent Factory control commands
The management REST API SHALL expose typed idempotent commands to cancel a non-terminal Factory Run, retry an eligible infrastructure-failed Stage Attempt, acknowledge or resolve an escalation with a bounded disposition, and request delivery for review of the exact currently accepted candidate. Commands SHALL require the current Factory Run version or equivalent strong precondition and SHALL NOT permit a caller to submit an arbitrary next state, Assessment, Decision, candidate digest, or delivery success.

#### Scenario: Operator cancels a Factory Run
- **WHEN** an authorized client confirms cancellation using the current precondition
- **THEN** the server records one durable cancellation intent, propagates it through Factory-owned active Builds, and preserves completed evidence and history

#### Scenario: Operator requests stale delivery
- **WHEN** delivery is requested using a precondition from an older candidate or Decision
- **THEN** the API returns a stable conflict and does not publish repository state

#### Scenario: Operator resolves an escalation
- **WHEN** an authorized client supplies one permitted bounded escalation disposition
- **THEN** the server records the actor, reason, precondition, and chosen policy transition rather than accepting an untyped lifecycle state

### Requirement: Factory OpenAPI and authorization inventory
Every Factory command and query SHALL use the existing management actor, action, resource, authorization, visibility, request-correlation, error, idempotency, and OpenAPI drift contracts. Factory resources SHALL have stable authorization targets, and collection visibility SHALL be applied before ordering, filtering, aggregation, cursor creation, or evidence projection.

#### Scenario: Factory route is registered
- **WHEN** the server publishes its OpenAPI and authorization inventories
- **THEN** every implemented Factory operation has matching typed schemas, stable errors, action and resource mappings, and route/schema drift coverage

#### Scenario: Actor cannot observe the Factory Run
- **WHEN** policy denies visibility to a target run or its owning Project
- **THEN** detail, child collections, commands, counts, and cursors disclose neither the run nor its candidate or external identities
