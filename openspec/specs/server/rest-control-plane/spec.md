# REST Control Plane Specification

## Purpose

Defines the stable REST adapter through which trusted-network operators and automation manage OctaCity without coupling application commands and queries to HTTP or requiring a graphical user interface.

## Requirements

### Requirement: Versioned machine-readable REST interface
The server SHALL expose its initial management interface below `/api/v1`, use bounded JSON request and response bodies, return stable machine-readable error codes, and publish an OpenAPI document matching the running implementation.

#### Scenario: Client discovers the contract
- **WHEN** a client requests the OpenAPI document
- **THEN** the server returns the operations, schemas, deployment security mode, and error responses implemented by that release

#### Scenario: Unsupported input is rejected
- **WHEN** a request has an unsupported media type, unknown command field, oversized body, or unsupported API version
- **THEN** the server rejects it without changing durable state and returns a stable error code

### Requirement: Unauthenticated trusted-network management
The first release SHALL accept management requests without operator authentication, authorization scopes, or role evaluation. The management listener SHALL expose this security mode explicitly through configuration and operational metadata and SHALL remain separate from authenticated agent and webhook routes.

#### Scenario: Management request reaches the trusted listener
- **WHEN** a syntactically valid management command reaches the configured trusted-network listener
- **THEN** the server evaluates the command without requiring an operator credential and audits it as an unauthenticated management actor

### Requirement: Transport-independent management authorization
Every management command and query SHALL carry a bounded normalized management actor and SHALL be authorized against a stable action and resource description before its application handler can observe the request. Authorization SHALL remain outside REST DTOs and infrastructure adapters. The trusted-network release SHALL use an explicit policy that authorizes its unauthenticated management actor for every currently registered management action without claiming an authenticated identity.

#### Scenario: Trusted-network request is authorized
- **WHEN** a valid request reaches the trusted-network management listener
- **THEN** the server normalizes an unauthenticated management actor, authorizes the action and resource through the management policy, and dispatches the existing application operation

#### Scenario: Another management transport invokes the same operation
- **WHEN** a future non-REST management adapter submits the same typed command or query with an equivalent actor context
- **THEN** it crosses the same authorization decision instead of duplicating policy in the transport adapter

### Requirement: Authorization denial is side-effect free
An authorization denial SHALL return a stable forbidden outcome before invoking the application operation, opening an authoritative mutation transaction, resolving a private transfer capability, or exposing whether a protected resource exists. The denial response SHALL retain the safe request correlation identity and SHALL NOT expose policy internals, credentials, actor attributes, or protected resource metadata.

#### Scenario: Management action is denied
- **WHEN** the configured management policy denies an actor's requested action and resource
- **THEN** the server returns the stable forbidden response and no command, query, audit mutation fact, outbox entry, or idempotency outcome is created by the protected operation

### Requirement: Idempotent commands and concurrency control
Every mutating REST command SHALL accept an idempotency key, return the same logical result for an exact replay by the same management security scope, reject reuse with different content, prevent one actor scope from receiving another actor scope's replayed outcome, and use an entity version or equivalent precondition where concurrent updates could conflict.

#### Scenario: Lost command response is retried
- **WHEN** a client repeats an identical command with the same idempotency key and management security scope after losing the response
- **THEN** the server returns the original result without applying the mutation twice

#### Scenario: Idempotency key is reused across actor scopes
- **WHEN** another management actor scope submits a command using the same caller-selected idempotency key
- **THEN** the server does not disclose or replay the first scope's stored outcome and evaluates the second request in its own idempotency scope

#### Scenario: Concurrent update is stale
- **WHEN** a client updates a resource using an obsolete version precondition
- **THEN** the server returns a conflict and preserves the newer state

### Requirement: Bounded collection and event reads
Collection endpoints SHALL provide deterministic cursor pagination. Job-event reads SHALL support a bounded wait after a caller-supplied sequence cursor so command-line clients can follow live execution without WebSocket state.

