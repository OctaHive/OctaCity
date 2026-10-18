## ADDED Requirements

### Requirement: External projects have protected single-submission setup
The management API SHALL provide a typed protected setup command accepting a bounded connection display name, external project address, transient write-only credential and selected supported task purposes. It SHALL advertise compatible installed setup capabilities and safe operation status through authorization-filtered reads. The command SHALL require connection, credential and enrollment authority, applicable strong preconditions and scoped idempotency before accessing private material or invoking a provider. It SHALL resolve and verify the native project under an explicitly installed enrollment policy, publish bounded immutable resource/capability references and make the connection usable without separate metadata, credential and activation commands. An address SHALL NOT authorize arbitrary network access, adapter installation, executable upload or filesystem access. Unsupported providers or enrollment policies SHALL fail explicitly. Saving SHALL NOT grant OctaCity Project access, enable pipeline inputs, synchronize tasks, admit Work or start execution.

#### Scenario: Administrator connects a previously unlisted private project
- **WHEN** an authorized administrator submits a supported private project address, name, token and task-read purpose under an enrollment-enabled installation
- **THEN** setup verifies the exact native resource using that token, publishes an enabled read connection and returns safe references/readiness without requiring the resource to have been prelisted
- **AND** no Project grant, task import, input activation or execution is created

#### Scenario: Another installed provider supports project setup
- **WHEN** an installed non-GitHub adapter advertises a compatible setup capability and approved address rules
- **THEN** the same management envelope accepts its project and returns opaque verified identities without GitHub-specific shared task or Factory types

#### Scenario: Address exceeds installed enrollment policy
- **WHEN** a submitted address targets an unapproved service, resource scope or unsupported adapter
- **THEN** setup rejects it before sending credentials to that destination or enrolling a resource, with no widening of installation policy

#### Scenario: Caller lacks credential or enrollment authority
- **WHEN** a metadata administrator without the other required setup authority submits a token
- **THEN** authorization denies setup before private preparation, native lookup or authoritative mutation and returns no protected existence information

### Requirement: Setup completion is recoverable and readiness is truthful
Setup SHALL retain one recoverable administrative operation identity across private preparation, verified resource publication and activation, without storing raw secrets in ordinary request history. Only successfully completed connections SHALL become selectable. Failed replacement SHALL preserve the prior usable current version. Restart and transport replay SHALL converge on the same logical result, reject conflicting intent, enforce current versions/policy/revocation and never resurrect older credential generations or disabled authority. Failed unreferenced preparation SHALL have bounded cleanup. Read-only checks SHALL distinguish confirmed, denied, unverified, deferred and unavailable capabilities. Authentication or required-read failure SHALL prevent successful setup; known-denied selected purposes SHALL fail without silent permission narrowing. Unverified write permission SHALL be shown explicitly and SHALL NOT be tested with mutations or represented as confirmed.

#### Scenario: Setup response is lost
- **WHEN** setup commits but the client loses the response and reads status or retries the same operation
- **THEN** the client obtains the original safe receipt and connection without duplicate versions, resources or credential publications

#### Scenario: Process stops during setup
- **WHEN** setup stops after private preparation but before activation
- **THEN** the new connection remains unavailable and recovery resumes that same operation or safely cleans its unreferenced preparation without deleting active material

#### Scenario: Permission check cannot prove writing
- **WHEN** resource reading succeeds but task-update permission cannot be verified without a write
- **THEN** setup can retain the explicitly selected task-update ceiling with an unverified result, makes no external mutation and does not claim write readiness

#### Scenario: Selected write permission is known to be denied
- **WHEN** the provider can establish that a selected task-update permission is denied
- **THEN** setup returns an actionable permission failure and neither silently saves a read-only substitute nor changes the old working connection

#### Scenario: Policy changes before setup completion
- **WHEN** enrollment policy or credential authority is revoked while a prepared setup is pending
- **THEN** recovery cannot activate it under stale authority and retains only safe failed-operation history and bounded cleanup

### Requirement: Archival retires connections without deleting history
The management API SHALL expose authorized provider-neutral archival with current metadata-version/admin-revision preconditions, scoped idempotency and safe audit receipts. Archival SHALL disable the connection, exclude it from default discovery and new-use selectors, and preserve authorization-filtered archived discovery, exact historical reads and retained dependency/history links. Filtering SHALL occur before pagination. Archival SHALL block subsequent provider dispatch, checks, metadata replacement, setup/rotation completion, new grants and input publication through that connection, including pending retries. It SHALL NOT delete or retarget retained grants, input versions, Work/Run, observations, pending effects or future internal-card references, recall accepted remote requests or prevent independent manual task creation/admission. Stale activation, setup completion and replay SHALL NOT restore archived authority. No unarchive operation is required by this change.

