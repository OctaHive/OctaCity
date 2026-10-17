## ADDED Requirements

### Requirement: Factory source inputs have typed versioned management
The management REST API SHALL expose authorization-filtered discovery, inspection, validation, creation and immutable replacement of Project-owned Work Source input bindings and exclusive input groups. Representations SHALL expose safe Project-authorized connection/resource and source/reporter capability choices, installed repository choices, exact target Factory Configuration and Repository references, bounded Issue state/label predicates, input mappings, status mappings and enabled state without provider tokens or raw provider request payloads. Mutations SHALL require the existing idempotency and current-version preconditions. Validation SHALL return normalized input selection, safe incompatibilities and target/version details before activation.

#### Scenario: Client configures an approved defect input
- **WHEN** an authorized client selects a visible installed GitHub source, a visible target configuration, and the required `approved` and `bug` labels
- **THEN** validation and publication retain a typed immutable input binding under that Project and exact mutation replay returns the same version

#### Scenario: Client references a hidden source or target
- **WHEN** an input names a repository, source capability, target configuration or reporting grant unavailable to its Project
- **THEN** the API returns the safe authorization or validation outcome without revealing hidden resources or activating the input

### Requirement: Source previews and diagnostics distinguish discovery from execution
The management REST API SHALL expose bounded read-only selector previews and safe input/reporting diagnostics including effective selector, exact versions, last scan/progress, next retry, candidate dispositions and visible admitted Work/Run correlations. A preview SHALL use installed read authority, respect provider bounds and SHALL NOT reserve identities, advance production cursors, admit Work or write to GitHub. Diagnostics SHALL distinguish filtered, ambiguous, replayed, newly admitted, unavailable and projection-pending/exhausted states without claiming preview results or external labels are authoritative execution outcomes. Raw Issue bodies and provider secrets SHALL be excluded from preview and diagnostic summaries.

#### Scenario: Client previews an unrestricted label selector
- **WHEN** a client previews a visible bounded `open` Issue source without required labels
- **THEN** the API returns safe labeled and unlabeled candidate summaries and selector reasons without creating Runs or modifying scan progress

#### Scenario: GitHub returns a rate limit
- **WHEN** preview or discovery is deferred by the provider
- **THEN** diagnostics expose a safe scoped retry time instead of reporting no matching Issues or global server failure

### Requirement: Stage source-update actions have typed configuration and provenance
The management REST API SHALL support configuring and validating source label-update effects for individual declared Flow stage outcomes through existing versioned Flow/action publication. Clients SHALL be able to select the original source binding, visible Reporter mapping and explicit labels to add/remove without entering provider wire payloads or credentials. Publication SHALL freeze the resolved mapping and reject conflicting or unauthorized effects. Diagnostics SHALL correlate each queued/applied/failed projection to its originating stage, outcome and action identity, including intermediate stages before the Run is terminal.

#### Scenario: Client configures label changes after an intermediate stage
- **WHEN** an authorized client configures a declared stage outcome to add and remove selected permitted labels
- **THEN** validation presents those exact source effects and publication retains them in the immutable Flow/action configuration independently of other stage outcomes

### Requirement: Reusable provider connections have versioned administration
The management API SHALL support authorized creation, inspection, validation, immutable replacement, disablement and dependency inspection of reusable named provider connections. Public metadata SHALL include stable connection identity, display name, installed provider/adapter reference, validated service endpoint, supported authentication/capability choices, safe resource references and configuration/readiness state. A connection SHALL NOT select a pipeline target or implicitly grant a Project access. Management SHALL reuse verified installed adapters and SHALL NOT accept executable uploads or arbitrary filesystem/credential paths. Provider identities and resource envelopes SHALL remain open to other installed providers without introducing native provider types into Factory execution. Mutations SHALL use current-version preconditions, idempotency and safe audit receipts.

#### Scenario: Connection metadata is published
- **WHEN** an authorized administrator creates a connection using a compatible installed provider and validated settings
- **THEN** publication returns a stable versioned identity and safe metadata without secrets, activated inputs or implicit Project grants

#### Scenario: Another installed provider is selected
- **WHEN** a compatible non-GitHub adapter advertises supported connection capabilities
- **THEN** the same administrative envelope accepts its opaque provider/resource identities while exposing only its actual installed capabilities

#### Scenario: Endpoint attempts to expand network access
- **WHEN** a proposed endpoint, redirect or native resource exceeds the trusted installation network/resource ceiling
- **THEN** validation rejects it before accessing credentials or making an unauthorized request

