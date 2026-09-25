# Server vocabulary, ownership, and seams

This document maps the canonical [OctaCity coordination language](../CONTEXT.md)
to crate ownership and public interfaces. It describes the modular-monolith
shape; it is not a database schema or a REST contract.

## Coordination flow

```text
Trigger Occurrence -> Trigger Engine -> Build -> Attempt -> materialized Jobs
                                                        |
persisted Job outcomes -> Orchestrator -> newly Ready Jobs
                                              |
                         Placement Scheduler -> fenced Lease -> Agent Executor
```

The roles are deliberately separate:

- the Trigger Engine decides whether one normalized occurrence creates a Build;
- the Orchestrator advances a persisted DAG and derives Attempt and Build state;
- the Placement Scheduler sees only already Ready Jobs and compatible Agents;
- the Executor exists on the Agent and runs one verified `JobSpec`;
- a Pipeline is an immutable definition, while an Attempt is its materialized execution graph.

The server never executes repository-controlled build code. Scheduling a
Trigger, progressing a DAG, placing a Ready Job, and executing a Job are four
different responsibilities.

## Dependency direction

```text
API adapters -> application commands and queries -> core modules and ports
composition root -> API + application + selected infrastructure adapters
infrastructure adapters -> core ports and provider-neutral protocols
agent and CLI -> shared cross-product contracts only
```

Only `octacity-server` selects concrete adapters. Core modules do not depend on
Axum, SQLx, S3 SDKs, Git implementations, provider SDKs, or process
configuration. Architecture checks enforce these directions from Cargo
metadata for normal, build, and development dependencies. Test-only integration
crates belong under `server/tests`; a layer-local dev-dependency cannot bypass
the production dependency rules. API crates may use HTTP libraries, but they
may not select persistence, object-store, VCS, or provider implementations.
External dependencies in API, application, core, shared, and protocol crates
use explicit per-layer allowlists. An unknown SDK is rejected by default, so
enforcement does not depend on recognizing every database, broker, VCS, or
provider crate name.

## Crate ownership

