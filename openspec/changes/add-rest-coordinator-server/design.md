## Context

The repository already owns the agent-side coordination protocol, a single-job execution state machine, output validation, an S3-compatible artifact implementation, a cache-session client, a source-materialization plugin protocol, and released-agent packaging. The server must implement the control-plane side without importing agent orchestration or duplicating Octa's task, cache-key, or plugin semantics.

The server is a modular monolith: one deployable process, multiple crates with enforced dependency direction, PostgreSQL as the first authoritative state adapter, and S3-compatible storage as the first object adapter. The first management deployment is explicitly unauthenticated and restricted to a trusted network; agent credentials, webhook verification, JobSpec signing, and secret-provider authentication remain mandatory.

The workspace is also moving from a flat `crates/` inventory toward four product areas: `cli`, `server`, `agent`, and `shared`. This design defines ownership before physical moves occur so crates are not placed in `shared` merely for convenience.

## Goals / Non-Goals

**Goals:**

- Enforce one-way dependencies between API adapters, application/core modules, infrastructure adapters, and the composition root.
- Keep REST, future GraphQL, provider payloads, database rows, and domain types as distinct representations.
- Model pipelines as immutable DAGs and durably orchestrate multiple dependent jobs per attempt.
- Separate build triggering from ready-job placement.
- Put correctness-relevant mutations behind deep interfaces and commit audit and outbox facts atomically with state.
- Make PostgreSQL, S3-compatible storage, Git, webhook providers, secret providers, telemetry exporters, and future message brokers replaceable at explicit seams.
- Preserve the existing outbound-only agent model and one active job per agent.
- Support one server process first without making correctness depend on one process or in-memory timers.

**Non-Goals:**

- A Web UI, server-rendered pages, or a permanent WebSocket management protocol.
- Running repository code, Octa, builds, or untrusted plugins in the server process.
- Operator authentication, LDAP, TOTP, RBAC, organizations, billing, or hosted multi-tenancy in the first release.
- A production Kafka, NATS, Redis, vSphere, Proxmox, or other dynamic-capacity adapter in the first release.
- Treating Kafka or NATS as a drop-in replacement for authoritative transactional state.
- More than one concurrently executing job per agent.

## Decisions

### 1. Four product areas and an enforced layered server graph

The target workspace layout is:

```text
cli/
agent/
server/
  app/
  api/
  application/
  core/
  protocols/
  infrastructure/
shared/
```

`shared` contains only dependency-light contracts used by at least two product areas, such as the agent wire protocol, artifact transfer messages, wire-level opaque identifiers, and telemetry vocabulary. Server-only identifiers and value objects remain in server core even when several server crates use them. REST DTOs, SQL rows, provider payloads, and server domain entities do not belong in `shared`.

The server dependency direction is:

```text
REST / webhook / agent API adapters
                 |
                 v
      application commands and queries
                 |
                 v
       domain modules and their ports
                 ^
                 |
 infrastructure adapters implement ports

composition root --> every selected adapter exactly once
```

API adapters depend on application interfaces. Application handlers depend on core modules and ports. Infrastructure adapters depend on the ports they implement. Core and application crates never depend on Axum, SQLx, S3 SDKs, Git executables, LDAP SDKs, provider SDKs, or process configuration. The `octacity-server` binary is the only composition root.

Architecture tests inspect Cargo metadata and reject reverse dependencies, direct API-to-infrastructure dependencies, and server dependencies from agent crates.

Alternative considered: one server crate with internal modules. It reduces initial manifests but cannot enforce the requested dependency graph and makes it easier for HTTP, SQL, and provider details to leak into core behavior.

### 2. Crates represent deep modules, not tables

Initial server crates are grouped by role.

API and application:

- `octacity-server-api-rest`: management REST and build-log search DTOs, routing, bounded decoding, OpenAPI, and mapping to commands and queries;
- `octacity-server-api-agent`: existing agent-protocol HTTP routes;
- `octacity-server-api-webhook`: bounded raw webhook HTTP ingress and transport error mapping;
- `octacity-server-application`: typed CQRS commands, queries, handlers, transaction coordination, Build Result projections, and error mapping.

