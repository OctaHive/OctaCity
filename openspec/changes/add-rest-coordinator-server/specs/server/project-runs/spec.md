## Purpose

Defines hierarchical projects, immutable pipeline DAGs, durable build triggers, and orchestration from reusable configuration through build, attempt, job, and signed execution intent.

## ADDED Requirements

### Requirement: Hierarchical durable projects
The server SHALL store projects as versioned resources with an optional parent project, stable identity, sibling-unique name, and acyclic ancestry. A project MAY contain child projects and build configurations at the same time.

#### Scenario: Nested project is created
- **WHEN** an operator creates a project below an existing project
- **THEN** the server assigns it a stable identifier and exposes its ancestry and children without deriving identity from its mutable display path

#### Scenario: Project cycle is requested
- **WHEN** an operator attempts to move a project below itself or one of its descendants
- **THEN** the server rejects the mutation and preserves the existing hierarchy

### Requirement: Explicit inherited project policy
The server SHALL resolve documented inheritable policy from root to leaf, store an immutable effective-policy snapshot with each build, and require explicit child overrides. Inheritable policy SHALL include allowed agent pools, repository access, secret and workload-identity profiles, runtime policy, cache namespace policy, output limits, concurrency, and retention where configured.

#### Scenario: Child narrows inherited pools
- **WHEN** a child project restricts its inherited allowed-pool set
- **THEN** builds below that child can be scheduled only to the effective restricted set

#### Scenario: Parent policy changes after a build starts
- **WHEN** an operator changes inherited project policy after a build has been accepted
- **THEN** the accepted build and all of its retries continue to use their recorded effective-policy snapshot

### Requirement: Versioned pipeline DAGs
A pipeline SHALL be an immutable versioned directed acyclic graph of job templates and explicit dependencies. The server SHALL reject cycles, missing dependencies, duplicate node identities, invalid fan-in or fan-out policy, and references to unavailable execution capabilities before a pipeline version becomes usable.

#### Scenario: Pipeline contains a cycle
- **WHEN** an operator submits a pipeline in which one job transitively depends on itself
- **THEN** the server rejects the pipeline version without changing the currently active version

#### Scenario: Pipeline version changes
- **WHEN** an operator publishes a new valid pipeline version
- **THEN** subsequent builds may use the new version while existing builds and retries retain their recorded pipeline snapshot

### Requirement: Versioned build configurations
A build configuration SHALL belong to exactly one project and store a sibling-unique name, enabled state, repository selection, pipeline version, parameter schema and defaults, trigger policy, agent requirements, allowed pool restriction, runtime policy, cache policy, output policy, and retry policy. Each accepted mutation SHALL create a new configuration version without rewriting builds created from an older version.

#### Scenario: Build configuration is updated
- **WHEN** an operator changes a configuration's pipeline or execution policy using its current version precondition
- **THEN** subsequent builds use the new configuration version while existing builds retain the prior snapshot

#### Scenario: Disabled configuration is triggered
- **WHEN** a manual, scheduled, external, or internal trigger targets a disabled build configuration
- **THEN** the server records a suppressed trigger outcome without creating queued work

### Requirement: Durable normalized triggers
The server SHALL accept manual commands, persisted schedules, authenticated external events, and server-generated internal events through one normalized trigger model. Each trigger occurrence SHALL carry a stable deduplication identity, cause, source time, target configuration, and bounded provider metadata.

#### Scenario: Scheduled occurrence is claimed after restart
- **WHEN** a due schedule was not processed before the server stopped
- **THEN** a later server instance durably claims it according to the configured missed-run policy and creates at most one build

#### Scenario: Internal event creates downstream work
- **WHEN** a terminal build event matches an enabled internal trigger
- **THEN** the server creates at most one causally linked downstream build and prevents an undocumented trigger cycle

### Requirement: Source-scoped inter-Build dependencies
An internal Build-completion Trigger SHALL identify an exact upstream Build Configuration version, one terminal outcome to match, and an exact downstream Build Configuration version. It SHALL NOT match terminal events from unrelated Build Configurations solely because they have the same outcome. The downstream Build SHALL retain the upstream Build and Trigger occurrence in its durable causality, and one upstream Build SHALL create at most one downstream Build per matching Trigger version.

The Trigger SHALL select the downstream source explicitly: either inherit the upstream Build's exact immutable revision when both configurations use the same compatible Repository identity, or resolve an allowed source expression against the downstream configuration. The server SHALL reject an incompatible inherited-revision definition before enabling it. Constant downstream parameters MAY be configured, but arbitrary expressions over upstream logs, outputs, or artifacts SHALL NOT be evaluated by the initial inter-Build dependency contract.

#### Scenario: Successful upstream Build starts its dependent Build
- **WHEN** the selected upstream Build Configuration version reaches `succeeded` and matches an enabled success Trigger
- **THEN** the server creates exactly one causally linked Build for the selected downstream Build Configuration version despite delivery retries or server restart

#### Scenario: Unrelated Build succeeds
- **WHEN** another Build Configuration reaches `succeeded`
- **THEN** the source-scoped Trigger does not create the downstream Build