| Crate | Owns | Explicitly does not own |
| --- | --- | --- |
| `octacity-server` | Process composition and concrete adapter selection | Domain decisions or reusable transport contracts |
| `octacity-server-api-rest` | Management REST DTOs, decoding, routing, OpenAPI, HTTP error mapping | Transactions, domain state, database rows |
| `octacity-server-api-webhook` | Exact-body public webhook routing, bounded decoding, and mapping to typed application delivery commands | Provider SDKs, adapter process supervision, Trigger persistence |
| `octacity-server-api-agent` | HTTP adaptation of the shared server-Agent protocol | Agent execution or placement decisions |
| `octacity-server-api-cache` | Exact bounded adaptation of Octa HTTP cache v1, including protocol headers and media types | Action-key calculation, plugin contracts, quota decisions, or physical object keys |
| `octacity-server-application` | Transport-independent commands, queries, handlers, projections, transaction coordination, and cross-module Project-policy resolution | HTTP DTOs and concrete infrastructure |
| `octacity-server-domain` | Server-only identities, versions, bounded values, timestamps, and typed errors | Aggregates, transport DTOs, persistence rows |
| `octacity-server-trigger` | Trigger normalization, occurrence deduplication, and Trigger evaluation state | Ready-Job placement or provider payloads |
| `octacity-server-pipeline` | Immutable Pipeline versions, DAG validation, dependency policy, Attempt materialization rules | Agent execution and queue leasing |
| `octacity-server-job` | Server Job state, requirements, and signed-intent construction inputs | Agent-side Job execution lifecycle |
| `octacity-server-orchestrator` | Build, Attempt, and reconciliation transitions; DAG progression decisions | Agent selection or repository code execution |
| `octacity-server-scheduler` | Ready-Job placement, Lease lifecycle, and Pool drain decisions | Trigger evaluation, DAG traversal, execution |
| `octacity-server-secrets` | Logical secret references, policy, and short-lived grant interfaces | Provider credentials and durable raw secret values |
| `octacity-server-cache` | Cache namespace authority, fenced session lifecycle, immutable byte-store port, and protocol-level blob integrity | Octa action-key calculation or plugin task contracts |
| `octacity-server-artifacts` | Logical upload, verification, publication, visibility, and retention state | Bucket names, object keys, provider credentials |
| `octacity-server-store` | Backend-neutral atomic persistence and log-search ports | SQL rows, SQL transactions, PostgreSQL syntax |
| `octacity-webhook-provider-protocol` | Versioned provider-neutral webhook adapter messages | Provider payload types and process supervision |
| `octacity-vcs-protocol` | Versioned provider-neutral VCS adapter messages | Git implementation and build workspaces |
| `octacity-agent-provisioning-protocol` | Versioned future provision/observe/terminate contract | vSphere, Proxmox, or other production adapters |
| `octacity-server-store-postgres` | PostgreSQL schema, declarative constraints, rows, locking, transactions, and store-port implementation | Application commands, stored business routines, and domain policy |
| `octacity-server-adapter-host` | Shared registry filesystem checks, executable digest verification, bounded framing, cancellation, timeout, and process cleanup | Provider manifests, wire-message semantics, capability policy, and adapter selection |
| `octacity-server-webhook` | Operator-installed webhook-adapter discovery, capability gating, and webhook protocol policy over the shared host | Public webhook HTTP routing, provider SDKs, Trigger evaluation, and durable delivery retry |
| `octacity-server-vcs` | Operator-installed VCS-adapter discovery, capability gating, credential-handle isolation, and VCS protocol policy over the shared host | Git implementation, repository execution, build workspaces, and provider SDKs |
| `octacity-server-audit` | Stable actor vocabulary, secret-safe bounded metadata, immutable fact projections, and deterministic read-query bounds | Independently callable audit writes, transport DTOs, and SQL rows |
| `octacity-vcs-git` | Read-only Git refs, immutable commit selection, tree browsing, and bounded content reads through an ephemeral bare object database | Build workspaces, checkout, repository hooks, Octafile evaluation, and source execution |
| `octacity-artifact-store` | Backend-neutral immutable-byte interface | Logical Artifact lifecycle and storage-provider details |
| `octacity-artifact-s3` | S3-compatible implementation of Artifact and cache immutable-byte ports with separate private layouts | Logical policy and public S3 details |
| `octacity-observability` | Shared stable metric names, units, closed low-cardinality labels, series budgets, safe facade emission, trace fields, and redaction rules | Exporters, subscribers, transport queues, raw diagnostics, or correctness state |
| `octacity-protocol` | Shared signed JobSpec, server-Agent, and Artifact-transfer wire contracts | Server domain entities and HTTP routes |

Planned ownership names are not compiled as empty packages. Tasks 6.3 and 6.4
have introduced `octacity-server-webhook` over the shared verified process host and
`octacity-server-api-webhook` with its exact-body authenticated ingress.
Tasks 6.6 and 6.7 have introduced `octacity-server-vcs` with its verified
process host and `octacity-vcs-git` as the first read-only implementation.
Task 8.1 has introduced `octacity-server-audit` with transactional mutation
consumers and read-only management queries. Task 8.2 has introduced
`octacity-observability` with closed metric labels, explicit per-instrument
series budgets, stable trace fields, and both server and Agent consumers.

The Artifact Store port lives in the core layer. Its S3-compatible adapter is
an infrastructure crate selected only by a composition root; neither the core
nor the server-Agent wire contract depends on the AWS SDK.

`octacity-server-job` owns JobSpec derivation and the private-key signer. It
accepts only immutable Build facts, a strict Pipeline execution template, and
validated server policy; it never accepts a caller-created `JobSpecV1` or
signed envelope. Materialization persists a stable `JobSpecTemplate` without
Job identity, Attempt number, issue time, or signature. The authoritative store
adds those volatile facts and persists `jobs.signed_job_spec` in the same
transaction that changes a root or dependent Job to `Ready`, before inserting
its queue entry. Blocked Jobs therefore cannot retain an expired envelope, and
key rotation applies naturally when they become ready. Lease fences, transfer
capabilities, and secret grants are created later and remain outside the signed
execution intent.

The architecture guard grants only `octacity-server-job` access to the pure
`ed25519-dalek` and `base64` libraries. This package-specific capability keeps
private-key handling out of shared wire contracts and prevents unrelated core
crates from acquiring crypto implementation dependencies implicitly.

