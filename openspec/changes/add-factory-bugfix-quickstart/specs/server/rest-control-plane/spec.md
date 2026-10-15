## ADDED Requirements

### Requirement: REST bug-fix preset discovery and resolution
The management API SHALL expose bounded authorization-filtered Project-scoped preset discovery and a read-only typed resolution operation for the exact release-versioned bug-fix preset and selected visible dependencies. Responses SHALL contain safe display metadata, exact preset identity, required parameter choices, normalized Flow and Configuration drafts, and authoritative validation diagnostics. Resolution SHALL allocate no published version, Work, object, or execution. Clients SHALL publish returned drafts through ordinary preconditioned idempotent Factory commands; server publication SHALL verify the preset's mandatory contract and pinned provenance rather than trusting a caller's preset label.

#### Scenario: Headless client configures the preset
- **WHEN** a client discovers and resolves the bug-fix preset for a visible Project
- **THEN** it can review and publish exact ordinary Flow and Factory Configuration versions through REST without copying JSON files, guessing opaque references, or using the Console

#### Scenario: Client edits a resolved preset draft
- **WHEN** a client changes a required gate, permission ceiling, or dependency before publishing a preset-derived draft
- **THEN** ordinary publication plus preset-contract validation rejects an invalid draft instead of relying on the prior resolution result

#### Scenario: A selected reference is hidden
- **WHEN** discovery or resolution includes a dependency outside the actor's visibility
- **THEN** the response, ordering, counts, diagnostics, and cursors do not disclose that dependency or its capabilities

### Requirement: REST text Work intake and safe receipts
The API SHALL expose an additive bounded Project-scoped text Work command using exact published Factory Configuration and Repository versions, the existing allowed source and classification contracts, a scoped Work identity, description, and acceptance text. It SHALL require `Idempotency-Key`, bind the complete request fingerprint, and return the ordinary immutable Work/Run result after successful admission. An accepted but incomplete publication SHALL return an explicit pending receipt with a stable submission identity and bounded status observation; it SHALL NOT claim a Run or successful execution before admission commits. A receipt query SHALL expose only safe operation state, exact subject metadata, logical result references, and request correlation without raw submitted text, secrets, or private storage URLs.

#### Scenario: An HTTP-only client starts the first bug fix
- **WHEN** a client submits valid bounded text for an enabled bug-fix Configuration without earlier Builds
- **THEN** the command publishes its exact Work inputs and returns one ordinary Factory Run, or an explicit recoverable pending receipt until that admission completes

#### Scenario: An accepted intake outlives the request deadline
- **WHEN** durable reservation exists but byte publication cannot complete within the bounded request window
- **THEN** the client receives or can replay the same pending receipt and observe completion without creating a second submission

#### Scenario: Existing Artifact-based clients submit Work
- **WHEN** an existing client uses the Artifact-reference Work command
- **THEN** its request, response, source resolution, replay, and authorization behavior remain compatible

### Requirement: REST readiness reports identify exact subjects
The API SHALL expose a bounded read-only configuration readiness query identifying the exact Configuration and Repository versions, Flow digest, observation freshness, required check statuses, safe reason codes, and visible remediation targets. It SHALL distinguish this report from global deployment readiness and execution permission. The server SHALL classify readiness; clients SHALL NOT need to reproduce scheduling or permission policy. Request bodies and responses SHALL reject unsupported provider payloads and credential values.

#### Scenario: Client requests readiness for a prepared configuration
- **WHEN** a client supplies visible exact Configuration and Repository identities
- **THEN** the API returns a safe typed report with required blockers and expiry, without dispatching Work or resolving secrets

#### Scenario: Factory prerequisites are unavailable while CI/CD is healthy
- **WHEN** selected Factory requirements are blocked
- **THEN** the report identifies their configuration scope and ordinary CI/CD readiness remains governed by its own required dependencies

### Requirement: Quickstart operations share management security and OpenAPI contracts
Every quickstart operation SHALL carry the existing normalized management actor and stable authorization action/resource mapping before handler execution, and SHALL participate in bounded schemas, safe errors, request correlation, route inventories, generated-client drift checks, and audit redaction. Mutation replay SHALL remain scoped to the original management security scope; denial SHALL occur before resolution, reservation, byte I/O, or a mutation fact. Published operation receipts and resources SHALL be visibility-checked on every read. Preset publication SHALL retain strong current-version preconditions for replacement.

#### Scenario: Text intake is denied
- **WHEN** policy denies execution in the owning Project
- **THEN** no source resolution, publication reservation, object write, Work, Run, audit mutation, or replay outcome is created and no protected subject is disclosed

#### Scenario: Another scope reuses an idempotency key
- **WHEN** a caller in a different management security scope submits the same idempotency key
- **THEN** it cannot obtain the other scope's pending receipt or completed result through replay

#### Scenario: Published routes and generated clients are checked
- **WHEN** API schema and authorization-inventory checks run
- **THEN** every preset, readiness, text-intake, and receipt route has matching bounded schemas, stable errors, and authorization targets
