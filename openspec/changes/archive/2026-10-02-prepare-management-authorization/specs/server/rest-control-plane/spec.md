## ADDED Requirements

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

### Requirement: Authorization-safe collection reads
Collection, search, and event reads SHALL apply an authorization-derived visibility scope before deterministic ordering, pagination, counts, freshness calculation, snippets, or cursor creation. The server SHALL NOT fetch an unrestricted page and remove unauthorized rows afterward.

#### Scenario: Actor can observe only part of a collection
- **WHEN** a management policy restricts an actor to a subset of otherwise matching resources
- **THEN** every returned item, page boundary, cursor, count, and freshness value is derived only from that authorized subset

#### Scenario: Actor has no visible resources
- **WHEN** an actor is authorized to perform the collection action but its visibility scope is empty
- **THEN** the server returns a valid empty page without disclosing whether hidden resources exist

## MODIFIED Requirements

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