Core:

- `octacity-server-domain`: server-only ubiquitous value objects shared by core modules, including opaque identities, versions, bounded names, timestamps, source references, network hosts, runtime classes, output ceilings, trigger and Pipeline-node identities, and typed domain errors; it owns no aggregate behavior or infrastructure representation;
- `octacity-server-trigger`: manual, scheduled, external, and internal trigger normalization and deduplication rules;
- `octacity-server-pipeline`: immutable pipeline versions, DAG validation, dependency policy, and attempt materialization;
- `octacity-server-job`: server-side Job identity, state, requirements, and JobSpec construction inputs;
- `octacity-server-orchestrator`: build/attempt/DAG state machines and transition decisions;
- `octacity-server-scheduler`: placement of ready jobs onto accepting compatible agents;
- `octacity-server-secrets`: logical secret references, policy, and short-lived grant interfaces;
- `octacity-server-cache`: cache authority, namespace policy, and session lifecycle;
- `octacity-server-artifacts`: logical upload, publication, download, and retention state;
- `octacity-server-audit`: immutable audit facts and redaction classifications.

Protocols and infrastructure:

- `octacity-webhook-provider-protocol`: versioned webhook verification, normalization, and managed-registration process contract;
- `octacity-vcs-protocol`: versioned ref, commit, tree, content, and revision-resolution process contract;
- `octacity-agent-provisioning-protocol`: future provision/observe/terminate contract, without a production adapter in v1;
- `octacity-server-store`: atomic persistence interfaces, the authoritative `LogIndexWorkStore` watermark source, the backend-neutral `LogSearchIndex` projection port, and adapter-neutral contract tests;
- `octacity-server-store-postgres`: migrations, SQLx rows, transactions, locks, PostgreSQL full-text and literal log-search projection, and persistence implementation;
- `octacity-server-webhook`: verified webhook-provider registry and bounded process host;
- `octacity-server-vcs`: verified adapter registry and bounded process host;
- `octacity-vcs-git`: first VCS implementation;
- `octacity-artifact-store`: backend-neutral object capabilities and verification interface;
- `octacity-artifact-s3`: S3-compatible implementation;
- `octacity-observability`: stable metric/trace vocabulary, cardinality policy, and server/agent instrumentation helpers.

A crate is not created for a table or a thin forwarding interface. Each listed crate owns non-trivial invariants, a versioned protocol, a coherent server-wide semantic vocabulary, or an independently enforceable dependency direction. `octacity-server-domain` is deliberately limited to cross-cutting server value objects so it cannot become a miscellaneous utilities or aggregate-implementation crate. Future auth, GraphQL, Kafka/NATS, Vault, GitHub, Gerrit, vSphere, and Proxmox adapters are added only when implemented, not as empty crates.

### 3. CQRS belongs to the application layer

`octacity-server-application` owns typed commands and queries. Commands express intent and cross one transaction interface; queries use purpose-built read interfaces and return application projections rather than database rows. A generic reflection-based command bus is not required: compile-time typed handlers are preferred until dispatch requirements justify one.

REST maps HTTP DTOs to commands and queries and maps application outcomes back to stable HTTP errors. A future GraphQL adapter invokes the same application interfaces. Transport adapters do not open transactions, coordinate repositories, publish audit facts, or advance state machines themselves.

Command success is returned only after domain state, idempotency outcome, audit fact, and required outbox records commit together.

### 4. Triggering, orchestration, placement, and execution are distinct

These terms are canonical:

- **Trigger**: a versioned rule that selects a Build Configuration and matches manual, scheduled, external, or internal input.
- **Trigger occurrence**: one normalized, deduplicated instance that requests evaluation of a Build Configuration.
- **Trigger engine**: accepts manual commands, persisted schedules, authenticated external events, and internal domain events and creates at most one Build per Trigger occurrence.
- **Orchestrator**: advances Build, Attempt, and DAG state and makes dependency-blocked Jobs ready.
- **Placement scheduler**: leases an already-ready Job to a compatible accepting Agent.
- **Executor**: agent-side code that executes one signed JobSpec; the server has no executor for repository code.

