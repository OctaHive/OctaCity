## ADDED Requirements

### Requirement: REST Work-card and exact history discovery
The management API SHALL expose an admitted Work by stable identity with its owning Project, safe immutable Work summary, existing Run identity/current version, exact subject/candidate/Decision references, independently authorized input metadata, and bounded links to execution history and checks. Child reads SHALL distinguish nested Flow Runs, cycles, node retries, ordinary Builds and attempts, exact results, and retained-unavailable facts without returning full aggregate snapshots or protected payloads. Existing Run URLs and APIs SHALL remain compatible.

#### Scenario: HTTP-only client inspects a Work
- **WHEN** a client opens a visible Work identity returned by ordinary admission or existing Run discovery
- **THEN** it can navigate its safe history, checks, discussion, and current action references through published REST without an opaque-identifier search or the Console

#### Scenario: A client requests current verification results
- **WHEN** evidence is missing, superseded, or incomplete for the current candidate
- **THEN** the API returns exact safe state and subject references without deriving overall success from a finished Build

### Requirement: REST operator-only comment commands and reads
The API SHALL expose bounded comment listing and scoped idempotent append commands for one visible Work. Append SHALL accept only bounded plain text and an optional same-Work root parent, resolve actor provenance from management context, and return safe publication metadata plus authorized discussion content through the dedicated discussion boundary. Exact replay SHALL preserve the original logical outcome and changed reuse SHALL conflict. Commands SHALL reject lifecycle-state, execution, Context-update, approval, or authenticated-author fields; body text that discusses such actions SHALL remain non-authoritative discussion. Audit and generic card/queue responses SHALL exclude comment bodies.

#### Scenario: Comment append is denied
- **WHEN** the actor lacks the required Work/Project discussion permission or cannot observe the Work
- **THEN** denial occurs before comment storage, replay outcome, sequence allocation, or audit mutation and discloses no protected Work

#### Scenario: Another security scope reuses a key
- **WHEN** a different management security scope submits an already-used idempotency key
- **THEN** it cannot obtain the first scope's comment or replay outcome through that key

### Requirement: REST actionable Factory decision discovery
The API SHALL expose bounded Project/global operator-action collections and a Work-scoped view with typed exact targets, permitted command kind, admitted response schema or disposition contract, current strong Run precondition, deadlines, and safe subject/reason references. Items SHALL not act as authorization tokens. Resolution SHALL use existing response, bound-operation, and retry endpoints with `Idempotency-Key` and `If-Match`; no independent close-item or approve-Work setter SHALL exist. A client SHALL be able to finish the workflow entirely through REST.

#### Scenario: Client resolves a declared clarification
- **WHEN** a client submits the item's declared typed value to its exact response endpoint using the observed Run precondition
- **THEN** the existing control authority records one intent and returns its ordinary receipt; queue/card observation distinguishes that receipt from later owner completion

#### Scenario: Generic response targets a bound operator request
- **WHEN** a client attempts to answer a delivery/promotion/escalation binding with a generic response or unsupported disposition
- **THEN** the existing contract rejects the attempt and no queue-specific bypass is accepted

### Requirement: Collaboration routes share security, pagination, and OpenAPI contracts
Every new Work, discussion, history/check, timeline, and queue operation SHALL be registered in the existing management authorization/action/resource and OpenAPI inventories, use bounded closed schemas and safe errors/correlation, and enforce current visibility before response projection and pagination. Cursor reuse across Work, queue scope, filters, or observation contract SHALL be rejected. Actor attribution SHALL reflect the actual trusted-network release and SHALL not claim authenticated personal identities. Source text SHALL not appear in operational logs, URL parameters, cursor payloads, or generic error bodies.

#### Scenario: Hidden targets surround a page
- **WHEN** a collection request includes hidden or no-longer-visible Work
- **THEN** its page, aggregate metadata, and cursor are computed only from authorized matching data

#### Scenario: Cursor belongs to another view
- **WHEN** a client reuses a Work discussion/history or decision-queue cursor for another scope or filter
- **THEN** the API returns a stable invalid-cursor response without continuing or disclosing the original stream

#### Scenario: A legacy client controls a Run
- **WHEN** it uses existing Factory command bodies and headers without collaboration metadata
- **THEN** authorization, replay, eligibility, concurrency, and receipt semantics remain compatible
