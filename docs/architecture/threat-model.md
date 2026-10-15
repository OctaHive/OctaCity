# Server and Agent threat model

This document records the security assumptions and required controls for the
first OctaCity server release. It is a release input: a change that introduces
a trust boundary, credential class, externally supplied payload, executable,
or durable data flow must update this model and its executable evidence.

## Scope and assumptions

The model covers the server process, static Agents, Octa and runner release
bundles, the optional static operator console and same-origin reverse proxy,
provider adapter processes, the opt-in Dark Factory control plane, PostgreSQL,
S3-compatible object storage, and the cache, artifact, webhook, VCS, and
management protocols between them. The
operating system, selected execution backend, database, object store,
release-signing infrastructure, and explicitly configured reverse proxy are
trusted according to their documented responsibilities.

Repository contents, build commands, source repositories, webhook payloads,
management request bodies, browser state and input, Agent protocol messages
before authentication, adapter output, object-store responses, and cached
bytes are untrusted.
Factory task bodies, Stage outputs, model responses, external Work identities,
and delivery-provider responses are also untrusted. They are evidence or input,
never authority to choose an undeclared lifecycle transition or widen execution
permissions.
Direct Host execution deliberately gives repository code the Agent host's
security boundary. It is therefore allowed only by explicit Project and Pool
policy and is not suitable for mutually untrusted workloads.

The v1 management API has no operator authentication, login, RBAC, or ABAC. An
application-layer policy authorizes only its canonical anonymous
trusted-network context before dispatch and grants that context unrestricted
visibility. It is safe only on a trusted network behind the documented listener
and proxy boundary. A public or multi-tenant deployment requires a later
identity-aware policy; network placement is not treated as user identity.

## Assets

- authoritative Project, Build, Attempt, Job, Lease, audit, idempotency, and
  outbox state;
- immutable Factory Configurations, Work Envelopes, Run and Stage histories,
  exact candidate/evidence/decision provenance, claims, budgets, and current
  projections;
- signing keys, Agent enrollment and registration credentials, webhook keys,
  provider credentials, cache grants, and presigned transfer capabilities;
- source, logs, artifacts, reports, cache entries, and their Project scope;
- the console's content-hashed assets, proxy policy, and bounded local
  presentation preferences;
- release binaries, manifests, checksums, dependency graph, and provenance;
- service availability, bounded storage, worker ownership, and scheduling
  fairness.

Raw secret values are not domain data. They must remain inside the owning
provider or bounded workload delivery mechanism and must not enter JobSpecs,
management responses, audit facts, metrics, traces, logs, or durable errors.

## Trust boundaries