Process readiness is composed only in `octacity-server`. The REST adapter sees
a constant-time boolean callback and therefore has no dependency on SQLx, S3,
signing keys, secret providers, or worker implementations. A supervised
monitor refreshes that snapshot under an aggregate deadline. Migrations,
PostgreSQL, object storage, signing capability, and the lease-expiry worker are
unconditional checks. Future configured mandatory secret providers and workers
must be added by the composition root through the same additional-required-check
seam. The expiry worker claims bounded batches in PostgreSQL with an owner and
deadline, fences the expired Lease, and only then requeues or terminally fails
the Job. Process timers merely wake the worker; the durable Lease and claim rows
remain authoritative across restart and replica races. The first successful object-storage check verifies the
actual bounded PUT, GET, COPY, and DELETE capability set with process-unique
probe objects. Healthy recurring checks use `HeadObject` on the process-owned
marker, so readiness adds no bucket-list permission. The configured capability
recheck interval periodically re-arms the full probe; an availability failure
or failed artifact operation does so immediately. Qualification and
invalidation are serialized so a concurrent older successful probe cannot hide
an object-operation failure. Versioned buckets require lifecycle cleanup for
noncurrent probe versions and delete markers below the health prefix. Recurring
migration readiness performs a read-only compatibility check
and takes the migration lock only while the schema is behind. The monitor logs
only initial state and transitions, with a stable non-secret dependency name
and unavailable/timeout reason. Liveness remains process-local and independent
of this snapshot.

The same composition root constructs the production management application
from typed Project, Pipeline, Repository, Build Configuration, Pool, Agent,
enrollment, Trigger, Build-run and Job-event handlers. It separately constructs
the authenticated Agent application for registration, placement, heartbeat,
event append and terminal completion. Both share the PostgreSQL adapters, but
only operations that can create a Ready Job receive the active JobSpec signer.
Health-only routers remain test components and are not the production wiring.

Several entries above currently contain only ownership documentation because
task 1.2 established the dependency boundaries before their implementation
tasks begin. These phase scaffolds are not evidence that a separate crate has
earned a permanent seam: they remain dependency-free until implemented, and
each must pass the deletion test described in the design (own real invariants
or an enforceable boundary) or be merged into its caller.

The shared observability package contains vocabulary and safety invariants, not
an exporter implementation. Its thin metrics facade validates the declared
instrument and closed labels before handing a point to the process recorder;
without a recorder it is a bounded no-op. High-cardinality request, Build, Attempt, Job,
Agent, Lease, Trigger-occurrence, and integration identities are trace-only
correlation fields. Metric labels are sealed enums with finite value sets, so
raw paths, identifiers, user input, and protected wrappers cannot create new
time series. Export failure is diagnostic loss and never correctness state.
The application layer owns the transport-independent Agent telemetry exporter
port, the Agent HTTP adapter owns its bounded concurrency and timeout gate, and
the composition root selects the concrete exporter adapter.

Task 8.4 instruments management and webhook HTTP boundaries, provider hosts,
Trigger and orchestration decisions, placement and Lease operations, the
PostgreSQL port boundary, cache and Artifact services, transactional-outbox
delivery, authoritative ready-Job queue snapshots, and supervised worker passes. Request,
adapter, store, and worker spans carry trace-only correlation while metric
series retain only the closed classifications above.

The server composition root installs a process-local Prometheus recorder,
supervises its maintenance task, and exposes snapshots on the management
listener at `GET /metrics`. Recorder initialization and scrape failures degrade
observability only: they cannot reject domain work or stop protocol listeners.

## Public protocol seams

A protocol seam crosses a process or product boundary and therefore requires
version negotiation, bounded decoding, classified failures, and independent
documentation. A core port is a Rust interface inside the server and is not a
wire compatibility promise.

