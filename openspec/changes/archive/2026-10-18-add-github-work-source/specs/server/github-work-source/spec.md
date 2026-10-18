## Purpose

Defines configurable GitHub Issue discovery, reusable bounded native task observations and non-authoritative status reporting while preserving source scope, ordinary admission replay and lifecycle authority.

## ADDED Requirements

### Requirement: GitHub Issues are selected by explicit pipeline input predicates
Each GitHub Work Source input SHALL select an installed repository and Issue state (`open`, `closed` or `all`) plus bounded `all_of`, `any_of` and `none_of` label sets. A match SHALL contain every `all_of` label, at least one `any_of` label when that set is nonempty, and no `none_of` label. Three empty label sets SHALL mean no label restriction, including unlabeled Issues. Invalid or contradictory selectors SHALL be rejected before activation. Label names SHALL be operator configuration rather than built-in Factory states. Pull requests SHALL be excluded even when returned by the provider's Issues listing.

#### Scenario: Operator selects every open Issue
- **WHEN** an enabled input selects `open` with three empty label sets
- **THEN** labeled and unlabeled open Issues are eligible for ordinary admission while closed Issues and pull requests are excluded

#### Scenario: Operator selects approved defects
- **WHEN** an input requires `approved` and `bug`, excludes `rejected`, and an Issue has those required labels without the excluded label
- **THEN** the Issue matches that input regardless of its unrelated labels

#### Scenario: Selector uses alternatives and exclusions
- **WHEN** an input requires `approved`, accepts either `feature-request` or `enhancement`, and excludes `blocked`
- **THEN** only Issues satisfying all three predicates match, and an absent alternative or present excluded label prevents admission

#### Scenario: Selector is contradictory
- **WHEN** an input both requires and excludes the same label or exceeds its declared predicate bounds
- **THEN** validation rejects that input with a safe field-level error and no polling or admission begins

### Requirement: Polling is bounded and restart-safe
The Source SHALL poll only authorized installed repositories under bounded page, item, byte, duration, concurrency and retry limits. It SHALL respect provider rate-limit delays, authenticate each call through its independently scoped credential and reject unapproved redirect or pagination destinations. Durable progress SHALL advance only after each covered candidate has a retained admission or classified disposition. Incremental polling SHALL include overlap and periodic bounded reconciliation so restarts, equal timestamps and changing paginated results do not permanently omit eligible Issues. Discovery SHALL NOT modify GitHub labels or Issue state to claim ownership.

#### Scenario: Server restarts during a page
- **WHEN** the server stops after admitting an Issue but before committing progress for its page
- **THEN** the next owner revisits the page and obtains the existing admission for that input without losing unprocessed candidates

#### Scenario: Provider throttles discovery
- **WHEN** GitHub returns a rate-limit delay or a transient failure
- **THEN** the input reports bounded deferred progress, retries no earlier than permitted, and neither records an empty successful scan nor affects ordinary CI/CD readiness

#### Scenario: Issues change during pagination
- **WHEN** multiple Issues share an update timestamp or label edits move Issues between pages while scanning
- **THEN** overlap and reconciliation revisit potentially omitted candidates within configured scan bounds without creating duplicate Runs

### Requirement: Accepted Issue input is a bounded immutable snapshot
The Source SHALL submit a bounded Issue snapshot containing stable provider installation, repository and item identities, its human-facing number, observed update identity, selected label/state evidence and title/body provenance. Task and acceptance Artifact inputs SHALL follow an explicit versioned mapping; absent required content or oversized content SHALL produce a classified failure rather than invented acceptance criteria or silent truncation. Admission SHALL bind the snapshot, exact input-binding version, authorized Project/Repository/Configuration and resolved immutable base. Issue text SHALL be untrusted task data and SHALL NOT choose credentials, executable profiles, repository targets or Flow transitions. Comments, attachments and linked resources SHALL NOT be fetched implicitly.