| Boundary | Untrusted input | Required control and evidence |
| --- | --- | --- |
| Management ingress | HTTP path, query, headers, and JSON | Separate listener, explicit trusted-network acknowledgement, bounded decoding and rate limits, typed application handlers, OpenAPI drift tests |
| Browser console and proxy | UI input, URL state, browser storage, static files, response headers | Same-origin relative API paths, omitted credentials, stripped identity and CORS headers, allowlisted bounded preferences, CSP, framing/referrer/MIME policy, immutable asset checks, released browser slice |
| Agent and cache ingress | Registration credentials, signed intent, fences, events, uploads | Credential authentication, protocol negotiation, signature and expiry verification, lease fencing, bounded frames, replay and mismatch tests |
| Webhook ingress | Provider headers and exact request bytes | Exact-body authentication before decoding, bounded payloads, deduplication, provider-process isolation, negative signature tests |
| Repository and VCS | Locator, refs, trees, content, malicious repositories | Operator-installed digest-locked adapter, credential handles rather than values, read-only bounded operations, traversal and hook-execution tests |
| Execution | Repository-controlled commands and outputs | Explicit Host admission or qualified isolation/virtualization, dedicated work roots, resource/output limits, cancellation, cleanup, backend conformance matrix |
| Operator-selected task executable | Executable bytes, configured product/version/platform/digest, plugin environment selector | Startup-time regular-file, ownership, ACL, digest, platform, release-compatibility and plugin-lock verification; opaque read-only backend projection; bounded version probe where host-executable; process-tree cleanup and leak scans |
| PostgreSQL | Concurrent replicas and restored state | Use-case-shaped transactions, declarative constraints, fencing, atomic audit/idempotency/outbox evidence, migration and race contracts |
| Object storage and cache | Missing, stale, corrupt, or cross-scope bytes | Private layouts, digest verification, immutable identities, scoped grants, transactional visibility, corruption and outage tests |
| Provider adapters | Executable, manifest, stdout frames, diagnostics | Regular-file and permission validation, digest lock, bounded versioned protocol, deadline and forced termination, redacted errors |
| Factory admission and reconciliation | External Work identity, mutable source reference, task artifacts, model and Stage results, concurrent workers | Replay probe before mutable resolution, exact immutable subject, enabled-configuration and WIP admission transaction, bounded snapshots, monotonic versions and budgets, owner/deadline/fence checks, immediate lifecycle-transition validation, atomic audit and outbox append |
| Factory side effects | Build creation, Decision Signal, delivery and reporting requests; missing or ambiguous responses | Stable operation and input digests, durable pending/claimed/terminal rows, bounded claim expiry and takeover, idempotent observe-before-retry, fenced settlement, no provider response as lifecycle authority |
| Release supply chain | Third-party crates, actions, tools, service images, archives | Locked dependencies, pinned actions/toolchains/images, advisory and license/source gates, checksums and attestations |
| Diagnostics | Errors, URLs, headers, identifiers, output text | Pre-persistence redaction, closed metric labels, bounded diagnostics, secret-leak assertions and repository secret scan |

## Threats and mitigations

### Spoofing and privilege escalation

An attacker may impersonate an Agent, replay an enrollment token, forge a
webhook, reuse a stale Lease, or request a stronger execution mode. Agent
credentials are scoped and rotated, enrollment is single-use and expiring,
webhooks are authenticated over their exact bytes, and every mutation that
depends on ownership checks the active fencing token. Placement intersects the
immutable Project policy with exact Pool and Agent capabilities; provider names
cannot broaden the provider-neutral guarantee.

### Tampering and confused-deputy behavior

An attacker may alter a JobSpec, substitute a plugin or release binary, inject
a host path or transfer URL, corrupt an object, or exploit an adapter as a
deputy. JobSpecs are server-derived and signed, executables and immutable bytes
are digest-verified, and caller-controlled intent cannot contain credentials,
host paths, fences, or presigned URLs. Adapters receive bounded commands and
credential handles only. PostgreSQL commits state, idempotency, audit, and
outbox evidence atomically.

Operator-selected task executables are an explicit extension of the Agent
trust base, not repository input. The Agent verifies them before source
discovery or coordinator contact, and isolated backends expose them from a
fixed read-only tools root under opaque deterministic names. The current
JobSpec v1/v2 contract has no per-Job tool selector: when a tool is configured,
every admitted Job on that Agent can reach it through the release-owned plugin
selector. Operators must therefore use a dedicated Agent and Pool constrained
to the intended Projects and must not install model credentials in that
profile. Factory Permission Sets in JobSpec v3 provide signed per-Job selection
for qualified v3 Jobs; v1/v2 are deliberately unchanged and still require this
deployment scope. Per-action Codex bounded control remains disabled until its
exact-action adapter and Agent-local broker path are release-qualified.

Factory admission stores the unresolved command intent as its idempotency
fingerprint before mutable capability or source resolution. An exact retry
therefore returns the original immutable result even if a Project is later
disabled or aliases change; a mismatched retry conflicts. Admission counts
active Runs under the same store lock that inserts Work, so concurrent callers
cannot exceed the immutable configuration WIP limit. Reconciliation may append
only an immediate code-owned lifecycle successor. Every Stage Attempt records
its creating owner, deadline and fence; terminal observations record their own
fenced owner and consumed budget.

### Configured Flow, private execution inputs, and Decision Signal

