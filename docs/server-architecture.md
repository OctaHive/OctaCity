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
| `octacity-server-api-agent` | HTTP adaptation of the shared server-Agent protocol | Agent execution or placement decisions |
| `octacity-server-api-webhook` | Bounded raw webhook HTTP ingress | Provider authentication, normalization, Trigger decisions |
| `octacity-server-application` | Transport-independent commands, queries, handlers, projections, transaction coordination | HTTP DTOs and concrete infrastructure |
| `octacity-server-domain` | Server-only identities, versions, bounded values, timestamps, and typed errors | Aggregates, transport DTOs, persistence rows |
| `octacity-server-trigger` | Trigger normalization, occurrence deduplication, and Trigger evaluation state | Ready-Job placement or provider payloads |
| `octacity-server-pipeline` | Immutable Pipeline versions, DAG validation, dependency policy, Attempt materialization rules | Agent execution and queue leasing |
| `octacity-server-job` | Server Job state, requirements, and signed-intent construction inputs | Agent-side Job execution lifecycle |
| `octacity-server-orchestrator` | Build, Attempt, and reconciliation transitions; DAG progression decisions | Agent selection or repository code execution |
| `octacity-server-scheduler` | Ready-Job placement, Lease lifecycle, and Pool drain decisions | Trigger evaluation, DAG traversal, execution |
| `octacity-server-secrets` | Logical secret references, policy, and short-lived grant interfaces | Provider credentials and durable raw secret values |
| `octacity-server-cache` | Cache namespace authority and fenced session lifecycle | Octa action-key or task-result semantics |
| `octacity-server-artifacts` | Logical upload, verification, publication, visibility, and retention state | Bucket names, object keys, provider credentials |
| `octacity-server-audit` | Immutable audit facts and redaction classifications | Diagnostic telemetry |
| `octacity-server-store` | Backend-neutral atomic persistence and log-search ports | SQL rows, SQL transactions, PostgreSQL syntax |
| `octacity-webhook-provider-protocol` | Versioned provider-neutral webhook adapter messages | Provider payload types and process supervision |
| `octacity-vcs-protocol` | Versioned provider-neutral VCS adapter messages | Git implementation and build workspaces |
| `octacity-agent-provisioning-protocol` | Versioned future provision/observe/terminate contract | vSphere, Proxmox, or other production adapters |
| `octacity-server-webhook` | Verified webhook-adapter registry and bounded process host | Provider-specific payload models |
| `octacity-server-vcs` | Verified VCS-adapter registry and bounded process host | Repository code execution |
| `octacity-server-store-postgres` | PostgreSQL schema, rows, locking, transactions, and store-port implementation | Application commands and domain policy |
| `octacity-artifact-store` | Backend-neutral immutable-byte interface | Logical Artifact lifecycle and storage-provider details |
| `octacity-artifact-s3` | S3-compatible implementation of the Artifact Store port | Logical Artifact policy and public S3 details |
| `octacity-protocol` | Shared signed JobSpec, server-Agent, and Artifact-transfer wire contracts | Server domain entities and HTTP routes |
| `octacity-observability` | Shared metric, trace, redaction, and cardinality vocabulary | Audit truth and exporter-specific state |

The Artifact Store port lives in the core layer. Its S3-compatible adapter is
an infrastructure crate selected only by a composition root; neither the core
nor the server-Agent wire contract depends on the AWS SDK.

Process readiness is composed only in `octacity-server`. The REST adapter sees
a constant-time boolean callback and therefore has no dependency on SQLx, S3,
signing keys, secret providers, or worker implementations. A supervised
monitor refreshes that snapshot under an aggregate deadline. Migrations,
PostgreSQL, object storage, and signing capability are unconditional checks;
future configured mandatory secret providers and workers must be added by the
composition root through the additional-required-check seam when their
implementations arrive. The first successful object-storage check verifies the
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

Several entries above currently contain only ownership documentation because
task 1.2 established the dependency boundaries before their implementation
tasks begin. These phase scaffolds are not evidence that a separate crate has
earned a permanent seam: they remain dependency-free until implemented, and
each must pass the deletion test described in the design (own real invariants
or an enforceable boundary) or be merged into its caller.