#### Scenario: Follow job events
- **WHEN** a client requests events after the last observed sequence with a bounded wait
- **THEN** the server returns the next contiguous page when available or an empty page with a current cursor when the wait expires

### Requirement: Authorization-safe collection reads
Collection, search, and event reads SHALL apply an authorization-derived visibility scope before deterministic ordering, pagination, counts, freshness calculation, snippets, or cursor creation. The server SHALL NOT fetch an unrestricted page and remove unauthorized rows afterward.

#### Scenario: Actor can observe only part of a collection
- **WHEN** a management policy restricts an actor to a subset of otherwise matching resources
- **THEN** every returned item, page boundary, cursor, count, and freshness value is derived only from that authorized subset

#### Scenario: Actor has no visible resources
- **WHEN** an actor is authorized to perform the collection action but its visibility scope is empty
- **THEN** the server returns a valid empty page without disclosing whether hidden resources exist

### Requirement: Inter-Build Trigger management
The management REST API SHALL expose typed commands and queries to create, inspect, list, and publish replacement versions of source-scoped internal Build-completion Triggers. Requests SHALL identify exact upstream and downstream Build Configuration versions, one terminal outcome, an explicit downstream source strategy, bounded constant parameters, priority, and enabled state. The API SHALL NOT require callers to submit an untyped provider payload or use a time-based schedule to represent an inter-Build dependency.

#### Scenario: Operator creates a success dependency
- **WHEN** a trusted-network client creates an enabled internal Trigger from Build Configuration A success to Build Configuration B with a valid source strategy
- **THEN** the API returns a versioned logical Trigger resource whose exact replay is idempotent

#### Scenario: Operator selects incompatible revision inheritance
- **WHEN** the source and target configurations do not use a compatible Repository identity
- **THEN** the API rejects inherited-revision selection with a stable validation error and creates no Trigger version

#### Scenario: Operator disables a dependency
- **WHEN** a client publishes a disabled replacement version using the current version precondition
- **THEN** later upstream terminal events do not create downstream Builds while already accepted occurrences remain immutable

### Requirement: Bounded build-log search
The REST API SHALL provide backend-neutral search over redacted committed build logs. A request SHALL use a bounded UTF-8 query, select full-text terms or literal-fragment mode, and MAY filter by project, Build, Attempt, Job, stream, and time range. Responses SHALL use deterministic cursor pagination and return bounded context snippets, logical chunk and event-sequence references, and index freshness without exposing object-store locations or database-specific query syntax. Unbounded regular-expression search SHALL NOT be part of the initial API.

#### Scenario: Client searches for an error across a Build
- **WHEN** a client submits a bounded full-text query scoped to one Build
- **THEN** the server returns matching redacted snippets with Job, stream and sequence context in a deterministic page

#### Scenario: Client searches for a path or error code
- **WHEN** a client submits a bounded literal-fragment query scoped to a project or Build
- **THEN** the server can match the exact fragment without requiring the client to understand PostgreSQL tokenization

#### Scenario: Search projection is behind durable logs
- **WHEN** committed log chunks have not yet been indexed
- **THEN** the response exposes contiguous indexed-through and authoritative committed-through positions so an empty result cannot be mistaken for a fully caught-up search

### Requirement: Build Result retention management
The management REST API SHALL expose the recorded automatic-retention deadlines and active hold state for a Build Result and SHALL provide idempotent commands to place and release a permanent or time-bounded Build Result hold. A place command SHALL require a bounded reason and MAY specify an expiry. Hold responses SHALL expose logical state and available audit identity without storage-provider details or credentials.

#### Scenario: Automation pins a Build Result
- **WHEN** a trusted-network client submits a valid hold command with an idempotency key before retention begins
- **THEN** the API returns the active hold and an exact replay returns the same logical result

#### Scenario: Automation unpins a Build Result
- **WHEN** a trusted-network client releases the active hold with the required concurrency precondition
- **THEN** the API returns the resulting retention state and does not reset any automatic-retention deadline