An admitted Run pins its complete immutable Flow Definition closure, schemas,
projections, transitions, repeat and expansion limits, permission ceilings and
budgets. Repository files and provider results cannot replace that closure or
add a route. New configuration publication affects later admissions. Control
dependencies establish ordering; they grant no Context, protected test,
credential, Artifact or mount access. A task consumes only its explicit frozen
projection and exact selected retained inputs.

The installation selects technical capabilities through exact immutable
identities and qualified authority. Logical Flow and node names do not select
executors. Provider wire decoding belongs to its adapter; the common engine
accepts independently verified schema-bound facts. A read-only compiler without
verified ChangeSet materialization must reject candidate context rather than
execute the base revision while claiming candidate provenance. Writable authors,
independent reviewers, protected validators, delivery and promotion require
separate installed credentials and enforceable backend grants.

Private compiler inputs are reserved before object-store writes, published under
the current fenced operation and exact content identities, and excluded from
ordinary output discovery. Transfer expiry cannot exceed the original execution
deadline. Cleanup deletes the physical object before acknowledging deletion,
including reservations left by interrupted writes. Selected Artifact reads
independently verify Project ownership, exact size and digest, byte ceilings and
retention through the consumer's execution window.

Decision Signal reserves its full call ceiling before external I/O. An unknown
response retains that reservation; loss of a response or a changed capability
cannot invent zero consumption. Exact receipt replay performs no new call.
Only an atomic fresh reservation permits provider evaluation. Concurrent or
uncertain retries can read the existing evaluation through the neutral
observation contract, but cannot evaluate again. Unsupported, unknown or
wrong-request observations leave usage pending. A matching measured outcome
received after the original deadline settles as a timeout with its actual
usage; read-only recovery does not extend dispatch authority.
Cancellation revokes dispatch, including replayed authorization. Settlement of
an already reserved call requires a current claim and records measured usage
without completing a cancelled node. Shadow and advisory modes preserve the
deterministic baseline. Tool-risk proposals remain subject to deterministic
permission checks and exact-action authorization independently of any signal.

The lease-bound server authorizer resolves the actual ordinary Job and latest
Attempt, persisted Factory Build origin, initial acceptance bridge, frozen Node
input and current Run claim. Initial hook calls can precede the owner's
execution-link append, so the authoritative acceptance bridge supplies that
binding without inventing a new Build. Retries require the retained execution
link and exact ordinary ancestry. Terminal Builds, replaced leases, stopped
Nodes and cancelled Runs cannot release an action. Receipt publication obtains
the current Run claim again after a provider call; the final lease check can
refuse the action while retaining the call's measured consumption.
Tool-risk reservation also locks and validates the actual registration, lease,
Job and latest Attempt in its journal transaction. It requires the signed
tool-risk control mode and rereads the database clock after lock acquisition;
a lease that expires while waiting for the Run lock cannot authorize I/O.

An installed tool-risk policy pins its provider/model/questions/calibration,
consuming mapping, separate credential and host, Project/local grants and finite
call allowance into the workload's opaque task digest. Its provider authority
must be granted by the admitted overall Flow ceiling and remain outside the
worker's signed credential list. A changed installation cannot silently alter
an admitted workload. The Signal journal enforces the dedicated allowance
atomically; worker settlement includes consumed Signal usage and waits for
unresolved reservations. Runtime call deadlines are enforced by a neutral
connector decorator, so the application never depends on a provider SDK or
asynchronous runtime.

Same-transaction audit facts and post-commit structured events contain only
bounded state, immutable digests, usage, budgets and fencing identities. Private
task or question text, provider payloads, protected tests, commands, paths,
credential material and provider aliases are excluded. Rejected mutations and
exact replay do not emit a fresh committed event.

Release qualification must exercise these controls through the released server
and Agent on every selected backend, alongside portable graph, schema, journal
and adapter contracts. Portable tests alone cannot prove filesystem, process,
network, secret or resource isolation. The released pilot must cover restarts,
unknown responses, immutable manifests and receipts, candidate lineage,
independent-agent isolation and policy-governed promotion before these boundaries
are considered complete.

The adversarial qualification cases cross explicit trust boundaries:

