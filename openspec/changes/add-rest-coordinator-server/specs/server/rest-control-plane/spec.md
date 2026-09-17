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

### Requirement: REST-only initial product surface
Every operation required to manage project hierarchies, pipelines, build configurations, triggers, builds, attempts, jobs, agent pools, agents, outputs, repository integrations, follow and search build logs, diagnose execution, drain capacity, and maintain the initial server SHALL be available through REST; no workflow SHALL require a Web UI.

#### Scenario: Headless operation
- **WHEN** an operator uses only an HTTP client and the published OpenAPI contract
- **THEN** the operator can complete the documented server and agent lifecycle