#### Scenario: Downstream Build inherits the upstream revision
- **WHEN** a matching Trigger selects revision inheritance and both configurations reference the same compatible Repository identity
- **THEN** the downstream Build records the exact immutable revision used by the upstream Build without resolving a mutable branch again

#### Scenario: Inter-Build chain contains a causal cycle
- **WHEN** configured or versioned internal Triggers would cause a Build occurrence to revisit an ancestor Trigger or exceed the documented depth limit
- **THEN** the server suppresses the unsafe occurrence durably without creating unbounded Builds

### Requirement: Immutable build input
Creating a build SHALL bind the project, build-configuration version, pipeline version, effective project policy, repository, exact immutable source revision, parameters, trigger occurrence, priority, and creation cause into an immutable build record.

#### Scenario: Manual build is accepted
- **WHEN** a client submits valid parameters and an exact revision for an enabled build configuration
- **THEN** the server durably creates the build, its first attempt, the jobs materialized from its pipeline, and the initially ready queue entries

#### Scenario: Mutable revision is unresolved
- **WHEN** a trigger names a branch or tag that cannot be resolved to an immutable revision
- **THEN** the server rejects or defers the build without leasing ambiguous source state

### Requirement: Attempts materialize pipeline jobs
Every build SHALL have monotonically numbered attempts. Each attempt SHALL materialize immutable jobs and dependency edges from the build's pipeline snapshot. Only jobs whose dependencies have satisfied their declared success policy SHALL become ready for placement.

#### Scenario: Root jobs become ready
- **WHEN** the server creates an attempt for a valid pipeline
- **THEN** jobs without dependencies become ready while dependent jobs remain blocked

#### Scenario: Dependency fails
- **WHEN** a job reaches a failed terminal state
- **THEN** the orchestrator applies the recorded pipeline failure policy to dependent jobs without mutating the pipeline snapshot

### Requirement: Server-signed JobSpec
The server SHALL persist stable server-derived JobSpec intent from validated server policy, immutable build input, and the pipeline job template. It SHALL add the concrete Job identity, Attempt number, validity window, and active-key signature atomically whenever that Job transitions to ready, before queue visibility. Blocked Jobs SHALL NOT retain a prematurely issued envelope. A repository or management caller SHALL never supply a signed envelope, secrets, host paths, fencing values, upload credentials, local repository paths, traversal locators, or credential-bearing repository locators inside this intent.

#### Scenario: Job becomes ready
- **WHEN** the orchestrator determines that a job's dependencies are satisfied
- **THEN** the resulting signed JobSpec is bound to that build, attempt and job and passes the shared protocol validator before queue visibility

#### Scenario: Blocked job waits longer than one signing window
- **WHEN** a dependent job remains blocked beyond the validity window used by an earlier ready job
- **THEN** the dependent job has no stale envelope and receives a new envelope using the current signing key and readiness time in the transaction that unblocks it

#### Scenario: Lost manual-trigger response is retried
- **WHEN** a client repeats the same manual trigger identity with a later transport or observation timestamp after losing the first response
- **THEN** the server replays the existing Build before resolving mutable VCS state because stable intent and the idempotency fingerprint exclude processing timestamps, resolved revisions, and short-lived signatures

### Requirement: Durable pipeline orchestration
The server SHALL advance build, attempt, and job state through explicit state machines driven by persisted commands and events. The server SHALL orchestrate dependencies but SHALL NOT execute repository-controlled build steps.

#### Scenario: Job completes successfully
- **WHEN** the current agent lease publishes an accepted terminal success
- **THEN** the orchestrator durably marks newly unblocked jobs ready and derives the attempt state from the complete DAG

### Requirement: Explicit cancellation and retry
Cancellation SHALL be a durable idempotent build intent propagated to every non-terminal job. Retry SHALL create a new positive attempt number from the same immutable build, pipeline, placement, dependency-policy, and stable JobSpec-intent snapshots while preserving prior attempt and output history. One shared core decision SHALL define retry graph equivalence for every store adapter.

#### Scenario: Running pipeline is cancelled
- **WHEN** a client cancels a build with queued, blocked, and running jobs
- **THEN** no blocked job becomes ready, queued jobs are withdrawn, and owning agents receive cancellation through heartbeat directives

#### Scenario: Failed build is retried
- **WHEN** a client retries a terminal failed build
- **THEN** the server materializes a new attempt from the recorded snapshots without mutating the completed attempt

#### Scenario: Retry changes root policy or execution intent
- **WHEN** a retry candidate changes a root Job dependency policy, source locator, runtime policy, or any other stable JobSpec input
- **THEN** every store adapter rejects the retry as a conflict before inserting the new Attempt

### Requirement: Queryable state and causality
Build, attempt and job reads SHALL expose current state, pipeline dependency state, timestamps, initiating cause, trigger identity, configuration and effective-policy versions, queue position inputs, selected pool, assigned agent when present, terminal outcome, event cursor, output references, and failure classification without exposing credentials or private signed URLs. Execution and infrastructure failure classes SHALL be typed completion input, persisted with the terminal completion, and projected from that authoritative fact rather than accepted from a read DTO.

#### Scenario: Client diagnoses a failed build
- **WHEN** a client reads a failed attempt
- **THEN** the response identifies the failed or skipped DAG nodes, their causal dependencies, failure classifications, events, and outputs
