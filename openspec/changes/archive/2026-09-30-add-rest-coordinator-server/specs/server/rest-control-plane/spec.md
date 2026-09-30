## Purpose

Defines the stable REST adapter through which trusted-network operators and automation manage OctaCity without coupling application commands and queries to HTTP or requiring a graphical user interface.

## ADDED Requirements

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

### Requirement: Idempotent commands and concurrency control
Every mutating REST command SHALL accept an idempotency key, return the same logical result for an exact replay, reject reuse with different content, and use an entity version or equivalent precondition where concurrent updates could conflict.

#### Scenario: Lost command response is retried
- **WHEN** a client repeats an identical command with the same idempotency key after losing the response
- **THEN** the server returns the original result without applying the mutation twice

#### Scenario: Concurrent update is stale
- **WHEN** a client updates a resource using an obsolete version precondition
- **THEN** the server returns a conflict and preserves the newer state

### Requirement: Bounded collection and event reads
Collection endpoints SHALL provide deterministic cursor pagination. Job-event reads SHALL support a bounded wait after a caller-supplied sequence cursor so command-line clients can follow live execution without WebSocket state.

#### Scenario: Follow job events
- **WHEN** a client requests events after the last observed sequence with a bounded wait
- **THEN** the server returns the next contiguous page when available or an empty page with a current cursor when the wait expires

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
