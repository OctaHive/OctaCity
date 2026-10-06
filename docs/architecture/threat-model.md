# Server and Agent threat model

This document records the security assumptions and required controls for the
first OctaCity server release. It is a release input: a change that introduces
a trust boundary, credential class, externally supplied payload, executable,
or durable data flow must update this model and its executable evidence.

## Scope and assumptions

The model covers the server process, static Agents, Octa and runner release
bundles, the optional static operator console and same-origin reverse proxy,
provider adapter processes, PostgreSQL, S3-compatible object storage, and the
cache, artifact, webhook, VCS, and management protocols between them. The
operating system, selected execution backend, database, object store,
release-signing infrastructure, and explicitly configured reverse proxy are
trusted according to their documented responsibilities.

Repository contents, build commands, source repositories, webhook payloads,
management request bodies, browser state and input, Agent protocol messages
before authentication, adapter output, object-store responses, and cached
bytes are untrusted.
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
| PostgreSQL | Concurrent replicas and restored state | Use-case-shaped transactions, declarative constraints, fencing, atomic audit/idempotency/outbox evidence, migration and race contracts |
| Object storage and cache | Missing, stale, corrupt, or cross-scope bytes | Private layouts, digest verification, immutable identities, scoped grants, transactional visibility, corruption and outage tests |
| Provider adapters | Executable, manifest, stdout frames, diagnostics | Regular-file and permission validation, digest lock, bounded versioned protocol, deadline and forced termination, redacted errors |
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