This separation prevents cron calculation, webhook parsing, DAG traversal, and lease selection from becoming one shallow scheduler interface.

Scheduled triggers store schedule expression, timezone, next occurrence, missed-run policy, and deduplication identity durably. External events arrive only after webhook authentication and normalization. Internal triggers consume documented domain events through the same durable trigger path and enforce cycle/depth policy.

### 5. Pipelines are immutable DAGs; attempts materialize jobs

A Pipeline Version owns a validated DAG of job templates and dependency policies. A Build binds one Build Configuration Version, Pipeline Version, immutable source revision, parameters, and effective project policy. An Attempt materializes Jobs and dependency edges from those snapshots.

Root Jobs become ready immediately. Other Jobs remain blocked until the orchestrator observes the required predecessor outcomes. Success, failure, cancellation, retry, fan-in, and skipped-node behavior are explicit state-machine inputs. An agent still owns at most one Job at a time.

The existing `octacity-job` crate is agent-side execution orchestration and moves under the `agent` product area; it is not reused as the server Job domain model. Shared `JobSpecV1` remains the signed wire intent crossing from server to agent.

### 6. Deep store operations and an honest PostgreSQL adapter

`octacity-server-store` is not a generic CRUD repository. Its interface grows only when an implementing feature adds a complete application use case, an adapter operation, and a reusable contract test. Stage 2 establishes Trigger acceptance, ready-Job claim, event append, completion, Agent credential lifecycle, authoritative log-index watermark, and derived log-search operations. Later feature tasks add the following complete operations when they are implemented, rather than reserving speculative CRUD methods:

- mutate a project hierarchy with acyclicity and optimistic version checks;
- publish a pipeline/configuration version;
- accept one trigger occurrence and create a Build at most once;
- create an Attempt, materialize its DAG, and enqueue root Jobs;
- apply a Job terminal event and atomically make downstream Jobs ready;
- claim one pool-compatible ready Job and create its lease;
- renew, cancel, fence, or complete a lease;
- append a contiguous event batch and advance its cursor;
- commit immutable redacted log-chunk metadata, event sequence ranges, and the indexing outbox record before acknowledgement;
- search committed log projections through a bounded backend-neutral `LogSearchIndex` query;
- reserve and publish an artifact upload;
- begin and revoke a cache session;
- claim due schedules, outbox entries, retention work, and adapter retries.

The PostgreSQL adapter owns SQL schema knowledge, transactions, locking, and row conversion. Domain and application logic remains in Rust: migrations use declarative keys, foreign keys, uniqueness, nullability, checks, and indexes, but do not use stored routines or triggers to decide domain transitions, hierarchy validity, sequencing, policy, or lifecycle behavior. When a cross-row invariant cannot be expressed declaratively, the adapter serializes the complete store operation with transaction isolation, row locks, or a transaction-scoped advisory lock, loads the authoritative facts, invokes the same Rust decision used by the in-memory adapter, and commits the resulting writes atomically. Application tests use a deterministic in-memory adapter where useful; the same behavioral contract suite runs against PostgreSQL, with additional concurrency tests for database-specific transaction semantics.

Atomic store inputs have documented item and encoded-byte limits. Trigger acceptance, DAG materialization, event append, Agent inventory, and other collection-bearing operations reject oversized work before opening a transaction. PostgreSQL adapters use bounded bulk statements rather than one round trip per Job, dependency edge, queue entry, or event.

Idempotency fingerprints describe stable caller intent and MUST NOT include a newly observed server processing timestamp. The first accepted transaction persists its authoritative acceptance or completion time, while a later replay with the same lease, sequence range, payload, and terminal intent returns the original outcome even when it is observed at a later time.

Naming the concrete crate `octacity-server-store-postgres` localizes rather than hides the technology dependency. Replacing it requires a new adapter to pass the atomic store contract, not changing core types.

### 7. PostgreSQL is the first authoritative control-plane store

PostgreSQL stores every correctness-relevant state transition. Initial tables are grouped by responsibility:

