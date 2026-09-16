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
One immutable accepted execution request, including its resolved source, configuration, policy, parameters, and initiating Trigger; it owns the history of all its Attempts.
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

## Triggering and coordination

**Trigger**:
A normalized cause that requests evaluation of a Build Configuration, regardless of whether the cause is manual, scheduled, external, or internal.
_Avoid_: Scheduler, webhook

**Trigger Occurrence**:
One durably deduplicated instance of a Trigger with a stable causal identity and target Build Configuration.
_Avoid_: Delivery, Build

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
One produced file or machine-readable report within a Build Result, addressed by logical identity and immutable content identity.
_Avoid_: Build Result, object-store object

**Build Log**:
The ordered redacted stdout and stderr history of Jobs in a Build Result.
_Avoid_: Server log, audit log
