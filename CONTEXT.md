# OctaCity Coordination

OctaCity coordinates reproducible CI execution from an initiating cause to
durable results. This glossary separates build creation, DAG progression, Job
placement, and execution so each term has one meaning across the product.

## Configuration and execution

**Project**:
A durable hierarchy node that scopes configuration, policy, and retained execution history.
_Avoid_: Folder, namespace

**Build Configuration**:
A versioned reusable definition that binds a Project, repository selection, Pipeline version, parameters, triggers, and execution policy.
_Avoid_: Build, Pipeline

**Pipeline**:
A reusable directed acyclic graph of Job templates and dependency policies.
_Avoid_: Workflow, Build

**Pipeline Version**:
An immutable published snapshot of a Pipeline DAG.
_Avoid_: Current Pipeline

**Build**:
One immutable accepted execution request, including its resolved source, configuration, policy, parameters, and immutable origin (a Trigger Occurrence or an exact Factory node operation); it owns the history of all its Attempts.
_Avoid_: Run, Attempt

**Attempt**:
One positively numbered materialization of a Build's Pipeline into Jobs; retry creates a new Attempt without rewriting an earlier one.
_Avoid_: Retry, Build

**Job**:
The smallest server-scheduled execution unit, materialized from one Pipeline node; a Job may contain multiple Octa tasks.
_Avoid_: Task, Pipeline node

**JobSpec**:
The immutable signed execution intent that authorizes an Agent to execute one Job.
_Avoid_: Job, lease

## Tasks and Factory execution

**Issue (task)**:
Persistent Project-owned intent with one stable internal identity and a Project-local human key. Manual creation needs no provider, repository or Factory configuration. An Issue can retain several independently admitted executions.
_Avoid_: Work Envelope, Run, Job

**Issue revision**:
One immutable requirements/business-metadata version. A separate applicability version fences whether an accepted execution may still apply a configured business transition. Expired text leaves the version and provenance available.
_Avoid_: Latest execution, mutable Work

**Issue workflow**:
A pinned Project-owned status vocabulary with finite permitted direct transitions and one initial status. Adoption of another version is explicit. Business status neither establishes Factory acceptance nor grants delivery authority.
_Avoid_: Factory Flow, Pipeline DAG

**Issue execution request**:
An explicit durable review of one Issue revision, exact Factory target and frozen input digests. Publication uses the common Project-text intake. Recovery retains the original request, Work and Run identities.
_Avoid_: Issue edit, automatic rerun

**Work Envelope**:
One immutable admitted Factory input, owning exactly one Factory Run. Distinct assessment and implementation inputs retain distinct Work identities even when associated with the same Issue.
_Avoid_: Editable task, latest requirements

**Factory Run**:
The technical execution and control history of one admitted Work Envelope. Nested Flows, attempts, retries and clarification remain children of that original execution.
_Avoid_: Business status, Build Attempt

**Factory Role (AI agent)**:
A Project-owned, versioned responsibility with a display name, icon, description, ordered shared instructions and concurrency ceiling. Console calls it an AI agent; it is independent of enrolled CI/CD workers.
_Avoid_: Worker Agent, Agent Pool, runtime installation

**Role Profile**:
An immutable version of one AI agent's complete execution settings, pinned to an exact Role version and qualified installed runtime/model, instructions, skills, environment, external bindings and resource ceilings.
_Avoid_: Performance tier, live inheritance, worker configuration

**Skill bundle**:
An immutable version of one explicitly selected skill folder, containing its root `SKILL.md` and reviewed supporting files. Its content describes behavior but grants no execution or credential authority.
_Avoid_: Ambient workstation folder, installed software, permission grant

**Profile selection**:
The retained choice of one pinned compatible profile for an exact stage occurrence, made manually, by ordered rules or by a qualified Decision Signal. A recorded control intent is distinct from a consumed choice and dispatched execution.
_Avoid_: Routing edge, stage completion, automatic model upgrade

**Execution package**:
The immutable effective invocation containing controlling Project rules, exact Role/Profile and skill versions, task, Context and execution settings. Recovery retains that invocation instead of adopting current defaults.
_Avoid_: Current catalog, mutable profile, deployment image

**External source link**:
The optional primary verified native ticket association in its Project/security/installation/resource/item namespace. Field ownership and external-status mapping are separately reviewed; a link alone authorizes no synchronization.
_Avoid_: URL-derived authority, PR/MR association

**Issue discussion**:
Append-only operator text with an Issue-local lifetime sequence. A comment cannot trigger execution, retry, clarification, status movement or acceptance.
_Avoid_: Agent response, evidence

**Content retention plan**:
An explicitly administered cutoff, deadline and optional hold for either requirements or discussion bodies. Each kind has its own version. Execution inputs, Artifacts, source snapshots and reporting keep their original owners and lifetimes; body expiry does not remove identity, provenance or replay fences.
_Avoid_: Run retention, quota reset

## Triggering and coordination

**Trigger**:
A versioned rule that permits one normalized origin kind to target an exact Build Configuration version.
_Avoid_: Trigger Occurrence, Scheduler, webhook

**Trigger Cause**:
Provider-neutral facts explaining why a Trigger Occurrence exists; its origin kind is manual, scheduled, external, or internal.
_Avoid_: Provider payload, Trigger definition

**Trigger Occurrence**:
One normalized instance of a Trigger with a source-scoped deduplication identity, exact target Build Configuration, source time, and stable causal lineage.
_Avoid_: Delivery, Build

**Deduplication Identity**:
A stable identity assigned by one Trigger source and scoped to an exact Trigger version; retries retain it even when their occurrence IDs differ.
_Avoid_: Occurrence ID, idempotency key

**Causal Lineage**:
The stable root occurrence, optional direct parent occurrence, and internal derivation depth carried by a Trigger Occurrence.
_Avoid_: Deduplication identity, Build dependency

**Trigger Engine**:
The domain role that evaluates Trigger Occurrences and creates at most one Build for each accepted occurrence.
_Avoid_: Scheduler, Orchestrator

**Orchestrator**:
The domain role that advances Build, Attempt, and DAG Job state from persisted outcomes and makes newly unblocked Jobs ready.
_Avoid_: Executor, Placement Scheduler

**Ready Job**:
A Job whose dependency policy is satisfied and which may be considered for placement.
_Avoid_: Running Job, leased Job

**Placement Scheduler**:
The domain role that selects a compatible accepting Agent for an already Ready Job and creates a fenced Lease.
_Avoid_: Trigger scheduler, Orchestrator, Executor

## Agents and execution

**Agent**:
An enrolled outbound worker that advertises verified execution capabilities and owns at most one active Job in the initial release.
_Avoid_: Runner, Executor

**Agent Pool**:
A stable policy and admission group to which each Agent belongs exactly once.
_Avoid_: Capacity provider, queue

**Lease**:
Time-bounded, fenced ownership of one Job by one current Agent registration.
_Avoid_: Job, assignment record

**Executor**:
The Agent-side role that executes one verified JobSpec and reports events and a terminal outcome; the server has no Executor for repository-controlled code.
_Avoid_: Orchestrator, runner

## Results

**Build Result**:
The logical aggregate of a Build's immutable execution configuration, ordered log and event history, and produced files and reports.
_Avoid_: Artifact, log archive

**Artifact**:
One immutable file or machine-readable document, addressed by logical identity and immutable content identity. It belongs either to a producing Build as part of its Build Result or to a Project as prepared Factory input.
_Avoid_: Build Result, object-store object

**Build Log**:
The ordered redacted stdout and stderr history of Jobs in a Build Result.
_Avoid_: Server log, audit log