#### Scenario: Automation pins a result after deletion begins
- **WHEN** a trusted-network client attempts to place a hold after the Build Result has become retention-invisible
- **THEN** the API returns a stable conflict and does not claim that deleted or partially deleted data is protected

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

### Requirement: Expandable Project navigation summaries
The management REST API SHALL report on each Project collection summary whether the Project has at least one visible direct child. Child existence SHALL be derived with the same authorization visibility as the returned collection and SHALL NOT require a client to probe every Project independently. The existing Project detail representation SHALL remain compatible.

#### Scenario: Visible Project has a visible direct child
- **WHEN** a client lists a Project whose direct child is visible to the caller
- **THEN** that Project summary reports that it can be expanded

#### Scenario: Project has no visible direct child
- **WHEN** a Project has no direct children or all of its direct children are hidden from the caller
- **THEN** that Project summary reports that it cannot be expanded without revealing hidden descendants

### Requirement: Project-owned definition discovery
The management REST API SHALL expose authorization-safe, project-scoped, cursor-paginated collections of the current Pipeline, Repository, Build Configuration, and Trigger definition versions. Collection items SHALL contain the stable identity, owning Project identity, current version, operator-facing name or kind, relevant enabled state, and publication time needed to select the exact version through an existing detail endpoint. Trigger discovery SHALL distinguish manual, scheduled, and internal definitions without exposing provider credentials or webhook secrets.

#### Scenario: Client discovers current Project definitions
- **WHEN** a client lists one supported definition collection for a visible Project
- **THEN** the server returns a bounded deterministic page of current visible summaries and an exclusive continuation cursor when another page exists

#### Scenario: Definition has several immutable versions
- **WHEN** a Project-owned definition has multiple published versions
- **THEN** its discovery collection contains only the current version while the existing version-addressed endpoint remains available for exact historical reads

#### Scenario: Project has no definitions of one kind
- **WHEN** the requested Project is visible but has no visible current definitions in that collection
- **THEN** the server returns a valid empty page without requiring a known definition identity

### Requirement: Project Build discovery
The management REST API SHALL expose an authorization-safe, project-scoped, cursor-paginated Build collection ordered deterministically from newest to oldest. A client SHALL be able to filter the collection by exact Build Configuration identity and Build state. Each summary SHALL identify the Build, owning Project, selected Build Configuration and version, creation cause and time, current Attempt identity, number and state, and terminal time or outcome when available without embedding the complete Attempt DAG or event history.

#### Scenario: Client lists recent Project Builds
- **WHEN** a client requests the first bounded Build page for a visible Project
- **THEN** the server returns the newest visible summaries in deterministic order and a cursor that continues strictly after the last returned Build

#### Scenario: Client filters by configuration and state
- **WHEN** a client supplies a valid Build Configuration identity and Build state belonging to the selected Project
- **THEN** every returned summary matches both filters and pagination is computed over only the matching visible Builds

#### Scenario: Hidden Builds surround a page boundary
- **WHEN** non-visible Builds sort before, within, or after the requested page
- **THEN** returned items, ordering, page size, and continuation cursor are derived only from visible matching Builds

#### Scenario: Existing client reads one Build
- **WHEN** a client continues to use the existing Build detail endpoint
- **THEN** its request and representation remain compatible and do not require use of the discovery collection

### Requirement: Agent capacity discovery
The management REST API SHALL expose authorization-safe, cursor-paginated Agent and Agent Pool collections for the capacity explorer. The Agent collection SHALL accept an optional exact Agent Pool identity and SHALL apply that filter before ordering, pagination, and cursor production. Because every enrolled Agent belongs to exactly one current Agent Pool, the API SHALL NOT invent an unassigned-Agent state. The API SHALL expose the current Agent Pool representation by stable Pool identity in addition to the existing immutable version-addressed read. An Agent detail representation SHALL include an optional bounded, non-secret projection of its current execution sufficient to identify the active Build, Attempt, Job, and lease state without exposing fencing material or requiring the console to reconstruct coordinator state. These reads SHALL publish capacity and execution facts, not a client-facing scheduling-compatibility decision.