| Seam | Contract owner | Host or consumers | Trust and translation rule |
| --- | --- | --- | --- |
| Management commands and queries | `octacity-server-application` | REST now; future GraphQL | Each transport owns its DTOs and maps them to application types |
| Public webhook delivery | `octacity-server-api-webhook` | Provider callbacks | The dedicated listener durably admits exact bounded bytes and allowlisted headers, returns a receipt identity, and never waits for adapter execution |
| Server-Agent coordination | `octacity-protocol` | `octacity-server-api-agent`, `octacity-coordinator` | Enrollment, registration epoch, Lease fence, and signed JobSpec remain mandatory |
| Artifact transfer | `octacity-protocol::artifact` | Server Artifact domain and Agent output flow | Only logical identifiers and short-lived opaque capabilities cross the seam |
| Webhook provider process | `octacity-webhook-provider-protocol` | `octacity-server-webhook` | Exact raw delivery is authenticated before normalization; optional managed create, observe, rotate, and delete use stable idempotency identities, protected credential handles, and durable bounded retry state |
| VCS provider process | `octacity-vcs-protocol` | `octacity-server-vcs` | Configured integration identities select an operator-pinned adapter and protected credential handle; mutable refs resolve once to immutable revisions, while repository content remains bounded data only |
| Agent provisioning process | `octacity-agent-provisioning-protocol` | Future infrastructure host | No production adapter or dynamic-provisioning readiness dependency exists in v1 |
| Authoritative store ports | `octacity-server-store` | PostgreSQL adapter; deterministic in-memory test adapter | Initial ports expose implemented atomic coordination, credential, and log-index watermark use cases and grow with feature implementations; SQL types never cross an interface |
| Build-log search ports | `octacity-server-store::{LogIndexWorkStore, LogSearchIndex}` | Authoritative store plus PostgreSQL projection; deterministic in-memory test adapters | The application reads the committed watermark from the authoritative port and combines it with contiguous projection progress; PostgreSQL query syntax and physical log locations do not cross either interface |
| Immutable byte-store ports | `octacity-artifact-store::{ArtifactStore, LogChunkStore}` | `octacity-artifact-s3` | Physical bucket, key, credential, and ETag remain adapter-private; log writes are idempotent and independently verified before their manifest can commit |
| Secret-provider port | `octacity-server-secrets` | Future configured provider adapters | Logical references and scoped grants cross the port; raw values do not enter durable domain state |
| Octa remote-cache data plane | Published Octa HTTP cache protocol | Cache HTTP adapter and Octa | Server authorizes namespaces and sessions but does not reinterpret Octa cache keys |
| Cache immutable bytes | `octacity-server-cache::CacheBlobStore` | Configured S3 adapter | Scope and validated descriptor cross the port; bucket, physical key, ETag, and credentials remain private |

Language-neutral wire documents and golden fixtures are indexed in
[the protocol catalogue](protocols/README.md). The server-side adapter protocol
crates are additionally mapped in
[the server protocol README](../server/protocols/README.md).

## Agent credential lifecycle

`AgentCredentialStore` is a core interface rather than a wire protocol. The
application layer supplies independently generated 256-bit secrets; adapters
persist only domain-separated SHA-256 verifiers. Secret and verifier formatting
is always redacted, and audit and outbox payloads contain only logical
identities, Pool binding, epoch, and expiry metadata.

An enrollment credential is bound to an exact Agent Pool version, an optional
exact platform policy, and an exclusive expiry. Its first successful
registration consumes it atomically. A process restart proves possession of
the current registration credential and creates the next positive epoch in the
same transaction that revokes the old registration. Exact retries derive the
proposed registration identity from the presented credential and
shared-protocol `request_id`, so a lost response recovers the original outcome
even when the server observes a later time. Using a consumed enrollment
credential or a superseded registration under a new identity is rejected. The
Agent API validates the complete shared `AgentInventory`, maps the request
through the application service, and never exposes store rows.

Credential files contain an opaque dot-separated token. Enrollment tokens use
`enrollment.<credential-uuid>.<base64url-secret>`. After registration the Agent
keeps the same 256-bit secret in memory and promotes the token to
`registration.<registration-uuid>.<base64url-secret>`; the raw secret is never
logged or persisted by the server. Every poll, heartbeat, event append, upload,
cache, and completion use case authenticates this current registration before
performing work. A new registration revokes the old epoch, and fenced lease
mutations also verify that their registration has not been revoked.

## Store transaction and projection invariants

