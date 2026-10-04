## ADDED Requirements

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