#### Scenario: Client expands one Agent Pool
- **WHEN** a client lists Agents with a visible Agent Pool identity
- **THEN** the server returns only visible Agents currently assigned to that Pool in deterministic cursor order

#### Scenario: Client opens the current Agent Pool
- **WHEN** a client requests a visible Agent Pool by stable identity without a version
- **THEN** the server returns its current immutable version while the existing version-addressed endpoint remains available for exact historical reads

#### Scenario: Agent has a current execution
- **WHEN** a visible Agent owns a current non-terminal lease
- **THEN** its detail representation contains the safe Build, Attempt, Job, and lease-state projection and no credential, bearer, or fencing token

#### Scenario: Agent is idle
- **WHEN** a visible Agent has no current non-terminal lease
- **THEN** its detail representation reports no current execution rather than fabricating an unassigned or compatible-capacity state

#### Scenario: Capacity cannot satisfy a workload
- **WHEN** published Agent or Pool capacity facts differ from a workload's requirements
- **THEN** the API returns the authoritative facts without publishing a derived scheduling decision for the console to reproduce

### Requirement: Bounded operator resource search
The management REST API SHALL expose an authorization-safe, cursor-paginated resource search for Projects, Builds, Agents, and Agent Pools. Search SHALL accept a bounded normalized query, an optional bounded set of resource kinds, a bounded page size, and an exclusive cursor bound to the normalized query and kinds. Results SHALL be typed and SHALL contain only the stable identity, safe operator-facing label, resource kind, and bounded non-secret context needed to distinguish and open the corresponding existing detail resource. Matching SHALL cover stable identifiers and the operator-facing Project, Build Configuration, Agent, and Agent Pool names available to the search projection. Exact identifier matches SHALL rank before normalized name-prefix matches and other normalized name matches, with resource kind, normalized label, and stable identity providing deterministic tie ordering. Authorization visibility SHALL be applied before ranking, pagination, and cursor production.

#### Scenario: Client searches across resource kinds
- **WHEN** a client submits a valid query without restricting resource kinds
- **THEN** the server returns one bounded page of typed visible Project, Build, Agent, and Agent Pool matches in deterministic rank order and an exclusive continuation cursor when more matches exist

#### Scenario: Client restricts search kinds
- **WHEN** a client searches only Builds or only Agents and Agent Pools
- **THEN** every result belongs to one requested kind and pagination is computed only over visible matching resources of those kinds

#### Scenario: Hidden matches surround a search page boundary
- **WHEN** non-visible resources would otherwise rank before, within, or after the requested search page
- **THEN** returned results, ordering, page size, and continuation cursor are derived only from visible matches

#### Scenario: Search cursor belongs to another query
- **WHEN** a client reuses a cursor with different normalized query text or resource kinds
- **THEN** the server rejects the cursor as invalid rather than skipping or disclosing results from the previous search

#### Scenario: Search query is absent or outside bounds
- **WHEN** a client submits an empty query or a query outside the declared normalized length bound
- **THEN** the server rejects it with the stable validation response and does not perform an unbounded collection scan

### Requirement: Bounded operator attention feed
The management REST API SHALL expose an authorization-safe, newest-first, cursor-paginated attention feed for attention-worthy Build, Agent, and Agent Pool transitions selected by a bounded target-identity set and for server-classified critical system conditions explicitly requested by the client. Each item SHALL have a stable attention identity, typed category, severity, safe code and summary, occurrence time, optional resolution time, and optional visible resource target sufficient to open an existing detail endpoint. The feed SHALL NOT model a recipient, personal unread state, arbitrary audit history, raw Job events, or ordinary unrelated successes. Target selection SHALL NOT grant visibility, and authorization SHALL be applied before classification, ordering, pagination, and cursor production. Cursors SHALL bind the target set, critical-condition selection, and time bounds.

#### Scenario: Client requests attention for favorite resources
- **WHEN** a client submits a bounded set of Build, Agent, and Agent Pool identities from its local favorites
- **THEN** the server returns only visible attention-worthy transitions for those targets in deterministic newest-first order without treating the submitted identities as authorization