Atomic store requests are bounded before an adapter opens a transaction. The
core interface publishes limits for encoded documents, Trigger materialization,
dependency edges, Pool allowlists, Agent inventory, and event batches. Concrete
adapters revalidate those limits and use bounded bulk statements for collection
writes. A durable Job event computes its own canonical digest from event kind,
source time, and payload; callers cannot supply payload and digest separately.

Application use cases depend on operation-shaped ports rather than the complete
adapter surface: Trigger acceptance, Job execution, and Build run control have
separate interfaces. `AuthoritativeStore` is only their composite adapter
contract. Manual Trigger context loading also uses a dedicated Trigger-definition
query and verifies the exact enabled manual Trigger version and target before
any mutable VCS reference is resolved. The REST command reserves its stable
intent and canonical payload in `trigger_evaluation_work` before that external
read. The request and recovery worker share one fenced claim; transient VCS
failures use a bounded policy, while permanent or exhausted failures remain as
secret-free dead letters. The first successful mutable-reference result is
checkpointed as an immutable revision under that claim before Build creation,
so a later store retry cannot observe a moved branch. A completed replay reaches
the authoritative Trigger acceptance record before it could invoke VCS again.

The PostgreSQL package exposes a plain `PostgresStore` for configuration,
Project, Pipeline, credential, and indexing ports. Only
`PostgresAuthoritativeStore` accepts the active JobSpec signer and implements
coordination mutations that can move Jobs through a signed `Ready` boundary.
This keeps unrelated persistence capabilities usable without signing material.
Platform requirements use the shared closed OS and architecture types at the
configuration boundary, so an unsupported spelling cannot be published and
fail later during JobSpec derivation.

PostgreSQL describes storage shape with keys, foreign keys, unique indexes,
nullability, and row-local checks. It does not own lifecycle transitions,
hierarchy validity, sequence allocation, or append-only policy through stored
routines or triggers. Cross-row decisions stay in Rust store operations and
are serialized with an explicit transaction isolation level, row lock, or
transaction-scoped advisory lock. This keeps the same core behavior available
to another store adapter while PostgreSQL remains responsible for durable
atomicity and structural integrity.

Pipeline dependency policy is a typed immutable Pipeline value. A terminal
Job mutation locks its Build and Attempt and loads the complete persisted Job
graph. The Orchestrator validates that snapshot and computes a deterministic
fixed point:
newly satisfied Jobs become `Ready`, dependency failures cascade `Skipped`
through descendants, and the post-transition graph derives the Attempt and
Build state. PostgreSQL then updates Jobs, the global ready queue, Attempt,
Build, completion replay data, audit, and outbox in the same transaction.
Job read projections call this durable Pool/Agent pair an assignment rather
than current ownership and retain it for executed terminal Jobs. Current Lease
ownership and its secret fence remain coordination-only data; queued or skipped
Jobs cannot claim an assignment, while cancellation may be terminal with or
without one depending on whether placement occurred.
The first newly persisted Agent event advances a leased Job through the core
`ExecutionStarted` transition to `Running`; terminal completion uses the same
state machine and may infer that transition only for a zero-event Job.

The decision is pure and stably ordered by Job identity. Re-evaluating the
same graph produces no transitions, while an exact completion replay returns
the originally committed transition set and aggregate states. A process crash
can therefore expose either the graph before the transaction or the complete
post-orchestration graph, never a persisted outcome with partially advanced
dependencies. The in-memory and PostgreSQL adapters run the same decision and
behavioral contract. This keeps failure policy out of infrastructure while
preventing concurrent predecessor completion from leaving a satisfied child
blocked.

Build cancellation and retry use separate complete store operations. A
cancellation locks the Build and its current Attempt, records one durable
Build-level intent, and asks the Orchestrator to transition the complete Job
set. Blocked and ready Jobs become terminal `Cancelled`, their queue entries
are removed, leased or running Jobs become `Cancelling`, and current Leases
become `cancellation_requested`; terminal Jobs never change. The Attempt and
Build remain running only while an Agent must acknowledge cancellation, then
normal fenced completion derives their terminal `Cancelled` state. Exact
replay returns the original transition set.

