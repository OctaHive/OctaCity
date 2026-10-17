## ADDED Requirements

### Requirement: REST manual Issue creation and revision commands
The management API SHALL expose bounded visible Issue listing, manual creation, safe card reads, authorized requirements/revision reads and editable-field revision commands independently of Factory configuration and external providers. Creation/editing SHALL use closed generated schemas, scoped idempotency and current Issue-version preconditions where applicable. Clients SHALL supply neither execution state, accepted evidence/Decision, authenticated actor identity, credentials nor arbitrary provider payloads. Source-owned field edits SHALL be rejected with safe ownership feedback. An Issue save SHALL create no Work or execution.

#### Scenario: HTTP-only client creates and edits a task
- **WHEN** an authorized client creates a manual Issue and edits it using its returned version
- **THEN** the API returns stable identity/key and exact revision receipts without a provider, repository, model or pipeline prerequisite

#### Scenario: Edit response was lost
- **WHEN** the client replays its exact frozen edit after another revision exists
- **THEN** it receives the original logical receipt rather than a fresh edit against the newer head

#### Scenario: Client attempts an execution-state setter
- **WHEN** an Issue mutation supplies an acceptance, Run-success, candidate or permission field
- **THEN** validation rejects it without changing execution authority

### Requirement: REST Issue execution requests preserve ordinary admission
The API SHALL expose bounded Issue execution history, explicit reviewed execution requests and request-status reads. Commands SHALL bind the observed Issue revision, exact visible target/repository and input mapping; validate ordinary admission authority and prerequisites; and expose pending publication/admission versus a durably linked Work/Run. Replay SHALL resume the original request and return its exact outcome after interrupted publication, admission, association or acknowledgement. Reads SHALL link to existing Factory Run/Build routes. Later intentional execution SHALL require a distinct request, while existing retry controls SHALL remain unchanged.

#### Scenario: Client starts another pipeline for an Issue
- **WHEN** it confirms a new request against an exact revision and authorized target
- **THEN** the returned execution is retained under the same Issue with its own immutable Work/Run and prior executions remain addressable

#### Scenario: Client retries a partially admitted request
- **WHEN** the same request is submitted after restart with a lost admission acknowledgement
- **THEN** the API recovers its original Work/Run association without a second execution

#### Scenario: Issue changed before execution confirmation
- **WHEN** the submitted reviewed Issue version is no longer current
- **THEN** the API returns a conflict without silently selecting new content or invoking ordinary admission

### Requirement: REST source association and synchronization are capability-scoped
The API SHALL expose the current optional primary source association, versioned mapping and field ownership, synchronization observation/desired-state metadata and safe conflict resolution for a visible Issue. Explicit source attachment SHALL use authorized connection/native resource choices and verify one-link scoped uniqueness. Mapping changes SHALL reuse existing definition/integration management, expose only supported directions and effects, and require their own current versions and independent grants. Synchronization resolution SHALL bind the exact Issue/link/field conflict and reviewed desired state or policy; it SHALL NOT accept a raw credential, endpoint, unconditional overwrite or new execution instruction.

#### Scenario: Manual task is linked explicitly
- **WHEN** a permitted client attaches an unlinked Issue to an authorized unassociated native task after reviewing ownership
- **THEN** the source association and exact mapping are retained without a provider-created ticket or automatic Run

#### Scenario: Write access is missing during resolution
- **WHEN** a client attempts to push a desired state without the required current Reporter grant
- **THEN** denial occurs before a provider call and preserves task and execution history

#### Scenario: Client inspects failed outbound synchronization
- **WHEN** an accepted result has a pending or exhausted source update
- **THEN** the API distinguishes that synchronization condition from successful execution and returns bounded safe provenance