| Untrusted proposal | Authoritative check | Required observable refusal |
| --- | --- | --- |
| Repository or retrieved text asks to change policy, execute instructions, or invent a route | The admitted definition closure, typed outcomes, exact input projections and deterministic tool permissions | The closure and permitted action remain unchanged; an undeclared outcome cannot complete the node |
| Repository supplies its own Flow definition or a recursive/unbounded subflow | Operator-owned publication and complete closure validation before admission | No Run, source download, provider call or Job dispatch is accepted |
| A control predecessor names another node's private context or protected tests | Explicit Context projection, selected Artifact identity, Project scope and independent consumer permissions | No undeclared bytes or transfer grant are delivered to the consumer |
| Provider substitutes a model, executable, policy, cost basis or result schema | Exact installed identities, immutable task policy and independently verified execution provenance | No substituted result or invented zero usage settles the node; unknown consumption stays reserved |
| Workload changes a mount, path, credential purpose, signed manifest or executable | Signed v3 intent, independent local grants, read-only staging, toolchain revalidation and selected backend controls | Failure occurs before source or spawn, with a fixed diagnostic and no Host fallback |
| Author or reviewer forges evidence, rewrites a ChangeSet, or reports a different candidate | Trusted output publication, size/digest checks, exact subject and candidate ancestry | No evidence, candidate, delivery or promotion authority is derived from prose |
| An old cycle, expired owner or duplicate external response attempts settlement | Current cycle/claim/fence and stable operation, input and receipt identities | No second logical side effect, transition or fresh committed event is created |
| A tool authorization returns allow as the leased execution is cancelled | Agent cancellation is rechecked after the coordinator exchange, before releasing the unchanged action | The blocked action remains denied even when allow and cancellation arrive in one poll |
| Hook identifiers or adapter errors embed commands, paths or credential canaries | Closed diagnostics and redacted formatting of provider-controlled fields | The canary is absent from diagnostics, logs, events, receipts and public discovery |