- projects and configuration: projects, policy versions, repositories, pipelines, pipeline versions, build configurations, triggers, and schedules;
- execution: builds, attempts, jobs, dependency edges, ready queue entries, pools, agents, registrations, and leases;
- delivery: job events, log-chunk manifests, log-search documents, completions, idempotency records, audit facts, and transactional outbox;
- outputs and cache: upload records, logical artifacts, cache sessions, namespace policy, and object metadata;
- operations: worker claims, adapter retries, retention claims, and migration state.

Queue acquisition and DAG transitions use short transactions and proven locking such as `FOR UPDATE SKIP LOCKED`. Correctness never depends on an in-memory mutex, notification, or timer.

Authoritative storage does not imply database-owned business behavior. PostgreSQL persists the state selected by core and application code and provides declarative structural integrity, isolation, and locking. It does not implement state machines, hierarchy traversal decisions, contiguous sequence allocation, policy resolution, or immutable lifecycle rules in PL/pgSQL triggers or stored routines. The Rust adapter exposes only complete atomic operations, so an alternative store implements the same port and contract rather than reproducing hidden PostgreSQL behavior.

Pipeline dependency policy is an explicit typed immutable value rather than adapter-owned JSON. A terminal Job mutation serializes the affected Attempt, the PostgreSQL adapter loads persisted predecessor outcomes, and the Orchestrator derives the readiness decision through the Pipeline policy before the adapter applies it in bulk. Concurrent fan-in completions therefore cannot leave a satisfied child blocked without moving domain policy into infrastructure.

Kafka and NATS are future `EventBus` adapters for wakeups, fan-out, and integration delivery. The transactional outbox preserves causality between authoritative state and publication. Making a broker the authoritative Job queue is a separate architecture change because it introduces cross-system transaction and recovery semantics.

### 8. Management REST is a replaceable trusted-network adapter

`octacity-server-api-rest` exposes `/api/v1/...` management resources. The first release performs no operator authentication or RBAC. Startup requires an explicit acknowledgement when the management listener is reachable beyond loopback, and documentation requires network-level isolation.

Agent routes remain independently authenticated with enrollment and registration credentials. Webhook routes authenticate provider deliveries. Liveness and readiness remain bounded and unauthenticated.

The composition root evaluates readiness under one aggregate deadline and records a stable non-secret dependency name plus an unavailable-or-timeout reason only on initial state and transitions. Object-storage readiness performs a complete process-unique PUT/GET/COPY/GET/DELETE qualification, uses `HeadObject` on its own retained marker for cheap checks without requiring bucket-list permission, and repeats complete qualification after a configured interval or immediately after availability loss or a failed artifact operation. Qualification and invalidation are serialized so an older successful probe cannot hide a concurrent operation failure. Operators of versioned buckets MUST expire noncurrent versions and delete markers under the health-probe prefix.

The dependency graph reserves a later `octacity-server-auth` core module with local, LDAP, TOTP, and other adapters. None of those crates or provider SDKs is created in v1; the future module will decorate application commands and queries with verified actor context without moving authorization decisions into REST or GraphQL adapters.

REST DTOs never serialize domain entities or database rows directly. Requests use bounded decoding, stable error codes, request IDs, idempotency keys, and optimistic preconditions. OpenAPI is generated from or verified against the actual registered routes and DTOs.

Live CLI output uses bounded long polling of Job events without holding a transaction or checked-out database connection while waiting.

Build-log search is a query interface distinct from live event following. It accepts a bounded query and explicit project, Build, Attempt, Job, stream, and time filters, and returns deterministic cursor pages with bounded snippets, sequence references, and index freshness. The first adapter supports full-text terms and literal fragments; unbounded regular-expression execution is not part of the initial contract.

### 9. Webhook providers and VCS implementations use separate protocols

Webhook integrations and VCS access are related through Repository identity but have different interfaces.

The webhook-provider protocol supports:

- capabilities and protocol negotiation;
- verification and normalization of a bounded raw delivery;
- optional managed create, observe, rotate, and delete registration operations;
- cancellation, classified errors, and bounded diagnostics.