#### Scenario: Client requests critical system conditions
- **WHEN** a client explicitly includes critical system conditions
- **THEN** the server returns visible active or recently changed critical conditions with their server-classified severity and resolution state

#### Scenario: Server readiness changes
- **WHEN** the readiness monitor observes a required dependency become unavailable or time out and later recover
- **THEN** the server durably opens one idempotent safe critical condition for that runtime source and classified failure and resolves only that source's condition after recovery

#### Scenario: Client supplies no attention scope
- **WHEN** a client supplies neither target identities nor critical-condition selection
- **THEN** the server rejects the request instead of returning an unbounded general activity stream

#### Scenario: Hidden target appears in the requested set
- **WHEN** the requested target set contains a resource the caller cannot observe
- **THEN** no item or cursor reveals whether that target exists and visible items retain correct page bounds

#### Scenario: Client reuses a cursor with another scope
- **WHEN** a client changes the target set, critical-condition selection, or time bounds while reusing a cursor
- **THEN** the server rejects the cursor as invalid rather than continuing the previous attention stream

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

### Requirement: Factory source inputs have typed versioned management
The management REST API SHALL expose authorization-filtered discovery, inspection, validation, creation and immutable replacement of Project-owned Work Source input bindings and exclusive input groups. Representations SHALL expose safe Project-authorized connection/resource and source/reporter capability choices, installed repository choices, exact target Factory Configuration and Repository references, bounded Issue state/label predicates, input mappings, status mappings and enabled state without provider tokens or raw provider request payloads. Mutations SHALL require the existing idempotency and current-version preconditions. Validation SHALL return normalized input selection, safe incompatibilities and target/version details before activation.

#### Scenario: Client configures an approved defect input
- **WHEN** an authorized client selects a visible installed GitHub source, a visible target configuration, and the required `approved` and `bug` labels
- **THEN** validation and publication retain a typed immutable input binding under that Project and exact mutation replay returns the same version

#### Scenario: Client references a hidden source or target
- **WHEN** an input names a repository, source capability, target configuration or reporting grant unavailable to its Project
- **THEN** the API returns the safe authorization or validation outcome without revealing hidden resources or activating the input

### Requirement: Source previews and diagnostics distinguish discovery from execution
The management REST API SHALL expose bounded read-only selector previews and safe input/reporting diagnostics including effective selector, exact versions, last scan/progress, next retry, candidate dispositions and visible admitted Work/Run correlations. A preview SHALL use installed read authority, respect provider bounds and SHALL NOT reserve identities, advance production cursors, admit Work or write to GitHub. It SHALL NOT create/edit an internal Issue, attach a source or commit a refresh observation to task state. Diagnostics SHALL distinguish filtered, ambiguous, replayed, newly admitted, unavailable and projection-pending/exhausted states without claiming preview results or external labels are authoritative execution outcomes. Raw Issue bodies and provider secrets SHALL be excluded from preview and diagnostic summaries.

#### Scenario: Client previews an unrestricted label selector
- **WHEN** a client previews a visible bounded `open` Issue source without required labels
- **THEN** the API returns safe labeled and unlabeled candidate summaries and selector reasons without creating Runs or modifying scan progress

#### Scenario: GitHub returns a rate limit
- **WHEN** preview or discovery is deferred by the provider
- **THEN** diagnostics expose a safe scoped retry time instead of reporting no matching Issues or global server failure

#### Scenario: Preview runs with an internal-Issue consumer enabled
- **WHEN** an operator previews candidates also known to the task-card capability
- **THEN** preview creates no Issue/revision/association or synchronization intent and does not consume that capability's production refresh progress

