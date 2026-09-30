# Deferred server extension seams

This document records where later integrations attach without making them part
of the v1 product. It is an architecture contract, not an implementation
schedule. The first release does not compile, configure, advertise, or require
any adapter described here.

An extension earns a production crate only when one delivery slice includes
real behavior, configuration, composition-root wiring, failure classification,
readiness or capability semantics, and contract tests. A package containing
only traits, placeholder types, or an unused SDK is not a seam and must not be
created. Provider-neutral interfaces remain with their current owner until an
implementation needs a process or infrastructure boundary.

## Boundary map

| Deferred capability | Existing stable seam | Future adapter responsibility | Types that must remain private |
| --- | --- | --- | --- |
| Operator authentication | Management API boundary before `octacity-server-application` dispatch | Authenticate the request, resolve a stable operator actor and claims, and enforce the configured entry policy before a command or query reaches its handler | Sessions, tokens, cookies, provider claims, and authentication-library types |
| LDAP and TOTP | Future operator-identity port behind the same authenticated management boundary | LDAP bind/search and TOTP enrollment or challenge verification; return only a provider-neutral actor and bounded assurance facts | LDAP entries and filters, TOTP seeds and recovery material, directory SDK errors |
| GraphQL | Typed commands, queries, handlers, and projections in `octacity-server-application` | Own the schema, request bounds, pagination, transport error mapping, and DTO-to-application translation as a sibling of REST | Resolver context, schema, GraphQL input/output, and library error types |
| Kafka and NATS | Transactional outbox plus durable PostgreSQL worker claims | Deliver notifications or wake workers after committed state exists; acknowledge only according to the durable outbox contract | Broker clients, topics or subjects, offsets, consumer groups, headers, and broker delivery objects |
| Managed GitHub and Gerrit | `octacity-webhook-provider-protocol`, `octacity-vcs-protocol`, and their verified process hosts | Implement provider authentication, webhook lifecycle, revision resolution, and bounded repository reads behind an operator-pinned executable digest | SDK payloads, installation tokens, provider credentials, rate-limit responses, and provider error types |
| Secret stores | Logical references and short-lived grants in `octacity-server-secrets` | Resolve a configured logical reference, apply provider policy, and return the smallest scoped, expiring, zeroizing grant | Provider credentials, raw configuration objects, secret versions, leases, and SDK response types |
| Additional execution providers | Provider-neutral `host`, `isolation`, and `virtualization` execution contract on the Agent | Advertise honest platform and guarantee capabilities, enforce the requested mode, supervise the workload, measure usage, cancel it, and clean it up | Hypervisor, container, VM, image, socket, and provider configuration types |
| Dynamic infrastructure provisioning | `octacity-agent-provisioning-protocol` | Run a verified adapter host and durable reconciler for idempotent provision, observe, terminate, bootstrap, retry, and recovery | vSphere, Proxmox, cloud, image, network, machine, and provider SDK types |

## Cross-cutting rules

Every later adapter follows the existing dependency direction:

```text
transport or process adapter -> application/core port or versioned protocol
composition root             -> selected concrete adapter
core/application             -X-> provider SDK or provider representation
```

The following rules apply to all eight seams:

- The provider-neutral side owns bounded inputs, stable outcomes, cancellation,
  and classified failures. The adapter owns SDK translation and diagnostics.
- Configuration selects an installed adapter explicitly. Discovery alone never
  enables a capability, weakens admission, or makes readiness depend on it.
- Credentials cross the seam only as protected handles or short-lived grants.
  Raw credentials, tokens, TOTP seeds, and secret values cannot enter durable
  Build state, JobSpec, REST or GraphQL output, audit metadata, logs, or metrics.
- Provider names and SDK types cannot enter Project policy, Pipeline, Build,
  Attempt, Job, Agent, Pool, Orchestrator, Placement Scheduler, or shared wire
  contracts. Those contracts express required behavior and guarantees.
- An optional configured adapter reports unavailable capability independently.
  It becomes a process-readiness dependency only when an operator explicitly
  configures it as mandatory for that process.
- Correctness remains durable and replayable. A broker notification, process
  wake-up, SDK callback, or in-memory timer is never the authoritative fact.
- Each adapter receives failure, timeout, cancellation, replay, credential
  redaction, and cleanup tests before the composition root can advertise it.

## Seam-specific constraints

### Operator identity and management transports

V1 management remains an explicitly acknowledged trusted-network interface;
it must not imply an authenticated operator. Later authentication wraps the
management adapter before application dispatch and supplies the audit actor.
It does not add authentication branches to domain commands.

LDAP and TOTP are mechanisms behind an operator-identity port, not attributes
of a Project or Build. A future identity implementation may combine mechanisms,
but application handlers receive only a stable actor identity and bounded
authorization facts. Secret enrollment material never crosses that port.

GraphQL is a sibling transport to REST. Resolvers call the same application
commands and queries and cannot read SQL adapters, object storage, provider
hosts, or domain persistence directly. REST and GraphQL may evolve their own
versioned DTOs without turning either representation into an application type.

### Brokers and managed source integrations

Kafka or NATS may transport committed outbox entries and disposable wake-ups.
PostgreSQL sequence, idempotency, claim, fencing, and retry state remains the
source of truth. Losing, duplicating, or reordering a broker message can delay
work but cannot invent or erase accepted work.

GitHub and Gerrit implementations run behind the existing verified webhook and
VCS process protocols. Their SDK models are translated inside the adapter
process. The server sees exact bounded webhook bytes, normalized authenticated
events, immutable revisions, bounded repository data, credential handles, and
classified outcomes. Adding a provider cannot add provider cases to application
commands or bypass the durable webhook retry state.

### Secrets, execution, and capacity

A secret-store adapter implements the existing secret-provider port. It may
prefer delegated access and provider leases over returning bytes. If values
must be read, they live only in scoped zeroizing grants and are never persisted.

An additional execution provider implements an existing execution mode; a
provider name is backend identity, not a new mode. It can be advertised only
after the released-machine contract proves its platform identity, filesystem,
network, process, resource, image-integrity, cancellation, accounting, and
cleanup claims. Unsupported guarantees cause admission rejection, not fallback
to a weaker backend.

Dynamic infrastructure provisioning remains separate from placement. A future
reconciler may change the inventory of registered Agents, but it cannot create
a Lease, assign a Job, or inject provider objects into scheduling. Its durable
desired state, idempotency keys, observed state, retry claims, and bootstrap
records must survive process restart. With no configured production adapter,
the capability stays unavailable while static placement and readiness remain
unchanged.

## V1 dependency evidence

`tools/check_architecture.py` enforces the compiled boundary from Cargo
metadata. API, application, core, shared, and protocol layers retain narrow
per-layer dependency allowlists. Composition, infrastructure, Agent, CLI, and
test packages use a reviewed v1 external-package inventory because those layers
legitimately select concrete implementations. The check also rejects known
operator-authentication, LDAP/TOTP, GraphQL, Kafka/NATS, GitHub/Gerrit,
secret-store, and deferred execution or provisioning SDK families in every
layer.

Consequently, a new external package name cannot enter an implementation-owning
layer silently, while a named deferred SDK cannot be approved accidentally by
another layer rule. Moving an extension into production requires an explicit
architecture-policy change together with its implemented vertical slice; an
empty crate or unused dependency is not acceptable evidence.
