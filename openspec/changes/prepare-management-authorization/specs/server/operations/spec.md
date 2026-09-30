## MODIFIED Requirements

### Requirement: Audited mutations
Every accepted management, agent, trigger, adapter, and worker mutation SHALL append an immutable audit fact in the same authoritative transaction as the state change. Records SHALL identify the actor kind and available verified identity supplied by the accepted application context, operation, target, request or idempotency identity, timestamp, and outcome without recording sensitive fields. An authoritative store adapter SHALL NOT select, replace, or invent the management actor for a mutation.

#### Scenario: Unauthenticated operator cancels a build
- **WHEN** a trusted-network management request cancels a build
- **THEN** the audit record identifies the supplied unauthenticated management actor, request identity, operation and build without inventing an authenticated principal

#### Scenario: Authenticated management actor is introduced later
- **WHEN** a future authentication adapter supplies a verified management identity and the authorized mutation commits
- **THEN** the audit fact records that identity without requiring the store adapter or domain mutation to understand the authentication mechanism, role model, or policy implementation

#### Scenario: Store input omits its accepted actor
- **WHEN** an authoritative management mutation reaches the store without the validated actor context selected by the application layer
- **THEN** the store rejects the operation instead of defaulting it to an unauthenticated or system actor