### Requirement: Source diagnostics preserve native and optional internal task correlation
Source/reporting REST SHALL expose safe native task correlation separately from input-scoped Work/Run identity. An internal Issue reference SHALL be optional, supplied only by an enabled authoritative association capability and filtered by its current visibility. Missing or backfilling cards SHALL NOT block existing diagnostics or cause read-time task creation, guessed text-based association or hidden-task disclosure. Integration routes SHALL remain responsible for connections/grants, automatic inputs and supported stage effects; internal Issue creation/editing/execution and field-ownership/conflict-resolution commands SHALL use the task capability's own contracts. Manual task creation when that capability is enabled SHALL NOT require a connection or provider check.

#### Scenario: One native task has two admitted pipeline inputs
- **WHEN** an authorized client inspects both visible admissions
- **THEN** it sees common native correlation and distinct Work/Run references, plus the same internal Issue link only when the enabled association is visible

#### Scenario: Internal task association is hidden or unavailable
- **WHEN** the source remains visible but no permitted card association can be returned
- **THEN** diagnostics retain native and existing execution references without revealing the hidden task or fabricating an Issue

### Requirement: Stage source-update actions have typed configuration and provenance
The management REST API SHALL support configuring and validating source label-update effects for individual declared Flow stage outcomes through existing versioned Flow/action publication. Clients SHALL be able to select the original source binding, visible Reporter mapping and explicit labels to add/remove without entering provider wire payloads or credentials. Publication SHALL freeze the resolved mapping and reject conflicting or unauthorized effects. Diagnostics SHALL correlate each queued/applied/failed projection to its originating stage, outcome and action identity, including intermediate stages before the Run is terminal.

#### Scenario: Client configures label changes after an intermediate stage
- **WHEN** an authorized client configures a declared stage outcome to add and remove selected permitted labels
- **THEN** validation presents those exact source effects and publication retains them in the immutable Flow/action configuration independently of other stage outcomes

### Requirement: Reusable provider connections have versioned administration
The management API SHALL support authorized creation, inspection, validation, immutable replacement, disablement and dependency inspection of reusable named provider connections. Public metadata SHALL include stable connection identity, display name, installed provider/adapter reference, validated service endpoint, supported authentication/capability choices, safe resource references and configuration/readiness state. A connection SHALL NOT select a pipeline target or implicitly grant a Project access. Management SHALL reuse verified installed adapters and SHALL NOT accept executable uploads or arbitrary filesystem/credential paths. Provider identities and resource envelopes SHALL remain open to other installed providers without introducing native provider types into Factory execution. Mutations SHALL use current-version preconditions, idempotency and safe audit receipts.

#### Scenario: Connection metadata is published
- **WHEN** an authorized administrator creates a connection using a compatible installed provider and validated settings
- **THEN** publication returns a stable versioned identity and safe metadata without secrets, activated inputs or implicit Project grants

#### Scenario: Another installed provider is selected
- **WHEN** a compatible non-GitHub adapter advertises supported connection capabilities
- **THEN** the same administrative envelope accepts its opaque provider/resource identities while exposing only its actual installed capabilities

#### Scenario: Endpoint attempts to expand network access
- **WHEN** a proposed endpoint, redirect or native resource exceeds the trusted installation network/resource ceiling
- **THEN** validation rejects it before accessing credentials or making an unauthorized request

### Requirement: Credential setup and rotation are protected write-only operations
A separately authorized integration credential operation SHALL accept a bounded transient secret value or a permitted existing server-side handle, store it in protected server-side credential storage and return only a safe receipt/reference and configured status. It SHALL support replacement/rotation without read-back of existing values. Raw secret values SHALL NOT enter public metadata, ordinary database configuration/history, Work, idempotency request-body storage, audit, logs, URLs, errors or API responses. Secret submission SHALL use authenticated protected transport and request-body redaction. Private material publication and connection activation SHALL be recoverable under one stable administrative operation identity: partial preparation SHALL NOT expose an active unusable connection, and retries SHALL NOT create duplicate active versions or restore an earlier credential after a later rotation/revocation. Unreferenced failed preparations SHALL have bounded cleanup. Existing operator-installed handles SHALL remain usable without exporting their values.