### Requirement: Credential setup and rotation are protected write-only operations
A separately authorized integration credential operation SHALL accept a bounded transient secret value or a permitted existing server-side handle, store it in protected server-side credential storage and return only a safe receipt/reference and configured status. It SHALL support replacement/rotation without read-back of existing values. Raw secret values SHALL NOT enter public metadata, ordinary database configuration/history, Work, idempotency request-body storage, audit, logs, URLs, errors or API responses. Secret submission SHALL use authenticated protected transport and request-body redaction. Private material publication and connection activation SHALL be recoverable under one stable administrative operation identity: partial preparation SHALL NOT expose an active unusable connection, and retries SHALL NOT create duplicate active versions or restore an earlier credential after a later rotation/revocation. Unreferenced failed preparations SHALL have bounded cleanup. Existing operator-installed handles SHALL remain usable without exporting their values.

#### Scenario: Secret submission succeeds or fails
- **WHEN** an administrator submits a valid or invalid token through the credential operation
- **THEN** both success and failure expose only safe classified outcomes, never the submitted value or private storage path

#### Scenario: Publication stops after private preparation
- **WHEN** a process stops between credential preparation and connection activation
- **THEN** the connection remains inactive, and retry resumes the same operation or bounded cleanup removes its unreferenced private preparation

#### Scenario: An old rotation request is replayed
- **WHEN** a previously completed rotation is replayed after a newer rotation or revocation
- **THEN** the API returns its original safe receipt without restoring old material or reauthorizing revoked access

### Requirement: Project access constrains connection resources and purposes
The management API SHALL support explicit versioned Project access to selected connection resources and purposes, distinguishing task reads, task updates, repository reads and Delivery. Publication SHALL require authority to manage the connection grant and the target OctaCity Project. Integration administration, credential management and pipeline use SHALL be separately authorized. Enumeration, validation, input publication and each protected runtime operation SHALL enforce current Project access and installed ceilings before exposing protected material or hidden resources. Remote token permissions SHALL NOT widen a Project grant, and ordinary pipeline publication SHALL NOT modify it.

#### Scenario: Pipeline editor has read-only task access
- **WHEN** a Project is granted task reading but not task updates or Delivery
- **THEN** its editor can select a compatible read input but cannot enable reporting/publication even if the installed token has broader remote permissions

#### Scenario: Caller references an ungranted connection
- **WHEN** a client lists resources or submits an input for a connection unavailable to its Project
- **THEN** the API denies the operation without enumerating hidden provider resources or revealing credentials

### Requirement: Connection checks are bounded and read-only
The management API SHALL expose a separately authorized bounded connection check that verifies installed configuration, authentication, permitted resource identity/access and supported capability readiness through the verified provider. It SHALL distinguish missing configuration/credential, denied authentication/access, unsupported or unverified capabilities, throttling and unavailability. It SHALL NOT mutate provider resources, grant authority, start workers, admit Work or advance production cursors. Write readiness that cannot be verified without mutation SHALL remain explicitly unverified rather than be asserted or tested with a write.

#### Scenario: Reading is ready but writing is unverified
- **WHEN** a read-only connection check confirms resource access but cannot prove task-update authority
- **THEN** it reports those results separately, makes no label/state/branch/review mutation and does not grant write access

#### Scenario: Provider is unavailable during a check
- **WHEN** a permitted connection check is throttled or receives a transient failure
- **THEN** the API returns a scoped safe deferred result without reporting an empty resource list or changing pipeline progress

### Requirement: Connection lifecycle preserves accepted work and current revocation
Connection/resource replacement SHALL retain referenced immutable versions and SHALL NOT silently redirect existing inputs or pending reporting to another endpoint, adapter or native resource. Credential rotation within the same logical scope SHALL preserve connection/input/admission identities. Disablement and Project-access revocation SHALL block subsequent protected operations, including retries of retained intents, without rewriting accepted Work/Run/Flow or falling back to other credentials. Retirement SHALL expose authorized dependency information, prevent destructive removal of referenced configuration/material, and retain safe history. Adoption of existing installed connections SHALL preserve original identities and SHALL NOT activate inputs or widen grants automatically.

#### Scenario: Connection changes while an old projection is pending
- **WHEN** an administrator publishes a replacement endpoint or native resource
- **THEN** the old projection retains its exact scope and either uses its still-authorized retained version or reports a bounded failure, never writes to the replacement resource

#### Scenario: Access is revoked before retry
- **WHEN** a Project's task-update grant is revoked after an action was accepted
- **THEN** retry cannot make a new provider write, and the original accepted result and finite reporting history remain retained

#### Scenario: Referenced connection is retired
- **WHEN** an administrator attempts to remove a connection used by inputs or pending reporting
- **THEN** the API shows only authorized dependencies and requires a safe disable/retention path rather than deleting referenced configuration or secrets