An unmanaged integration returns the callback URL and verification requirements so an operator can create the remote hook manually. A managed integration supplies protected provider administration credentials only to the selected adapter. Both produce the same server-owned integration identity.

The adapter authenticates the exact raw body before returning an `AuthenticatedRepositoryEvent`. The normalized event contains integration-scoped delivery identity, event kind, repository identity, ref/revision facts, provider time, actor display metadata, and a bounded opaque metadata map. The trigger engine never receives an unauthenticated event.

The VCS protocol separately supports bounded ref listing, commit selection and metadata, tree listing, file-content reads, and immutable revision resolution. `octacity-vcs-git` is the first implementation. It does not execute content or materialize a working tree for server-side builds.

Provider-specific implementations such as GitHub and Gerrit can implement webhook, VCS, or both protocols. Gerrit event-stream support, if needed later, is another inbound adapter that produces the same authenticated normalized event rather than pretending every provider transport is HTTP webhook delivery.

### 10. Secret integration passes references and grants, not values

`octacity-server-secrets` owns logical secret references, project policy, workload identity selection, and the interface for obtaining bounded short-lived access. Provider adapters own provider credentials and mapping configuration.

The preferred path is for the server to give the agent or Octa a scoped identity/grant so Octa reads the secret itself. Raw secret values do not enter Build records, JobSpecs, logs, metrics, audit facts, or management responses. A provider that cannot issue delegated access requires an explicit separately reviewed delivery mode.

The first release may use operator-configured references needed by the existing Octa contract; LDAP/TOTP and operator identity are unrelated and deferred.

### 11. Build Results split authoritative metadata from large immutable bytes

`octacity-server-artifacts` owns logical upload records, fencing, idempotency, visibility, and retention. The shared artifact protocol exposes logical identifiers, immutable content identity, and opaque short-lived transfer capabilities. `octacity-artifact-store` defines byte-store operations; `octacity-artifact-s3` implements them with S3-compatible storage.

Begin upload commits a pending logical record before asking the byte store for a transfer capability. Complete upload verifies exact generation, size, and SHA-256 before publishing metadata. ETag is never a content identity. Physical bucket and key layout never crosses the artifact interface.

The existing crate that combines the artifact interface and S3 implementation is split during the workspace reorganization rather than duplicated.

A **Build Result** is the logical aggregate returned to clients for one Build and its Attempts. It contains the immutable execution-configuration snapshot, ordered event and log history, and produced files and reports. An **output artifact** is only a produced file or report within that aggregate; it is not the configuration snapshot or the log.

PostgreSQL is authoritative for the immutable configuration snapshot, Build/Attempt/Job provenance, event ordering and cursors, logical object metadata, log-chunk manifests, visibility, and retention state. The configured object store holds produced file/report bytes and bounded immutable redacted log chunks. Clients address every component through logical identifiers and never depend on a bucket, key, or S3 response type.

Before a log payload becomes durable, required agent and server redaction removes configured credentials, secret values, presigned query credentials, and protected wrappers. The server accumulates textual stdout and stderr into bounded chunks rather than one object per line. It writes an immutable chunk using an idempotent logical identity and digest before a short PostgreSQL transaction records its sequence range, object metadata, contiguous event acknowledgement, and indexing outbox entry. A database failure may leave an invisible orphan object for retention cleanup; committed metadata never names a missing or unverified object.

The first `LogSearchIndex` implementation is a rebuildable PostgreSQL projection. It indexes normalized redacted text with `to_tsvector('simple', ...)` and a GIN index for full-text terms, plus `pg_trgm` for bounded literal fragments such as error codes, paths, and hashes. Search text may duplicate archived chunk bytes, but the projection is not correctness state and can be recreated from committed chunks and manifests. A later OpenSearch or equivalent adapter can implement the same port without changing REST or application contracts.