A retry is allowed only for the latest failed Attempt. The PostgreSQL adapter
locks the Build, calculates its next positive Attempt number in Rust, and
rejects a stale proposed number, so concurrent requests cannot create sibling
retries. Candidate Jobs carry fresh identities, while one backend-neutral core
decision compares every Pipeline node, dependency, root and non-root policy,
placement requirement, Job snapshot, and stable JobSpec template with the
preceding immutable Attempt. Root JobSpecs are signed only after that check.
The new Attempt records `retry_of_attempt_id`; the prior Attempt, events,
logs, and artifacts remain append-only and addressable. Attempt creation, DAG
materialization, root queue insertion, Build reactivation, idempotency, audit,
and outbox commit atomically.

### Application projection boundary

`octacity-server-application` owns transport-independent projections for
Projects, immutable Pipeline and Build Configuration versions, Builds,
Attempts, Jobs, diagnostic DAG causality, and Trigger history. They are not
REST DTOs or database rows: later query handlers return these models, while a
REST or future GraphQL adapter maps them into its own versioned contract.

Projection construction is deliberately lossy at security boundaries. A
Pipeline node is decoded through the strict repository-controlled
`JobExecutionTemplate`; a Build projection decodes the server-owned input and
effective-policy snapshots but omits its JobSpec toolchain policy; a Job
projection omits the signed JobSpec, Lease fence, and internal node snapshot;
and Trigger history omits opaque provider metadata. Configuration policy
exposes only validated logical cache, secret-profile, and workload-identity
references. Output
references contain logical Artifact identity, name, content digest, size, and
publication time, never a bucket, object key, credential, or transfer URL.

The diagnostic DAG projection validates that every dependency stays within
one Attempt and that the resulting graph is acyclic. It exposes stable Job and
Pipeline-node identities, current states, dependency policy, and safe failure
classification so skipped and failed causal paths remain explainable without
returning execution credentials or provider configuration.

### Immutable Pipeline publication

`octacity-server-pipeline` accepts a publication draft only through
`PublishablePipelineDag::new`. The resulting proof type is serializable but
not deserializable, so a restored historical `PipelineDag` must explicitly
cross `for_publication` with the current capability catalog before a store
create or publish operation can accept it. Construction validates node and
edge identity, missing references, cycles, fan-in and fan-out bounds,
process-safe JSON depth, per-node and aggregate encoded size, and requested
execution capabilities. It then canonicalizes nodes, edges, capabilities, and
nested JSON objects. Topological ordering uses node identity as the stable tie
breaker, and the serialized snapshot carries an explicit schema version.

Capabilities are checked against the publication-time `CapabilityCatalog`.
Reading an existing snapshot validates its structure and schema but does not
re-evaluate it against the current catalog: removing a capability from a later
server release must not make historical Pipeline versions unreadable.
Dependency policies return explicit `Blocked`, `Ready`, or `Skipped` decisions,
so failure propagation is owned by Pipeline and Orchestrator code rather than
being inferred by a storage adapter.

`PipelineStore` exposes three use-case-shaped operations: create a Pipeline
with version one, append exactly the next version under an optimistic version
precondition, and read an exact version. It intentionally exposes no update or
delete operation for a published snapshot. Each accepted mutation commits its
idempotency result, audit fact, and outbox event in the same transaction.
PostgreSQL locks the Pipeline identity to serialize competing publications;
the reusable store contract and a PostgreSQL concurrency test verify that one
next version wins and every earlier snapshot remains unchanged.

### Immutable Repository and Build Configuration publication

A Repository has a stable Project-owned identity and sibling-unique name, but
its provider-neutral VCS integration, locator, and revision-selection policy
live in append-only Repository versions. A Build Configuration follows the
same identity/version split. The same `RepositoryLocator` value crosses
configuration and JobSpec derivation:
it accepts provider-owned opaque remote identifiers as well as URLs while
rejecting local paths, traversal, embedded credentials, and query fragments.
Each Build Configuration version captures its enabled state, an exact
Repository version, an exact immutable Pipeline version owned by the same
Project, parameter schema and defaults, accepted Trigger kinds, Agent
requirements, allowed Pools, runtime and network policy, cache authority, an
optional logical Octa secret profile, artifact and report ceilings, and retry
policy. The profile is checked against the immutable effective Project-policy
snapshot before source resolution or persistence side effects.

### Logical secrets and delegated grants

