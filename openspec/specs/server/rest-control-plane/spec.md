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
Every operation required to manage project hierarchies, pipelines, build configurations, triggers, builds, attempts, jobs, agent pools, agents, outputs, repository integrations, follow and search build logs, diagnose execution, drain capacity, and maintain the initial server SHALL be available through REST; no workflow SHALL require a Web UI.

#### Scenario: Headless operation
- **WHEN** an operator uses only an HTTP client and the published OpenAPI contract
- **THEN** the operator can complete the documented server and agent lifecycle

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
