## MODIFIED Requirements

### Requirement: Durable restart recovery
After restart, the server SHALL reconstruct schedules, trigger work, pipeline state, queued jobs, builds and attempts, Factory Configurations, Work Envelopes, Factory Runs, Stage Attempts, current stage claims and budgets, ChangeSets, Evidence Manifests, Evaluation Plans, Assessments, Decisions, delivery and reporting work, current leases, event cursors, log-chunk manifests, pending uploads, cache sessions, drain state, idempotency outcomes, and outbox records from the authoritative store and SHALL fence or expire ambiguous ownership before reassignment. Recovery SHALL require no worktree, model session, in-memory transcript, process-local timer, or provider-local hidden state.

#### Scenario: Server restarts during a running job
- **WHEN** the agent reconnects with the current registration and fence before lease expiry
- **THEN** the server resumes heartbeat, event, and completion handling without duplicating the job or attempt

#### Scenario: Server restarts between Factory stages
- **WHEN** a server replica stops after committing a Stage Attempt result but before dispatching the next stage
- **THEN** a later reconciler derives exactly one permitted next action from persisted Factory state and input digests

#### Scenario: Model session disappears
- **WHEN** an implementation or evaluator process and resumable provider session are lost after a typed result was committed
- **THEN** recovery continues or retries from exact revisions, immutable task inputs, typed results, budgets, and artifact references without treating session loss as lost authoritative state

## ADDED Requirements

### Requirement: Factory reconciliation is supervised and bounded
Factory admission, reconciliation, evaluation planning, decision, delivery, and reporting workers SHALL be supervised dependencies with configurable bounded concurrency, claim duration, retry delay, batch size, WIP, and shutdown behavior. Process-local wakeups and circuit breakers MAY control load but SHALL NOT authorize or reconstruct a Factory transition. Readiness SHALL distinguish mandatory factory dependencies from optional disabled Factory mode.

#### Scenario: Factory mode is disabled
- **WHEN** a deployment has no enabled Factory Configuration or factory worker requirement
- **THEN** unavailable model, evaluator, and delivery providers do not make ordinary CI/CD readiness fail

#### Scenario: Required factory worker repeatedly fails
- **WHEN** an enabled deployment cannot safely progress admitted Factory Runs because a mandatory supervised factory worker is unavailable
- **THEN** readiness and operator diagnostics report the degraded factory capability while liveness and already-safe CI/CD observation remain honest

### Requirement: Factory audit and secret-safe observability
Every accepted Work admission, Factory claim, Stage Attempt, Build linkage, candidate capture, evidence publication, evaluation plan, Assessment receipt, Decision, rework, escalation, cancellation, delivery, and reporting mutation SHALL append an immutable audit fact in the same authoritative transaction as its state change. Structured logs, metrics, and traces SHALL expose bounded lifecycle state, latency, usage, retries, budget, fences, connector and policy digests, and failure classifications while excluding task bodies, prompts, raw findings, paths of unbounded cardinality, provider credentials, secrets, private delivery capabilities, and hidden model reasoning.

#### Scenario: Factory decision is committed
- **WHEN** the Decision Engine accepts, reworks, escalates, rejects, or cancels a candidate
- **THEN** the transaction records the actor, Factory Run, Stage Attempt, exact subject and evidence digests, policy version, reasons, request identity, and outcome without storing a secret or raw model transcript in audit

#### Scenario: Provider error contains sensitive material
- **WHEN** a model, evaluator, source, or delivery adapter returns an error containing a credential, private URL, prompt fragment, or repository-sensitive text
- **THEN** operational logs and metrics contain only the redacted stable classification and safe correlation identity

### Requirement: Factory state participates in backup and retention
Backup and restore procedures SHALL cover Factory Configurations, Work Envelopes, Factory Runs, Stage Attempts, claims, budgets, ChangeSets, evidence, Assessments, Decisions, delivery identities, audit facts, and referenced retained objects consistently with the Build and Artifact stores. Retention SHALL preserve the exact candidate, evidence, decision, and delivery provenance required by any visible non-terminal, escalated, or held Factory Run and SHALL safely retry partial cleanup.

#### Scenario: Deployment is restored with active Factory Runs
- **WHEN** matched authoritative metadata and object snapshots are restored
- **THEN** the server validates their referenced digests, expires ambiguous ownership, and can reconcile each non-terminal run without reviving an obsolete claim
