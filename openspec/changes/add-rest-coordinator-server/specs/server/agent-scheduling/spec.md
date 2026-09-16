## Purpose

Defines safe placement of ready jobs from one global durable queue onto pooled outbound-only agents using registration epochs, capability matching, leases, and fencing.

## ADDED Requirements

### Requirement: Registration epochs
The server SHALL authenticate agent enrollment, validate complete inventory and configured pool membership, and issue a new opaque registration epoch for each process registration so requests from replaced processes can be rejected. Every registered agent SHALL belong to exactly one agent pool.

#### Scenario: Agent process restarts
- **WHEN** an enrolled agent registers again with the same stable agent identifier
- **THEN** the new registration supersedes the prior epoch and later requests from the old epoch are rejected

### Requirement: Durable agent pools
The server SHALL manage versioned agent pools with stable identities, enabled and drain state, admission policy, concurrency, and static capacity policy. Pool deletion SHALL be prohibited while agents, active leases, build configurations, or queued jobs reference it.

#### Scenario: Pool is drained
- **WHEN** an operator drains an agent pool
- **THEN** no new job is assigned to any agent in the pool while active leases follow the requested graceful or forced drain policy

#### Scenario: Agent changes pools
- **WHEN** an operator reassigns an idle agent to another compatible pool
- **THEN** the next registration uses the new pool and no active lease silently changes ownership or policy

### Requirement: One global durable ready-job queue
All pipeline jobs that become ready SHALL enter one durable logical queue. Each queue entry SHALL retain immutable allowed-pool and capability requirements, priority, enqueue time, project lineage and concurrency scope. The server SHALL NOT enqueue dependency-blocked jobs or create independent queue truth per agent or process replica.

#### Scenario: Several pools can run a job
- **WHEN** a ready job allows more than one pool and compatible agents poll concurrently
- **THEN** exactly one agent acquires the job and the selected pool is recorded with the lease

#### Scenario: Job is still blocked by its DAG
- **WHEN** a job has an incomplete required dependency
- **THEN** the placement scheduler cannot observe or lease that job

### Requirement: Transactional capability placement
The placement scheduler SHALL select only ready jobs whose allowed pools include the polling agent's pool and whose labels, platform, runtime mode, isolation, runner protocols, plugins, source capabilities, resource requirements, and cache requirements are satisfied by that accepting agent. One job SHALL have at most one current lease.

#### Scenario: Concurrent agents poll
- **WHEN** two matching agents concurrently request work
- **THEN** a ready job is leased to at most one registration epoch

### Requirement: Deterministic fairness and concurrency
Queue selection SHALL apply explicit priority, project and build-configuration concurrency limits, configurable pool fairness policy, enqueue time, and stable job identity in a documented deterministic order. A blocked or incompatible job SHALL NOT cause head-of-line blocking for unrelated compatible jobs.

#### Scenario: One project reaches its concurrency limit
- **WHEN** the highest-priority ready job belongs to a project at its effective concurrency limit
- **THEN** the scheduler may lease the next compatible job without changing the blocked job's durable priority

#### Scenario: Agent is under disk pressure
- **WHEN** an agent polls with `accept_jobs` false
- **THEN** the server returns no work or drain and does not assign a lease

### Requirement: Expiring fenced leases
Every assignment SHALL include an expiry and opaque fencing token. Renewal, cancellation, drain, event append, cache authority, output upload, and terminal completion SHALL require the current registration and fence.

#### Scenario: Stale agent submits data
- **WHEN** an older attempt owner sends a fenced mutation after ownership has changed
- **THEN** the server rejects it with the stable `lease_fenced` outcome and changes no attempt data

### Requirement: Independent heartbeat control
Heartbeat handling SHALL renew eligible leases and return exactly one of continue, cancel, fenced, or drain. Failure of event ingestion SHALL NOT prevent a valid heartbeat from extending a lease.

#### Scenario: Event pipeline is backpressured
- **WHEN** an active agent continues heartbeating while event ingestion is temporarily unavailable
- **THEN** the server independently evaluates and renews or terminates the lease

### Requirement: Ordered durable event ingestion
The server SHALL insert attempt events idempotently by job and stream sequence, accept only contiguous bounded batches, and acknowledge only the greatest contiguous sequence durably stored.

#### Scenario: Agent replays a batch
- **WHEN** an agent repeats a previously stored event batch after losing an acknowledgement
- **THEN** the server returns the same contiguous acknowledgement without duplicating events

### Requirement: Log acknowledgement preserves archive and search causality
For stdout and stderr events assigned to an immutable log chunk, the server SHALL acknowledge their sequence only after the verified chunk manifest, contiguous cursor, and durable search-index outbox record commit together. Object bytes SHALL be written idempotently before that transaction so committed metadata never names a missing or unverified chunk. Search-index completion itself SHALL NOT be required for acknowledgement.

#### Scenario: Search indexing is unavailable
- **WHEN** a verified redacted log chunk and its manifest commit while the search index worker is unavailable
- **THEN** the server acknowledges the contiguous events, retains durable indexing work, and reports search lag without blocking heartbeat or Job completion

#### Scenario: Agent replays an archived log batch
- **WHEN** the agent repeats events whose chunk manifest and cursor already committed after losing an acknowledgement
- **THEN** the server returns the same acknowledgement without creating another visible chunk or search document

### Requirement: Terminal completion after acknowledged history
The server SHALL accept one idempotent terminal completion only from the current lease owner and only when its declared final event sequence is already durable.

#### Scenario: Completion races missing events
- **WHEN** an agent attempts completion before the declared final event sequence is stored
- **THEN** the server rejects completion as retryable and leaves the job active

### Requirement: No arbitrary agent commands
The server SHALL send agents only protocol-defined leases, heartbeat directives, timing policy, and bounded credentials associated with an owned lease.

#### Scenario: Operator needs maintenance
- **WHEN** an operator drains an agent
- **THEN** the server expresses drain through the documented coordination protocol rather than sending a shell command