The executable coverage map and portable release-gate commands are maintained
in [Factory adversarial contracts](../testing/security.md#factory-adversarial-contracts).
Restoring persisted admission must recheck relationships as well as structural
validity: a replacement definition is not the originally admitted definition,
even when its logical identity and version match. Restore must recheck the exact
root digest against the retained root Run before interpreting any route.

Portable parser, compiler, graph and broker tests establish these logical
refusals; they do not establish released-backend isolation. The ordinary
released Codex fixture exercises redaction, overflow and cancellation, and the
v3 backend contracts exercise protected mounts and local permission ceilings.
Factory qualification additionally requires the released server and Agent to
execute the configured nested Flow, including the installed blocking tool gate,
independent author/reviewer/verifier roles and trusted candidate reconstruction.
Absence of that wiring or of selected-backend evidence must remain a missing
qualification, rather than being replaced by an in-memory success or a broader
execution route.

### Cross-project disclosure and secret leakage

An attacker may traverse artifact paths, reuse cache authority, place secrets
in output, or cause credentials to appear in diagnostics. Project and Lease
scope are checked at every transfer boundary, paths are normalized beneath
owned roots, and redaction occurs before log persistence and indexing. Secret
wrappers redact formatting, observability accepts only closed safe fields, and
the security contract matrix exercises archive, search, cache, URL, adapter,
and telemetry leakage paths. Gitleaks additionally rejects credential-shaped
material committed to either release source tree.

The console adds no browser credential or general persistence API. One
versioned adapter may retain only bounded language, theme, explorer, favorite,
recent, and browser-local notification timestamp values. API payloads,
notification items, mutation bodies, request identities, logs, query caches,
and download capabilities remain in memory or are discarded. Artifact
capabilities are obtained immediately before navigation and are never rendered
or stored.

### Browser script, origin, and identity confusion

An attacker may try to inject script through server data, frame the console,
serve a JavaScript asset with the wrong MIME type, attach cookies or bearer
credentials, or make a reverse proxy imply an authenticated operator. React
renders server values as data, the package forbids source-bearing or
environment-bound output, and the proxy applies same-origin CSP,
`frame-ancestors 'none'`, `nosniff`, no-referrer, exact MIME types, and
file-safe SPA fallback rules. Browser requests use `credentials: 'omit'`; the
proxy strips authorization and forwarded-user headers and hides upstream CORS
headers. The neutral operator menu never claims a user identity or logout
flow.

### Denial of service and resource exhaustion

An attacker may send oversized frames, expensive searches, excessive events,
adapter output, or workloads that ignore cancellation. Input sizes, page sizes,
output chunks, diagnostics, retry counts, worker batches, rate-limit state, and
execution resources are bounded. Heartbeats have independent admission so
ordinary overload cannot silently expire healthy work. Adapter and runner
processes have deadlines and a force-stop path. Durable work can be reclaimed
after process loss without relying on an in-memory queue.
Factory outbox work follows the same rule: an unknown response remains claimed
until expiry, after which another worker appends a new attempt under the same
logical operation identity. Stale owners cannot settle it.

### Repudiation, replay, and rollback ambiguity

Accepted mutations record immutable audit facts and stable idempotency outcomes
in their authoritative transaction. Trigger occurrences, events, completions,
uploads, cache sessions, and worker claims use stable identities and fencing.
Backup restore, migration rollback, and released-Agent lifecycle gates retain
non-secret evidence sufficient to distinguish safe replay from a new action.

### Parser and dependency compromise

Protocol parsers may panic, allocate without bound, or accept ambiguous input;
dependencies may acquire a known vulnerability, incompatible license, or
unreviewed source. Strict decoders reject unknown or oversized structures, and
the server-Agent, runner, source-plugin JSON/JSONL, TOML manifest, signed
JobSpec, and signature paths are fuzzed on a pinned nightly toolchain. RustSec
audits both lockfiles. `cargo-deny` rejects dependencies outside the reviewed
license set or crates.io/path source policy. Any exception requires a narrow,
reasoned configuration change rather than a CI bypass.

## Residual risks and deployment obligations

- Management v1 remains unauthenticated. Operators must keep it on a trusted
  network and preserve ingress separation when terminating TLS at a proxy.
  Deploying the console does not add login, sessions, RBAC, ABAC, or operator
  identity: reachability of the console origin grants the same anonymous
  management authority as direct reachability of the private listener.
- Host mode is not a sandbox. Use Isolation or Virtualization for untrusted
  repository code and dedicate Agent identities and machines where Host is
  unavoidable.
- A v1/v2 Agent with `tool_executables` is a dedicated tool profile. It must
  not share a Pool with general workloads or carry ambient model credentials;
  the configured digest grants execution authority to every admitted Job.
- Containerd control is host-equivalent authority, KVM and Apple
  Virtualization.framework are privileged resources, and their sockets or
  devices must never be exposed to repository code.
- S3-compatible storage, PostgreSQL, DNS, the operating system, and release
  distribution remain part of the deployment trust base. Least-privilege
  credentials, encryption, patching, backups, and access logging are operator
  responsibilities.
- Secret scanning detects credential-shaped material; it does not prove that
  arbitrary business data is non-sensitive. Code review and provider-side
  rotation remain required after any suspected disclosure.

## Release evidence

No single test closes this model. A releasable revision must pass all of these
independent gates without `continue-on-error`, ignored warnings, or blanket
lint allowances:

1. portable formatting, architecture, repository-tool, Clippy
   (`-D warnings`), rustdoc (`-D missing_docs`), unit, contract, and coverage
   checks plus locked frontend generation, format, lint, type, unit,
   accessibility, browser, bundle-budget, package, and archive-verification
   gates from `ci.yml`;
2. production and fuzz lockfile audits, dependency license/source review,
   secret scanning of both release source trees, and bounded protocol fuzzing
   from `security.yml`;
3. the release-qualified backend, failure, security, lifecycle, and performance
   matrices from `backend-contracts.yml`;
4. locked release builds, deterministic manifests and checksums, provenance
   attestations, the PostgreSQL-backed released server/console same-origin
   browser slice, and only then publication from `release.yml`.

The release workflow calls the first three workflows directly, so a manually
dispatched or tagged release cannot publish merely because a separate workflow
was green on an older revision.