Indexing consumes durable outbox work and is eventually consistent. Every Project has contiguous positive indexing positions persisted with its work items. PostgreSQL serializes a per-Project counter in the same transaction as each work insert, rejects explicit gaps, and derives the committed watermark from that counter rather than `MAX(position)`. Search responses expose both the greatest contiguous indexed position and the latest committed authoritative position so callers can distinguish no match from an index that has not caught up; observing a later position never hides a gap. Index unavailability or lag never rejects event ingestion, prevents a valid heartbeat, or changes Job completion. Rebuild, retry, and retention are idempotent. Removing a Build Result first removes logical visibility, then records a durable search tombstone and removes its documents, and finally removes its object bytes; stale indexing or rebuild work cannot resurrect a tombstoned Build.

### 12. Cache authority reuses Octa semantics

`octacity-server-cache` implements server-side namespace policy and fenced session lifecycle. The HTTP L2 adapter implements the published Octa cache protocol but does not calculate action keys, interpret task results, or understand plugin cache contracts.

Cache grants bind registration, current lease fence, project namespace, permissions, and expiry. Revocation, expiry, or fencing immediately ends authorization. Metadata is authoritative in the store; immutable bytes use the configured object adapter behind a cache-specific layout.

### 13. Audit and observability are different modules

Audit is correctness data. Every accepted command, agent mutation, trigger decision, orchestrator transition, and durable worker outcome appends a structured audit fact atomically with its state mutation. In unauthenticated management mode the actor is honestly recorded as an unauthenticated management actor plus request identity, not as an invented principal.

Observability is diagnostic data. `octacity-observability` defines stable metric names, units, trace fields, redaction, and cardinality limits shared by server and agent. Exporter failure cannot fail a valid lease heartbeat or state transition. Agent telemetry uses bounded protocol fields and remains separate from ordered build-event history.

### 14. Agent provisioning stops at a protocol in v1

`octacity-agent-provisioning-protocol` defines version negotiation, provision, observe, terminate, cancellation, idempotency, normalized lifecycle state, and classified failures. It prevents virtualization-provider types from leaking into JobSpec, Agent, Pool, or placement interfaces.

The first release ships no vSphere, Proxmox, or other production provisioning adapter and no reconciliation loop that changes real infrastructure. Static-agent readiness does not depend on an agent provisioner. A deterministic protocol conformance fixture validates the contract without advertising dynamic agent provisioning as available.

### 15. Background work is durable and leased

Due schedules, trigger evaluation, DAG transitions, lease expiry, webhook/VCS retries, outbox delivery, build-log indexing and rebuild, pending-artifact and orphan-log cleanup, cache retention, and audit retention are store-backed work. Each worker claims bounded batches with owner and deadline, performs idempotent work, and records completion.

Process-local tasks are wake and execution mechanisms, never the only record that work exists. Notifications and future brokers are hints; every wake or timeout re-reads authoritative state. The process supervises all workers under one cancellation tree.

### 16. Delivery phases and completion gates

Implementation proceeds through executable vertical slices:

1. **Workspace and contracts**: establish `cli/server/agent/shared`, crate ownership, dependency checks, protocol crates, domain vocabulary, configuration, and a minimal composition root.
2. **Store foundation**: atomic store, authoritative log-index watermark, and `LogSearchIndex` contracts; PostgreSQL adapter, migrations, agent credentials, idempotency, audit, outbox, readiness, and restart tests.
3. **Projects, pipelines, and manual builds**: hierarchy, policy, repositories, pipeline DAGs, build configurations, manual triggers, Attempt materialization, JobSpec signing, and queries.
4. **Static-agent vertical slice**: pools, placement scheduler, leases, heartbeat, events, completion, orchestrator transitions, drain, expiry, and a released Native agent running a multi-node pipeline sequentially.
5. **REST completion**: all initial management commands and queries, OpenAPI, CLI examples, long-poll event reads, bounded build-log search, and trusted-network deployment guardrails.
6. **Artifacts, logs, cache, and secrets**: backend-neutral contracts, S3 adapter, immutable redacted log chunks, PostgreSQL search projection and rebuild, Octa L2, logical secret references, and provider-isolation tests.
7. **Triggers, webhooks, and VCS**: durable schedules/internal events, webhook protocol and manual mode, managed-provider protocol, VCS protocol/host, Git adapter, deduplication, and retry.
8. **Observability and Agent Ready matrix**: server/agent metrics, tracing, release installation, runtimes, failure, reboot, upgrade, drain, cleanup, and performance gates.
9. **Production hardening**: backup/restore, migration compatibility, replica contention, retention load, rate limiting, TLS/proxy and trusted-network runbooks.
10. **Future extension contracts**: agent-provisioning conformance only; operator auth, GraphQL, brokers, managed-service adapters, and virtualization adapters remain later changes.

