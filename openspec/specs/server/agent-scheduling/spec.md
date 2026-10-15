# Agent Scheduling Specification

## Purpose

Defines safe placement of ready jobs from one global durable queue onto pooled outbound-only agents using registration epochs, capability matching, leases, and fencing.

## Requirements

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
The placement scheduler SHALL select only ready jobs whose allowed pools include the polling agent's pool and whose labels, host and target platforms, execution mode, isolation guarantees, runner protocols, plugins, source capabilities, resource requirements, and cache requirements are satisfied by that accepting agent. Execution mode SHALL be one of provider-neutral `host`, `isolation`, or `virtualization`; concrete backend names SHALL remain Agent configuration and diagnostics rather than placement requirements. One job SHALL have at most one current lease.

#### Scenario: Concurrent agents poll
- **WHEN** two matching agents concurrently request work
- **THEN** a ready job is leased to at most one registration epoch

#### Scenario: Backend satisfies semantic mode
- **WHEN** two Agents implement the same requested execution mode through different qualified backends
- **THEN** either Agent may accept the Job if its advertised target platform and required guarantees also match

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

### Requirement: Factory permission capability placement
Placement for a Factory-owned Job SHALL require an Agent and execution backend that advertise the protocol and enforcement capabilities needed by the immutable Factory Permission Set, including plugin and executable identity, command and argument policy, descendant-process control, filesystem and mount policy, network policy, secret and workload-identity delivery, resources, and output bounds. Capability matching SHALL remain provider-neutral and SHALL NOT select an Agent merely because it can start the requested executable.

#### Scenario: Agent lacks one required enforcement capability
- **WHEN** a ready Factory-owned Job requires a filesystem, network, process, or identity restriction that an otherwise compatible Agent cannot enforce
- **THEN** that Agent cannot acquire the Job and unrelated compatible work remains schedulable

#### Scenario: Equivalent qualified backends exist
- **WHEN** different Agent backends prove the same required semantic permission capabilities and execution guarantees
- **THEN** either may accept the Job without exposing its concrete backend name in factory policy

### Requirement: Pre-spawn permission revalidation
The Agent SHALL verify the signed Factory Permission Set, pinned plugin and executable identity, local grants, resolved paths, mounts, network hosts, secret and workload-identity profiles, resources, outputs, and selected backend capabilities immediately before source or process activity. Every effective permission SHALL be no broader than the signed Job intent and local policy. Failure SHALL be typed, secret-safe, and terminal for that execution attempt unless server policy explicitly classifies a provider-independent retry.

#### Scenario: Local grant changed after placement
- **WHEN** a Job is leased but a required local tool, path, profile, or backend grant is no longer available before spawn
- **THEN** the Agent rejects execution before revealing source, prompt, or credentials and reports the missing capability without falling back to Host

#### Scenario: Repository requests an extra permission
- **WHEN** repository content or a harness request attempts to add a command, argument, path, mount, host, child process, secret, or output outside signed intent
- **THEN** the Agent or backend denies that operation and cannot persist the requested widening as a new grant

### Requirement: Optional tool-risk assessment cannot widen execution authority
Before any optional Decision Signal assessment, a model-proposed tool or command action SHALL be intercepted by a trusted blocking pre-execution hook and normalized into bounded executable identity, arguments, resolved paths, network destinations, credential profiles, descendants, resources, and expected outputs. An Agent-local authorization broker bound to the current Job, lease and fence SHALL apply the signed Factory Permission Set, Task Envelope, Project policy, local Agent grants, backend capabilities, and deterministic hard-deny rules before the action can execute. Deterministic hard-allow rules MAY authorize an in-envelope action without a provider call. Only an ambiguous action already inside every authoritative envelope MAY be sent as a redacted `tool_risk` request to a server-side Decision Signal provider. Its consuming code policy MAY allow the unchanged action, deny it, or escalate, but the signal SHALL NOT add or broaden any authority. The Agent and backend SHALL revalidate and enforce the final unchanged action immediately before the protected operation. Provider credentials SHALL NOT be delivered to the Agent workload or harness.

#### Scenario: Proposed command is outside the whitelist
- **WHEN** a harness proposes a command, argument, path, host, credential, descendant, resource, or output not present in the effective permission intersection
- **THEN** deterministic policy denies it before any Decision Signal request and no provider response can authorize it

#### Scenario: Ambiguous in-envelope action is assessed
- **WHEN** an action fits every authoritative permission boundary but is not covered by deterministic hard-allow or hard-deny policy
- **THEN** the configured exact `tool_risk` provider and policy may assess the normalized redacted action, after which code applies its threshold and fail-closed disposition

#### Scenario: Tool-risk provider is unavailable
- **WHEN** bounded control requires a tool-risk signal but its provider is unavailable, invalid, or below the configured confidence floor
- **THEN** the action is denied or escalated according to immutable policy and execution does not fall back to an unassessed allow

#### Scenario: Harness lacks a blocking tool hook
- **WHEN** a Factory Job requires `tool_risk` bounded control but its pinned plugin or harness cannot prove the required pre-execution interception capability
- **THEN** placement or pre-spawn validation rejects the Job and no action runs through an advisory or best-effort fallback

#### Scenario: Tool authorization loses its fence
- **WHEN** the Job is cancelled, its lease or fence becomes stale, the authorization broker disconnects, or the returned receipt does not match the blocked proposal
- **THEN** the pending action is denied and cannot execute using a disposition from another Job or proposal

### Requirement: Factory secrets remain stage scoped
Model, evaluator, source, and delivery credentials SHALL use distinct logical profiles and SHALL be delivered only to the trusted component and Stage Attempt authorized to consume them. Coding and evaluation execution SHALL NOT receive a delivery credential, and a delivery adapter SHALL NOT receive model session credentials or unredacted model traces.

#### Scenario: Coding stage requests delivery identity
- **WHEN** a coding Task Envelope or child process attempts to resolve the configured forge delivery profile
- **THEN** secret resolution is denied before value materialization and the denial is recorded without exposing the credential