#### Scenario: Used connection is archived
- **WHEN** an authorized administrator archives a connection referenced by inputs and retained execution/reporting
- **THEN** the ordinary list and selectors omit it, authorized archived/history reads retain its identity and dependents, and subsequent provider dispatch is blocked without rewriting accepted results

#### Scenario: Archive races with a prepared setup or credential rotation
- **WHEN** archival commits before a prepared operation completes
- **THEN** completion cannot activate the connection or publish usable authority, and unreferenced preparation follows bounded cleanup

#### Scenario: Discovery pages include archived connections in storage
- **WHEN** a client requests ordinary connection discovery over stored active, disabled and archived connections
- **THEN** archived entries are excluded before bounded pagination and authorized archived discovery remains separately available

### Requirement: Permanent deletion is restricted to unused unreferenced connections
The management API SHALL expose authorized dependency inspection and permanent deletion for never-used connections only. A bounded safe preview SHALL report authoritative eligibility, visible dependency reasons and non-disclosing blockers for protected dependencies. Deletion SHALL require explicit intent, current metadata-version/admin-revision preconditions and scoped idempotency, and atomically recheck absence of retained Project grants, input versions, Work/Run, observations/card references, pending effects, in-progress setup/credential publication and other consumers. Concurrent reference publication SHALL be fenced against deletion. Archival alone SHALL NOT make a used connection eligible. Completed setup, credential configuration and administrative read-only checks alone SHALL NOT count as task/execution use; accepted runtime use SHALL remain a blocker even when it produced no Work.

Deletion SHALL retire only the unused connection data and exclusively owned unreferenced private credentials through recoverable generation-fenced cleanup. Shared installed handles, resources/profiles, adapters and other consumers' credentials SHALL remain intact. Private cleanup and ownership retirement SHALL prevent inheritance of deleted credentials or permanent ownership collisions during fresh setup. Interrupted cleanup SHALL remain unavailable for use and recover safely without claiming completed deletion prematurely. Minimal safe audit/idempotency tombstone evidence SHALL remain without secret values and SHALL prevent stale replay or identity reuse from resurrecting authority; completed administrative receipts alone SHALL NOT block deletion. Archive/delete SHALL NOT mutate external projects/items or revoke remote tokens. These rules SHALL apply equally to existing installed and newly enrolled connections and SHALL preserve manual creation/intake without a provider.

#### Scenario: Administrator deletes an unused configured connection
- **WHEN** a connection has configured credentials but has never been used, has no retained consumers or pending preparation, and the administrator confirms deletion
- **THEN** deletion completes with a safe receipt after exclusive credential cleanup, shared installation data remains, and fresh setup neither inherits old credentials nor conflicts with the deleted owner's retired binding

#### Scenario: Dependency appears after deletion preview
- **WHEN** a Project grant or input reference is published after an eligible preview but before deletion commits
- **THEN** deletion conflicts or is rejected with a safe dependency result and retains the connection and its credentials

#### Scenario: Retained or protected dependency prevents deletion
- **WHEN** a used or referenced connection is submitted for deletion, including after archival
- **THEN** deletion is rejected without cascading removal, visible reasons are reported and protected dependent identities remain undisclosed

#### Scenario: Process stops during private deletion cleanup
- **WHEN** deletion is interrupted after the connection becomes unavailable but before exclusive private cleanup completes
- **THEN** status/retry resumes the same operation after restart, retains minimal safe audit evidence and cannot expose or reactivate the old credential

#### Scenario: Old operation is replayed after deletion
- **WHEN** a completed setup, activation, rotation or deletion is retried after permanent deletion
- **THEN** the original safe receipt is returned where permitted without recreating metadata, credentials, grants or usable authority

## MODIFIED Requirements

### Requirement: Reusable provider connections have versioned administration
The management API SHALL support authorized creation, inspection, validation, immutable replacement, disablement and dependency inspection of reusable named provider connections. Public metadata SHALL include stable connection identity, display name, installed provider/adapter reference, validated service endpoint, supported authentication/capability choices, safe resource references and configuration/readiness state. Resources SHALL originate either from verified installed declarations or protected verified enrollment within an explicit installed policy; ordinary metadata requests SHALL NOT self-enroll arbitrary resources. A connection SHALL NOT select a pipeline target or implicitly grant a Project access. Management SHALL reuse verified installed adapters and SHALL NOT accept executable uploads or arbitrary filesystem/credential paths. Provider identities and resource envelopes SHALL remain open to other installed providers without introducing native provider types into Factory execution. Mutations SHALL use current-version preconditions, idempotency and safe audit receipts. Existing metadata-only administration SHALL remain supported without accepting raw credentials.