Phases 1-4 form the minimum useful server. Later phases remain independently gated so the broad module inventory does not become one indivisible delivery.

## Risks / Trade-offs

- **[Many crates can become shallow pass-throughs]** -> Require each crate to own real invariants, a versioned protocol, or an enforced dependency seam; merge crates that fail the deletion test.
- **[Moving the existing flat workspace can obscure server work]** -> Perform mechanical path moves separately from behavioral edits and keep Cargo package names stable where possible.
- **[Unauthenticated management is unsafe outside a trusted network]** -> Require explicit startup acknowledgement, separate listeners/routes, prominent diagnostics, and deployment tests; add operator auth in a later change.
- **[DAG orchestration expands the first vertical slice]** -> Implement deterministic dependency policies first and defer matrices, dynamic graph generation, and conditional expression languages.
- **[A generic store can become shallow]** -> Define use-case-shaped atomic operations and adapter contract suites rather than table CRUD.
- **[PostgreSQL queue contention can grow]** -> Keep placement and DAG transactions short, index ready predicates, use skip-locked claims, and benchmark before adding a broker.
- **[Outbox delivery can duplicate messages]** -> Require idempotent consumers and stable message identities; the outbox guarantees durable at-least-once publication, not magical exactly-once transport.
- **[Provider plugins expand trusted code]** -> Require operator installation, digest locks, protocol validation, bounded processes, and least-privilege credential delivery.
- **[Secret abstractions can accidentally centralize raw values]** -> Prefer delegated identities and explicit provider-owned grants; prohibit secret values in durable domain and diagnostic types.
- **[Presigned capabilities can leak]** -> Use redacted wrappers and prohibit query strings in tracing, metrics, audit, and errors.
- **[Searchable logs can retain secrets or become an unbounded query surface]** -> Redact before both archive and indexing, bound chunk and query sizes, provide full-text and literal modes rather than unrestricted regular expressions, scope every query, and test deletion and rebuild.
- **[A derived search index can lag or be lost]** -> Publish indexing work through the transactional outbox, expose freshness, keep live event reads independent, and rebuild idempotently from committed log chunks.
- **[One process can become a hidden singleton]** -> Test every transactional claim and worker with concurrent server identities.

## Migration Plan

1. Establish the four product directories and architecture checks while preserving existing Cargo package names and released agent behavior.
2. Add server core, application, protocol, API, infrastructure, and composition crates without enabling mutation routes.
3. Split the existing artifact interface from its S3 adapter without changing the agent artifact wire contract.
4. Deploy PostgreSQL, S3-compatible storage, signing material, agent credential material, and one server replica in a test trusted network.
5. Create projects, pipeline, build configuration, manual Build, Agent Pool, and enrollment token through REST; pass the multi-node Native vertical slice.
6. Enable artifacts, immutable log archival, PostgreSQL log search, cache, secrets, schedules, webhooks, and VCS independently behind validated configuration.
7. Run the complete Agent Ready matrix before declaring the first production server version.
8. Rehearse forward schema migration and previous-binary rollback within a declared compatibility window; restore database and object storage as one consistency unit when backward compatibility is impossible.
9. Add replicas only after transactional contention tests pass. Add operator auth, external brokers, managed provider adapters, and dynamic agent provisioning through later changes.

## Open Questions

- Exact retention durations, project quotas, telemetry budgets, and schedule catch-up defaults will be chosen from Agent Ready load measurements; every value remains explicit configuration.
- TLS may terminate in the server or a documented reverse proxy; both modes must preserve the separation between trusted management, authenticated agent, and authenticated webhook ingress.
- The first managed webhook-provider adapter will be selected from deployment needs; unmanaged authenticated webhook configuration and the provider protocol are mandatory.
