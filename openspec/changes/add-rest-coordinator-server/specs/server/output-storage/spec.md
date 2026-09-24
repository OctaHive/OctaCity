## Purpose

Defines the logical Build Result, backend-neutral artifact and report publication, and immutable build-log chunk storage while keeping object-storage credentials and physical layout server-private.

## ADDED Requirements

### Requirement: Build Result is one logical aggregate
A Build Result SHALL expose the immutable execution-configuration snapshot, ordered event and build-log history, and produced files and reports as one logical aggregate even when its components use different physical stores. The configuration snapshot and ordering metadata SHALL remain authoritative in PostgreSQL, while large immutable bytes MAY use the configured object store.

#### Scenario: Client reads a completed Build
- **WHEN** a client requests the completed Build Result
- **THEN** it can discover the exact configuration snapshot, ordered log, and produced outputs through logical identifiers without learning their physical storage layout

### Requirement: Backend-neutral artifact protocol
Agent and management interfaces SHALL describe artifact operations using logical identifiers, immutable content identity, bounded metadata, and opaque short-lived transfer capabilities. The protocol SHALL NOT require callers to understand S3 buckets, object keys, access keys, or provider-specific response fields.

#### Scenario: Storage implementation changes
- **WHEN** an operator replaces the S3-compatible adapter with another conforming object adapter
- **THEN** agent upload and management download protocol messages remain compatible

### Requirement: Durable fenced upload records
Beginning an output upload SHALL atomically persist its lease fence, idempotency key, logical metadata, expected size, SHA-256, and pending state before returning a short-lived upload capability.

#### Scenario: Begin response is lost
- **WHEN** the current agent repeats an identical begin request
- **THEN** the server returns the same logical upload record without allocating another published output

### Requirement: Verified immutable publication
Completing an upload SHALL independently verify the exact stored object generation, byte length, and SHA-256 before atomically marking the output published. Provider ETags SHALL NOT be treated as content digests.

#### Scenario: Uploaded bytes differ
- **WHEN** the object does not match the authorized size or SHA-256
- **THEN** the server rejects completion, keeps the output unpublished, and makes the invalid generation unavailable for download

### Requirement: Logical storage interface
Management and agent responses SHALL identify outputs by opaque logical identifiers and SHALL NOT expose object-store access keys, bucket names, or permanent physical object keys.

#### Scenario: Client requests an artifact
- **WHEN** a client requests download of a published artifact
- **THEN** the server returns bounded metadata and a short-lived download capability for the immutable object

### Requirement: Artifact and report metadata
The server SHALL preserve artifact names, report names and plugin-defined report formats, build/attempt/job provenance, media type, size, digest, and publication timestamps without hard-coding a finite report-format enum.

#### Scenario: Unknown report format is uploaded
- **WHEN** an agent publishes a report with a valid bounded format string unknown to the server
- **THEN** the server stores and returns that format without interpreting or rejecting it solely for being unknown

### Requirement: Immutable redacted log chunks
The server SHALL archive textual stdout and stderr as bounded immutable chunks with logical identity, Build, Attempt and Job provenance, stream, contiguous event-sequence range, byte length, and cryptographic digest. Required redaction SHALL run before a chunk is written to object storage or supplied to a search index, and the protocol SHALL NOT create one object per log line.

#### Scenario: A Job emits a large stdout stream
- **WHEN** accepted contiguous events cross the configured chunk boundary
- **THEN** the server writes an idempotently addressed redacted chunk and records its verified logical manifest without exposing a bucket or object key

#### Scenario: Database commit fails after a chunk write
- **WHEN** an immutable chunk reaches object storage but its manifest transaction does not commit
- **THEN** the chunk remains invisible, retry uses the same logical identity and digest, and retention may safely remove the orphan

### Requirement: Retention is safe and idempotent
Each Build Result component SHALL receive its automatic-retention deadline from the immutable effective project-policy snapshot recorded for that Build. Retention SHALL remove logical visibility before deleting search documents, manifests, or bytes, preserve components whose deadline has not elapsed or whose Build Result has an active retention hold, and safely retry interrupted deletions. A retained component SHALL continue to count toward its applicable storage or capacity quota.

#### Scenario: Configured retention period elapses
- **WHEN** a Build Result component reaches its recorded automatic-retention deadline without an active hold or another retained reference
- **THEN** the server durably makes it unavailable and eventually removes its derived search documents, manifests, and object bytes in an idempotent order

#### Scenario: Server stops during deletion
- **WHEN** retention is interrupted after hiding metadata but before object deletion completes
- **THEN** a later pass resumes search-document and object deletion without making the Build Result visible again

#### Scenario: Stale indexing follows deletion
- **WHEN** delayed indexing or rebuild work arrives after a Build Result has a durable deletion tombstone
- **THEN** the projection acknowledges the obsolete work without restoring any searchable document

### Requirement: Build Result retention holds
The server SHALL allow an operator to place a permanent or time-bounded retention hold, also presented as a pin, on a visible Build Result. A hold SHALL cover the Build metadata, configuration snapshot, logs, artifacts, and reports as one aggregate. It SHALL be stored separately from the original automatic-retention deadlines and SHALL record a bounded reason, creation time, optional expiry, available actor identity, and request identity. Releasing or expiring a hold SHALL NOT extend or recompute any original deadline.

#### Scenario: Operator pins a Build Result
- **WHEN** an operator places a hold before retention has hidden the Build Result
- **THEN** every component remains visible and retrievable after its automatic-retention deadline while the hold is active

#### Scenario: Operator releases an overdue hold
- **WHEN** the last active hold is released or expires after one or more original retention deadlines have elapsed
- **THEN** those overdue components become eligible for the next retention pass without receiving a new retention period

#### Scenario: Pin races with retention
- **WHEN** a hold command races with the first retention transition for the same Build Result
- **THEN** the server atomically accepts the hold before hiding any component or rejects it with a stable conflict after deletion has begun, and never reports a pin for partially deleted data