#### Scenario: Issue body changes after admission
- **WHEN** an admitted Issue is edited before the next poll or during its Run
- **THEN** the retained Work inputs remain unchanged and repeated observation returns the existing admission for that input

#### Scenario: Source content cannot satisfy the input contract
- **WHEN** an Issue lacks required mapped acceptance input or exceeds the configured payload limit
- **THEN** the input records a safe rejected or deferred candidate disposition and starts no Run with fabricated or partial requirements

### Requirement: GitHub reporting maps configured values to scoped Issue effects
The Reporter SHALL translate arbitrary configured Work status values through an exact versioned mapping of managed-label additions/removals and optional Issue open/close state. It SHALL accept those actions from any configured intermediate, nested or terminal stage without waiting for the entire pipeline to complete. It SHALL preserve unrelated labels, reject conflicting mappings and unconfigured status values, and require explicit Issue-write authority separate from Source-read and Delivery authority. `approved`, `rejected`, `bug` and `feature-request` SHALL have no intrinsic meaning in the adapter. Work-backed status projection in this change SHALL originate only from accepted configured Factory actions and SHALL NOT accept arbitrary writes from Issue prose or model output. Additional internal-Issue command producers SHALL require their separately specified authorization and retained intent boundary; generic connection metadata or a card association SHALL NOT authorize them.

#### Scenario: Assessment is approved
- **WHEN** an accepted status action requests a configured value mapped to adding `approved` and removing `rejected`
- **THEN** the Reporter converges those managed labels on the exact source Issue while retaining all unrelated labels

#### Scenario: Status spelling differs between installations
- **WHEN** an operator maps `needs-human-eyes` to a different label set and leaves Issue state unchanged
- **THEN** the same reporting capability applies that mapping without a new lifecycle state or adapter implementation

#### Scenario: Status mapping is absent
- **WHEN** a reporting action requests a value absent from its pinned mapping
- **THEN** projection fails safely without guessing a label, closing the Issue or changing the accepted Factory result

#### Scenario: Internal task association changes after an action was accepted
- **WHEN** a card later attaches a source or replaces its mapping while a Work-backed operation is pending
- **THEN** reporting retains its originally authorized native target/effects and current grant checks rather than resolving a replacement target from the card

### Requirement: GitHub observations can serve optional execution-neutral consumers
The installed Source read boundary SHALL provide bounded authorized native task observations independently of a consumer's pipeline matching decision. Its application-facing observation SHALL preserve verified installation/container/item identity, human display context, update/freshness evidence and bounded title/body digest or retained-input provenance without leaking secrets or GitHub wire types into shared task/Factory contracts. Repeated admission SHALL keep its original snapshot while a separately authorized consumer can observe fresh source data. Source access SHALL respect current task-read grants, installed network/resource ceilings, rate limits and bounded reconciliation for filtered, already admitted or closed linked tasks. This seam SHALL NOT create an internal Issue, require a Factory target for observation consumption, implicitly enable refresh or start a new admission; the card change owns those policies.

#### Scenario: A task is edited after its first admission
- **WHEN** an authorized optional consumer obtains a fresh bounded observation of that same native task
- **THEN** it receives fresh source provenance while ordinary input replay returns the original Work/Run and leaves its input Artifacts unchanged

#### Scenario: Read authority is revoked before linked-task refresh
- **WHEN** an optional consumer requests an observation without current task-read authority
- **THEN** no protected GitHub read occurs, the result is safely classified and no broader token or execution-input grant is used as fallback

### Requirement: Reporting recovery preserves authoritative history and managed-label ordering
Reporting SHALL use the existing durable independently fenced projection mechanism with stable operation identities, original deadlines and bounded retries. An uncertain response SHALL be observed before further writes. Partial multi-request updates SHALL converge only the original desired effects unless superseded by a newer authorized intent. The Reporter SHALL reject known-stale operations for the same Issue and overlapping managed fields before dispatch and SHALL reconcile late in-flight effects toward the newest authorized projection within its bounds, reporting exhaustion or external conflicts rather than claiming atomic external ordering. Projection failure SHALL NOT roll back accepted Work, and a later matching label observation SHALL authorize only a new ordinary admission rather than mutate an existing Run.