`octacity-server-secrets` owns bounded provider identities, logical secret
references, secret and workload-identity profile names, validated public
provider configuration, and the asynchronous short-lived grant port. A grant
request binds one allowed profile and one provider's bounded reference set to
the Project, Build, Job, current application-authenticated Lease, issue time,
and expiry. The application caller must verify the fence before constructing
this request; the raw fence never crosses the provider seam. The grant lifetime
cannot exceed the provider ceiling, and a returned grant cannot change provider
or extend that authority.

Provider credentials and provider-specific mappings remain inside concrete
adapters. Delegated grant bytes are transient zeroizing values: they implement
neither serialization nor display, and debug output is always redacted.
Provider errors cross the core seam only as stable classifications without raw
messages. Consequently durable Build state, signed JobSpec data, management
REST resources, audit facts, logs, and metric labels can contain logical
profile/reference metadata but not raw secret values, provider credentials, or
delegated grant bytes. Repository-controlled Pipeline templates still cannot
supply a secret profile; the server inserts the policy-authorized logical
profile while deriving the signed JobSpec.

Mutable VCS references, exact network hosts, runtime classes, and artifact
ceilings are server-domain value objects shared by Trigger, policy, and
configuration modules. VCS references therefore have one byte bound, network
allowlists cannot contain URLs, ports, or implicit wildcards, and artifact
shape validation cannot drift between inherited policy and configuration.

`ConfigurationStore` exposes only create, append-next-version, and exact-version
read operations for these aggregates. The core types validate bounded typed
snapshots at every adapter seam; PostgreSQL supplies keys, foreign keys, row
locks, atomicity, and JSON persistence but does not decide configuration
policy. Stable identities and version rows are separate tables, and published
versions have no update or delete operation. Competing publications use the
caller's expected-current-version precondition, while exact idempotent replays
return the original outcome without adding audit or outbox facts.

A Build records exact Build Configuration and Pipeline version references.
Publishing later Repository, Pipeline, or Build Configuration versions can
therefore affect only subsequent Builds. Existing Builds and retries continue
to resolve the snapshots they originally captured. The reusable in-memory and
PostgreSQL store contract verifies version immutability, reference validation,
typed policy rejection, replay behavior, and atomic mutation evidence; a
PostgreSQL concurrency test additionally verifies that only one competing next
configuration version wins.

### Normalized Trigger occurrences

`octacity-server-trigger` owns the normalized occurrence interface. Manual
commands, persisted schedule times, authenticated external repository events,
and server-generated internal events become one strict
`NormalizedTriggerOccurrence` before they reach authoritative persistence.
Every occurrence names an exact Trigger definition and Build Configuration
version, a source-observed time, a source-scoped deduplication identity, a
typed provider-neutral cause, and causal lineage. Only authenticated external
events may retain bounded opaque provider metadata.

Deduplication and causality are deliberately separate. The deduplication key
is the exact Trigger version plus the stable source identity and prevents
retries with different occurrence IDs from creating another Build. Causal
lineage records the root occurrence, optional direct parent, and derivation
depth so later internal-trigger cycle policy can reason about ancestry without
using a provider delivery identifier as a graph edge. Root manual, scheduled,
and external occurrences point to themselves at depth zero; internal
occurrences require distinct root and parent identities and a positive depth.

The authoritative store revalidates this core model and the exact
configuration target before mutation. PostgreSQL persists typed cause,
lineage, origin kind, and bounded provider metadata as data, applies only
structural checks and foreign keys, and keeps the unique source-scoped
deduplication key. The existing atomic Trigger-acceptance operation and its
in-memory/PostgreSQL contract verify that duplicate occurrences expose at most
one Build.

Public webhook ingress first commits a raw receipt with a server-owned identity
and returns `202 Accepted`. A durable worker claims verification work with an
owner and expiry, invokes the pinned adapter, and atomically records the
provider-neutral event under the integration-scoped provider delivery identity.
If the integration was disabled before processing, the worker moves the receipt
to the terminal `suppressed` state without invoking the adapter or evaluating a
Trigger. This also closes the race where a receipt was admitted immediately
before the integration was disabled.
An exact duplicate completes without new Trigger work; conflicting normalized
data under the same provider identity is dead-lettered. The canonical receipt
then evaluates the Trigger with a stable composite identity. A crash before
completion therefore replays the same occurrence and Build. Only transient
adapter failures receive bounded exponential retries; exhausted and permanent
failures retain attempt counts and secret-free diagnostics, while raw bodies
and header values are cleared from terminal records.
Delivery verification and managed-registration work intentionally share one
operator-level polling, claim-lifetime, and retry policy because both invoke
the same bounded webhook adapter host. Their batch sizes remain independent so
one workload cannot silently enlarge the other workload's sequential claim
budget.