#### Scenario: Connection metadata is published
- **WHEN** an authorized administrator creates a connection using a compatible installed provider and validated settings
- **THEN** publication returns a stable versioned identity and safe metadata without secrets, activated inputs or implicit Project grants

#### Scenario: Another installed provider is selected
- **WHEN** a compatible non-GitHub adapter advertises supported connection capabilities
- **THEN** the same administrative envelope accepts its opaque provider/resource identities while exposing only its actual installed capabilities

#### Scenario: Endpoint attempts to expand network access
- **WHEN** a proposed endpoint, redirect or native resource exceeds the trusted installation network/resource ceiling
- **THEN** validation rejects unauthorized access; only separately authorized setup discovery within an installed enrollment policy can verify a previously unlisted resource

### Requirement: Credential setup and rotation are protected write-only operations
A separately authorized integration credential operation or protected combined setup command SHALL accept a bounded transient secret value or a permitted existing server-side handle, store it in protected server-side credential storage and return only a safe receipt/reference and configured status. It SHALL support replacement/rotation without read-back of existing values. Raw secret values SHALL NOT enter public metadata, ordinary database configuration/history, Work, idempotency request-body storage, audit, logs, URLs, errors or API responses. Secret submission SHALL use authenticated protected transport and request-body redaction. Private material publication and connection activation SHALL be recoverable under one stable administrative operation identity: partial preparation SHALL NOT expose an active unusable connection, and retries SHALL NOT create duplicate active versions or restore an earlier credential after a later rotation/revocation. Unreferenced failed preparations SHALL have bounded cleanup. Existing operator-installed handles SHALL remain usable without exporting their values. Combining setup in one submission SHALL NOT merge independent purpose authority or grant Project use.

#### Scenario: Secret submission succeeds or fails
- **WHEN** an administrator submits a valid or invalid token through the credential operation or combined setup
- **THEN** both success and failure expose only safe classified outcomes, never the submitted value or private storage path

#### Scenario: Publication stops after private preparation
- **WHEN** a process stops between credential preparation and connection activation
- **THEN** the connection remains inactive, and retry resumes the same operation or bounded cleanup removes its unreferenced private preparation

#### Scenario: An old rotation request is replayed
- **WHEN** a previously completed rotation is replayed after a newer rotation or revocation
- **THEN** the API returns its original safe receipt without restoring old material or reauthorizing revoked access

#### Scenario: Replay key is reused with another token
- **WHEN** the same security scope reuses a setup operation key with a different credential value
- **THEN** the command conflicts without exposing credential-derived comparison material or replacing the accepted preparation

### Requirement: Project access constrains connection resources and purposes
The management API SHALL support explicit versioned Project access to selected connection resources and purposes, distinguishing task reads, task updates, repository reads and Delivery. Publication SHALL require authority to manage the connection grant and the target OctaCity Project. Integration administration, credential management and pipeline use SHALL be separately authorized. Grants SHALL be limited to a subset of the connection's current declared resources/purposes and installed ceilings. Enumeration, validation, input publication and each protected runtime operation SHALL enforce current Project access and current connection ceilings before exposing protected material or hidden resources. Remote token permissions SHALL NOT widen a Project grant, and ordinary pipeline publication SHALL NOT modify it. Removing a connection purpose SHALL block its subsequent use and invalidate affected grants so that later re-adding the purpose does not silently restore Project access.

#### Scenario: Pipeline editor has read-only task access
- **WHEN** a Project is granted task reading but not task updates or Delivery
- **THEN** its editor can select a compatible read input but cannot enable reporting/publication even if the installed token has broader remote permissions

#### Scenario: Caller references an ungranted connection
- **WHEN** a client lists resources or submits an input for a connection unavailable to its Project
- **THEN** the API denies the operation without enumerating hidden provider resources or revealing credentials

#### Scenario: Grant attempts to exceed connection permission
- **WHEN** an administrator requests task-update access from a read-only connection
- **THEN** grant publication is rejected regardless of the token's broader remote permissions

#### Scenario: Connection write permission is removed and later restored
- **WHEN** a connection removes task-update permission and a later version adds it again
- **THEN** pending writes remain blocked until an authorized explicit Project grant is republished, without rewriting accepted Work or reporting targets