#### Scenario: Label update response is lost
- **WHEN** GitHub applies a managed-label change but the Reporter loses its response
- **THEN** retry observes the exact Issue and converges the original operation without another Run or a duplicated logical status action

#### Scenario: An older reporter resumes after a newer projection
- **WHEN** a delayed reporting operation would restore an obsolete value of a managed status group
- **THEN** its stale ownership or superseded intent prevents that write and the newer authorized status remains authoritative for projection

#### Scenario: Reporting fails after assessment completed
- **WHEN** an assessment result commits but GitHub is unavailable
- **THEN** the completed assessment remains retained, reporting shows pending or exhausted delivery, and no downstream admission is falsely reported as successful

#### Scenario: An in-flight request lands after its intent was superseded
- **WHEN** a request already sent to GitHub applies an older managed-label value after a newer intent was accepted
- **THEN** observation detects the mismatch and bounded reconciliation restores the newest desired projection or records an explicit conflict/exhaustion without rewriting Factory history

### Requirement: Capabilities and credentials remain independently scoped
The installed GitHub provider SHALL advertise Source and Reporter separately from Delivery, enforce repository and network ceilings before accessing protected material, and reject credential-class substitution. Credentials SHALL remain in protected server-side storage resolved through scoped private handles. Existing operator-installed private files SHALL remain supported. Management SHALL accept raw credentials only as transient input to the authorized write-only integration credential operation. Raw credentials SHALL NOT appear in Work inputs, provider responses, ordinary REST representations, persisted browser state, logs, audit or diagnostics. Pipeline editors SHALL select authorized connection/resource references without receiving or entering tokens. A Source or Reporter outage SHALL remain scoped to its Factory inputs/effects and SHALL NOT disable ordinary Builds or require those capabilities for manual admission.

#### Scenario: Delivery credential is presented as an Issue source credential
- **WHEN** a caller tries to use Delivery authority to read tickets or update labels without the separately permitted capability
- **THEN** the operation is rejected before a provider effect and no credential is exposed

#### Scenario: GitHub source is not installed
- **WHEN** a deployment runs ordinary Builds or manually admitted Factory Work without a GitHub input
- **THEN** those workflows remain available without GitHub credentials or polling

### Requirement: Managed GitHub connections preserve installed scope and identity
GitHub connection administration SHALL configure a named connection using the installed adapter, permitted API endpoint, token authentication and explicitly selected native repository identities with owner/name display context. It SHALL provision or adopt independently scoped private credentials for enabled purposes and expose safe configuration/access readiness without returning token values. Repository enumeration and connection checks SHALL remain bounded, respect the trusted network ceiling and verify native repository identity before protected use. Unsupported authentication methods or capabilities SHALL NOT be advertised as usable. Native GitLab/Jira implementations SHALL NOT be implied by common connection metadata alone.

#### Scenario: Administrator provisions an Issue-read connection
- **WHEN** an authorized administrator submits a token for an allowed GitHub repository and enables only task reading
- **THEN** the connection can be granted to an OctaCity Project for Issue discovery without granting Reporter or Delivery authority

#### Scenario: Existing installed configuration is adopted
- **WHEN** an administrator explicitly adopts a private installed GitHub configuration and scoped handles
- **THEN** the managed connection retains its installation/native resource identities, requires no token read-back, and does not enable inputs or duplicate existing Work

#### Scenario: Token rotates while reporting is pending
- **WHEN** a credential is rotated behind the same permitted purpose handle
- **THEN** pending reporting retains its original operation, native item, profile and mapping and uses the current permitted credential without resetting deadlines or creating Work

#### Scenario: Connection or write access is revoked
- **WHEN** a connection is disabled or its Project's task-update grant is revoked before the next dispatch
- **THEN** that dispatch performs no new native mutation or credential fallback, while accepted Factory history remains retained under the original bounded reporting policy
