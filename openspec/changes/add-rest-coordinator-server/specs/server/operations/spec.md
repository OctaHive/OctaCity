## Purpose

Defines observable, auditable, secret-safe, and recoverable operation of the headless server and its durable dependencies across startup, shutdown, upgrade, and disaster recovery.

## ADDED Requirements

### Requirement: Validated startup and migrations
The server SHALL reject unknown or unsafe configuration, verify required credential files and dependencies, and apply or explicitly validate ordered PostgreSQL migrations before becoming ready.

#### Scenario: Database schema is incompatible
- **WHEN** the database schema is newer than the server or a required migration cannot complete
- **THEN** readiness remains false and no agent lease or mutating management request is accepted

### Requirement: Distinct liveness and readiness
The server SHALL expose unauthenticated bounded liveness and readiness probes. Liveness SHALL describe process responsiveness; readiness SHALL require the database, signing capability, mandatory storage dependencies, configured mandatory secret providers, and supervised workers needed for safe mutations.

#### Scenario: Object storage is unavailable
- **WHEN** object storage is configured as mandatory and its health check fails
- **THEN** liveness can remain successful while readiness reports unavailable

### Requirement: Trusted-network management mode
The first release SHALL expose management operations without operator authentication or role evaluation and SHALL require explicit configuration acknowledging trusted-network deployment. This mode SHALL NOT disable agent credentials, webhook verification, JobSpec signing, or secret-provider authentication.

#### Scenario: Trusted-network acknowledgement is absent
- **WHEN** the server is configured to listen on a non-loopback management address without explicitly acknowledging unauthenticated management access
- **THEN** configuration validation fails before the listener starts

### Requirement: Secret-provider isolation
The server SHALL store and expose logical secret and workload-identity references rather than secret values. Secret-provider credentials and returned sensitive material SHALL remain inside the configured provider adapter and SHALL be delivered only through the bounded agent or workload mechanism selected by policy.

#### Scenario: Management client reads a build configuration
- **WHEN** a configuration references a Vault, file, or future secret provider entry
- **THEN** the response contains only the logical reference and policy metadata and no provider credential or secret value

### Requirement: Secret-safe observability
The server SHALL emit structured tracing and bounded metrics for management requests, webhook and VCS adapters, triggers, orchestration, scheduling, leases, queues, database operations, cache, artifacts, and workers while redacting agent credentials, signing keys, webhook secrets, repository credentials, presigned queries, and secret values.

#### Scenario: Request fails with a signed URL
- **WHEN** an object-store operation reports an error containing a presigned URL
- **THEN** logs and metrics contain only a safe operation classification and no query credential

### Requirement: Secret-safe searchable build logs
Configured credentials, secret values, presigned query credentials, and protected wrappers SHALL be redacted before build-log text enters either immutable object storage or a search projection. Search snippets, diagnostics, index failures, and rebuild logs SHALL apply the same protection and SHALL NOT expose unredacted source text.

#### Scenario: Job output contains a configured secret
- **WHEN** the agent or server redaction boundary recognizes protected material in stdout or stderr
- **THEN** the archived chunk and every search result contain only the redacted representation

### Requirement: Agent and server telemetry
The server SHALL accept bounded protocol-defined agent telemetry and collect server metrics using stable names, units, labels, and cardinality limits. Telemetry failure SHALL NOT invalidate an otherwise valid heartbeat, event append, or terminal completion.

#### Scenario: Metrics backend is unavailable
- **WHEN** the configured telemetry exporter cannot accept data
- **THEN** build coordination continues, bounded telemetry loss is reported, and memory usage remains limited

### Requirement: Durable restart recovery
After restart, the server SHALL reconstruct schedules, trigger work, pipeline state, queued jobs, builds and attempts, current leases, event cursors, log-chunk manifests, pending uploads, cache sessions, drain state, idempotency outcomes, and outbox records from the authoritative store and SHALL fence or expire ambiguous ownership before reassignment.

#### Scenario: Server restarts during a running job
- **WHEN** the agent reconnects with the current registration and fence before lease expiry
- **THEN** the server resumes heartbeat, event, and completion handling without duplicating the job or attempt

### Requirement: Rebuildable log-search projection
Build-log search SHALL be a derived projection behind a backend-neutral `LogSearchIndex` port. The initial PostgreSQL adapter SHALL provide full-text search using the `simple` text-search configuration and GIN indexing plus bounded literal-fragment search suitable for error codes, paths, and hashes. Indexing SHALL consume durable outbox work, expose lag, retry idempotently, and rebuild from committed redacted chunks without blocking coordination when the projection is unavailable.

#### Scenario: Search projection is lost
- **WHEN** an operator starts a rebuild from retained committed log chunks
- **THEN** the server recreates equivalent searchable documents without changing event cursors, Job outcomes, or archived bytes

#### Scenario: Index worker repeatedly fails
- **WHEN** the PostgreSQL search projection cannot accept updates
- **THEN** event ingestion, heartbeat and valid completion continue, indexing work remains durable, and in-memory retry state remains bounded

### Requirement: Signing and agent credential lifecycle
The server SHALL support overlap-based rotation of JobSpec signing keys and independent expiry and revocation of agent enrollment and registration credentials.

#### Scenario: Signing key rotates
- **WHEN** a new signing key is activated while the previous public key remains trusted by agents
- **THEN** new JobSpecs use the new key and already leased jobs remain verifiable during the overlap window

### Requirement: Audited mutations
Every accepted management, agent, trigger, adapter, and worker mutation SHALL append an immutable audit fact in the same authoritative transaction as the state change. Records SHALL identify the available actor kind, operation, target, request or idempotency identity, timestamp, and outcome without recording sensitive fields.

#### Scenario: Unauthenticated operator cancels a build
- **WHEN** a trusted-network management request cancels a build
- **THEN** the audit record identifies an unauthenticated management actor, request identity, operation and build without inventing an authenticated principal

### Requirement: Graceful bounded shutdown
Shutdown SHALL stop accepting new mutating requests and leases, allow in-flight transactions to finish within a configured deadline, preserve durable ownership, and exit without silently acknowledging uncommitted work.

#### Scenario: Shutdown deadline expires
- **WHEN** an operation cannot finish within the configured shutdown deadline
- **THEN** the server terminates the operation without publishing a success response and recovery can safely retry it

### Requirement: Backup and restore procedure
The release SHALL document and test a consistent backup and restore procedure covering authoritative metadata, immutable log chunks and other object-store objects, server configuration, signing keys, and provider credential references. The search projection MAY be restored or rebuilt, but readiness and reported freshness SHALL remain honest until it catches up.

#### Scenario: Restored deployment starts
- **WHEN** an operator restores a matched database and object-store snapshot using the documented procedure
- **THEN** the server validates consistency before becoming ready and does not revive expired leases as current