The shared observability package is one such explicit scaffold. Current
inbound HTTP tracing is REST-adapter-local; task 8.2 must introduce the stable
cross-product vocabulary and real server and Agent consumers together.

## Public protocol seams

A protocol seam crosses a process or product boundary and therefore requires
version negotiation, bounded decoding, classified failures, and independent
documentation. A core port is a Rust interface inside the server and is not a
wire compatibility promise.

| Seam | Contract owner | Host or consumers | Trust and translation rule |
| --- | --- | --- | --- |
| Management commands and queries | `octacity-server-application` | REST now; future GraphQL | Each transport owns its DTOs and maps them to application types |
| Server-Agent coordination | `octacity-protocol` | `octacity-server-api-agent`, `octacity-coordinator` | Enrollment, registration epoch, Lease fence, and signed JobSpec remain mandatory |
| Artifact transfer | `octacity-protocol::artifact` | Server Artifact domain and Agent output flow | Only logical identifiers and short-lived opaque capabilities cross the seam |
| Webhook provider process | `octacity-webhook-provider-protocol` | `octacity-server-webhook` | Exact raw delivery is authenticated before a normalized event reaches the Trigger Engine |
| VCS provider process | `octacity-vcs-protocol` | `octacity-server-vcs` | Mutable refs resolve once to immutable revisions; repository content is bounded data only |
| Agent provisioning process | `octacity-agent-provisioning-protocol` | Future infrastructure host | No production adapter or dynamic-provisioning readiness dependency exists in v1 |
| Authoritative store ports | `octacity-server-store` | PostgreSQL adapter; deterministic in-memory test adapter | Initial ports expose implemented atomic coordination, credential, and log-index watermark use cases and grow with feature implementations; SQL types never cross an interface |
| Build-log search ports | `octacity-server-store::{LogIndexWorkStore, LogSearchIndex}` | Authoritative store plus PostgreSQL projection; deterministic in-memory test adapters | The application reads the committed watermark from the authoritative port and combines it with contiguous projection progress; PostgreSQL query syntax and physical log locations do not cross either interface |
| Immutable byte-store port | `octacity-artifact-store` | `octacity-artifact-s3` | Physical bucket, key, credential, and ETag remain adapter-private |
| Secret-provider port | `octacity-server-secrets` | Future configured provider adapters | Logical references and scoped grants cross the port; raw values do not enter durable domain state |
| Octa remote-cache data plane | Published Octa HTTP cache protocol | Cache HTTP adapter and Octa | Server authorizes namespaces and sessions but does not reinterpret Octa cache keys |

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
same transaction that revokes the old registration. Exact retries are keyed by
the newly proposed registration identity and recover the original outcome;
using the consumed enrollment credential or a superseded registration under a
new identity is rejected. HTTP authentication and mapping to the shared Agent
DTOs are added at the Agent API seam in task 5.3.

## Store transaction and projection invariants

Atomic store requests are bounded before an adapter opens a transaction. The
core interface publishes limits for encoded documents, Trigger materialization,
dependency edges, Pool allowlists, Agent inventory, and event batches. Concrete
adapters revalidate those limits and use bounded bulk statements for collection
writes. A durable Job event computes its own canonical digest from event kind,
source time, and payload; callers cannot supply payload and digest separately.

Pipeline dependency policy is a typed immutable Pipeline value. PostgreSQL
serializes terminal mutations for one Attempt and loads persisted fan-in
outcomes; the Orchestrator makes the readiness decision, after which the
adapter updates Jobs and queue entries in bounded bulk statements. This keeps
policy out of infrastructure while preventing concurrent predecessor
completion from leaving a satisfied child blocked. The in-memory adapter runs
the same decision and behavioral contract, including registration and
Lease-expiry checks.

`LogIndexWorkStore` reads the committed project-local watermark from the
authoritative store. The application, rather than an external query DTO,
combines it with the greatest contiguous position reported by
`LogSearchIndex`. Applying a later position therefore cannot conceal an earlier
gap. Build deletion records a durable tombstone; delayed indexing and rebuild
work has a projection-specific `Superseded` outcome and cannot restore
searchable documents.

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