Terminal Build transitions publish `build.succeeded`, `build.failed`, or
`build.cancelled` facts through the same transactional outbox commit as the
authoritative state change. The internal-Trigger worker claims bounded batches
with a process owner and expiry, reloads authoritative Build and occurrence
state, and evaluates every enabled immutable Trigger definition that existed
when the source event committed. A crash before claim completion causes
at-least-once redelivery; each downstream occurrence uses the outbox entry as
its stable source identity, so replay reaches the existing Build instead of
creating another one.

Internal causality has two independent guards. An exact Trigger definition may
appear only once from the root occurrence through the candidate, which rejects
direct and indirect cycles such as `A -> B -> A`. A non-repeating chain is also
limited to 32 internal derivations. Protected candidates create no Build, but
the worker completes their source event after processing other candidates, so
a causal loop cannot become an unbounded retry loop. PostgreSQL stores claims
and lineage; the cycle and depth decisions remain in `octacity-server-trigger`
and are shared by every adapter.

`ManualTriggerService` completes the manual path without exposing persistence
or VCS protocol details to a transport. It loads the exact immutable Build
Configuration, Repository, Pipeline, and effective Project policy through
application ports; rejects invalid parameters, source selection, and policy
before external work; then resolves or verifies the selected source through a
provider-neutral `RevisionResolver`. Occurrence, Build, first Attempt, and Job
identities are derived deterministically from the Trigger version and
deduplication identity, so a lost-response replay cannot create a second graph.
The service materializes every Pipeline node and dependency, intersects the
configuration and Project Pool allowlists, and submits one `AcceptTrigger`
operation. That transaction persists the occurrence, immutable Build snapshot,
Attempt, Jobs, dependency edges, audit, outbox, and only root ready-queue
entries together. The first manual observation time is retained, but a replay
with the same source identity is not rejected merely because the retry arrived
later.

`LogIndexWorkStore` reads the committed project-local watermark from the
authoritative store. The application, rather than an external query DTO,
combines it with the greatest contiguous position reported by
`LogSearchIndex`. Applying a later position therefore cannot conceal an earlier
gap. Build deletion records a durable tombstone; delayed indexing and rebuild
work has a projection-specific `Superseded` outcome and cannot restore
searchable documents.

Agent stdout and stderr take a stricter acknowledgement path. The application
decodes runner output frames, masks configured in-memory credentials and URL
query credentials, rewrites the durable event payload with the redacted bytes,
and coalesces adjacent events of one stream into bounded chunks. It writes each
deterministically identified chunk through `LogChunkStore` and verifies length
and SHA-256 before calling the authoritative append. That single append
transaction locks the Project counter and commits Job events, the contiguous
cursor, immutable manifests, and one indexing-work position per chunk. A lost
acknowledgement repeats the same object and transaction identities. An object
store outage cannot advance the cursor; a database rollback can leave only an
invisible object, which orphan cleanup deletes after checking that no committed
visible manifest owns the identity.

## State-machine interfaces

The initial pure transition interfaces live with the modules that own their
invariants:

- [Build, Attempt, and orchestration](../server/core/octacity-server-orchestrator/src/lib.rs);
- [DAG Job](../server/core/octacity-server-job/src/lib.rs);
- [Lease and Pool drain](../server/core/octacity-server-scheduler/src/lib.rs);
- [Artifact publication](../server/core/octacity-server-artifacts/src/lib.rs);
- [cache session](../server/core/octacity-server-cache/src/lib.rs);
- [Trigger Occurrence](../server/core/octacity-server-trigger/src/lib.rs).

Each transition is a pure `state + event -> new state` decision. Persistence,
audit, outbox publication, and wakeups are coordinated outside these modules so
the same interface is used by production callers and table-driven tests.