#### Scenario: Secret submission succeeds or fails
- **WHEN** an administrator submits a valid or invalid token through the credential operation
- **THEN** both success and failure expose only safe classified outcomes, never the submitted value or private storage path

#### Scenario: Publication stops after private preparation
- **WHEN** a process stops between credential preparation and connection activation
- **THEN** the connection remains inactive, and retry resumes the same operation or bounded cleanup removes its unreferenced private preparation

#### Scenario: An old rotation request is replayed
- **WHEN** a previously completed rotation is replayed after a newer rotation or revocation
- **THEN** the API returns its original safe receipt without restoring old material or reauthorizing revoked access

### Requirement: Project access constrains connection resources and purposes
The management API SHALL support explicit versioned Project access to selected connection resources and purposes, distinguishing task reads, task updates, repository reads and Delivery. Publication SHALL require authority to manage the connection grant and the target OctaCity Project. Integration administration, credential management and pipeline use SHALL be separately authorized. Enumeration, validation, input publication and each protected runtime operation SHALL enforce current Project access and installed ceilings before exposing protected material or hidden resources. Remote token permissions SHALL NOT widen a Project grant, and ordinary pipeline publication SHALL NOT modify it.

#### Scenario: Pipeline editor has read-only task access
- **WHEN** a Project is granted task reading but not task updates or Delivery
- **THEN** its editor can select a compatible read input but cannot enable reporting/publication even if the installed token has broader remote permissions

#### Scenario: Caller references an ungranted connection
- **WHEN** a client lists resources or submits an input for a connection unavailable to its Project
- **THEN** the API denies the operation without enumerating hidden provider resources or revealing credentials

### Requirement: Connection checks are bounded and read-only
The management API SHALL expose a separately authorized bounded connection check that verifies installed configuration, authentication, permitted resource identity/access and supported capability readiness through the verified provider. It SHALL distinguish missing configuration/credential, denied authentication/access, unsupported or unverified capabilities, throttling and unavailability. It SHALL NOT mutate provider resources, grant authority, start workers, admit Work or advance production cursors. An enabled internal-Issue consumer SHALL NOT turn a check into task creation, revision/association publication or synchronization progress. Write readiness that cannot be verified without mutation SHALL remain explicitly unverified rather than be asserted or tested with a write.

#### Scenario: Reading is ready but writing is unverified
- **WHEN** a read-only connection check confirms resource access but cannot prove task-update authority
- **THEN** it reports those results separately, makes no label/state/branch/review mutation and does not grant write access

#### Scenario: Provider is unavailable during a check
- **WHEN** a permitted connection check is throttled or receives a transient failure
- **THEN** the API returns a scoped safe deferred result without reporting an empty resource list or changing pipeline progress

### Requirement: Connection lifecycle preserves accepted work and current revocation
Connection/resource replacement SHALL retain referenced immutable versions and SHALL NOT silently redirect existing inputs or pending reporting to another endpoint, adapter or native resource. Credential rotation within the same logical scope SHALL preserve connection/input/admission identities. Disablement and Project-access revocation SHALL block subsequent protected operations, including retries of retained intents, without rewriting accepted Work/Run/Flow or falling back to other credentials. Retirement SHALL expose authorized dependency information, prevent destructive removal of referenced configuration/material, and retain safe history. Adoption of existing installed connections SHALL preserve original identities and SHALL NOT activate inputs or widen grants automatically.

#### Scenario: Connection changes while an old projection is pending
- **WHEN** an administrator publishes a replacement endpoint or native resource
- **THEN** the old projection retains its exact scope and either uses its still-authorized retained version or reports a bounded failure, never writes to the replacement resource

#### Scenario: Access is revoked before retry
- **WHEN** a Project's task-update grant is revoked after an action was accepted
- **THEN** retry cannot make a new provider write, and the original accepted result and finite reporting history remain retained

#### Scenario: Referenced connection is retired
- **WHEN** an administrator attempts to remove a connection used by inputs or pending reporting
- **THEN** the API shows only authorized dependencies and requires a safe disable/retention path rather than deleting referenced configuration or secrets