### Requirement: REST Issue history and results preserve exact subjects
Card and child reads SHALL publish bounded links to requirements revisions, execution requests, Work/Run/Flow/cycle/attempt/Build identities and exact candidate/evidence/assessment/Decision results. They SHALL expose task metadata, all active execution facts and synchronization state separately, including historical/unavailable/stale observations. AI profile/model and worker identities SHALL depend on retained visible facts. Log links SHALL use existing authorized readers. Existing admission/Run responses SHALL gain Issue links only as additive optional metadata; old URLs, clients and command bodies SHALL remain valid.

#### Scenario: Client requests current verification results
- **WHEN** evidence is missing, superseded or belongs to an earlier execution/candidate
- **THEN** the API returns exact state and subject references without deriving acceptance from the Issue status or a completed Build

#### Scenario: Old Artifact-admitted Work has no linked Issue yet
- **WHEN** a legacy client follows its existing Run URL before backfill completes
- **THEN** discovery and controls remain usable without read-time creation or provider access

### Requirement: REST operator-only Issue discussion retains scoped replay
The API SHALL expose bounded dedicated comment body reads and idempotent plain-text append commands for a visible Issue, with optional one-level same-Issue root replies. Actual actor provenance SHALL come from management context. Exact replay SHALL preserve the original logical outcome and changed reuse SHALL conflict. Comments SHALL not execute commands, update Context or authorize approval. Audit, list/card summaries and queue payloads SHALL exclude bodies; dedicated body reads SHALL apply no-store. Discussion rights SHALL grant neither Issue execution nor Factory control rights.

#### Scenario: Comment append is denied
- **WHEN** a client lacks Issue discussion permission or cannot observe its parent
- **THEN** denial precedes comment storage, sequence/replay allocation or mutation audit and discloses no hidden target

#### Scenario: Another security scope reuses a key
- **WHEN** a different management scope submits an already-used append key
- **THEN** it cannot obtain the first scope's comment or replay outcome

### Requirement: REST actionable Factory decision discovery spans Issue executions
The API SHALL expose bounded Project/global operator-action collections and an Issue-scoped view across its associated Work. Items SHALL identify exact target, permitted command, declared schema/disposition contract, current strong Run precondition, deadline and subject/reason. Resolution SHALL use existing response, bound-operation and retry endpoints with `Idempotency-Key` and Run `If-Match`; no Issue-version substitution, independent close-item or approve-Issue endpoint SHALL exist. Commands SHALL revalidate current eligibility regardless of the read projection.

#### Scenario: Client resolves a declared clarification
- **WHEN** it submits the item's declared value to the exact existing endpoint using the observed Run precondition
- **THEN** one intent is recorded and observation distinguishes that receipt from later owner completion

#### Scenario: Issue edit occurs before a control command
- **WHEN** its metadata changes without changing the target Run's eligibility
- **THEN** the control still uses its exact Run precondition and cannot be redirected to another execution by the Issue head

### Requirement: Issue routes share security pagination and generated contracts
Every new Issue/revision/execution/source/synchronization/discussion/result/action operation SHALL use the current authorization/action/resource and OpenAPI inventories, closed bounded schemas, safe errors/correlation and current visibility before projection/counts/order/cursors. Cursors SHALL bind normalized scope, filters, selected execution and declared observation contract. Dedicated task/revision/comment content and logs SHALL retain separate authorization and no-store policy; source text, command values and secrets SHALL not appear in URLs, cursor payloads, operational logs or generic errors. Query routes SHALL produce no task creation, backfill, provider dispatch or execution reservation.

#### Scenario: Cursor belongs to another Issue or execution
- **WHEN** a client reuses a history/result/discussion cursor for a different target or observation
- **THEN** the API returns a stable invalid-cursor outcome without continuing the original stream

#### Scenario: Hidden Issues surround a page
- **WHEN** matching hidden or no-longer-visible records exist
- **THEN** page items, counts and continuation metadata derive only from visible matching records

#### Scenario: Legacy client controls a Run
- **WHEN** it uses its existing Factory command body and headers without Issue metadata
- **THEN** authorization, replay, eligibility, concurrency and receipt semantics remain compatible
