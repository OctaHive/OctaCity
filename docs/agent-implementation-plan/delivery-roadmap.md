# Agent Delivery Roadmap

This part of the implementation plan records implementation phases, release gates, deferred work, and the first coding milestone.

## Implementation phases

### Phase 0: lock the contracts

- Pin the released `octa-runner-protocol` crate and validate its bundled
  input/output schemas and version constants in contract tests.
- Define server-agent protocol v1 schemas and golden examples.
- Define signed `JobSpecV1`, lease, fencing, events, acknowledgements,
  completion, resource usage, host capacity, and artifact DTOs.
- Define the `CoordinatorClient` and `ExecutionBackend` behavioral contracts.
- Define source-plugin v1 manifests, messages, golden examples, cancellation,
  and terminal results; defer provider schema publication to a future manifest
  version.
- Define the server-side `ArtifactStore` boundary and S3 object model.
- Record the security and trust model in an ADR.
- Pin Rust MSRV and dependency policy.

Completion gate: invalid signatures, unknown protocol versions, oversized
payloads, stale leases, and unknown fields are rejected by contract tests.

### Phase 1: agent process and inventory

- Create the Cargo workspace and focused protocol, configuration, source,
  runner, execution-port, backend-adapter, and composition-root crates.
- Implement strict TOML configuration and path/permission validation.
- Implement structured tracing and secret-safe error types.
- Calculate agent and installed release inventory.
- Inventory configured execution backends and operator-installed source
  plugins.
- Report static host capacity and validate advisory heartbeat snapshots.
- Invoke and validate `octa-runner capabilities`.
- Add `octacity-agent validate` for installation diagnostics.

Completion gate: an operator can validate a machine without contacting the
server or executing repository code.

### Phase 2: source-plugin host and Git provider

- Implement source-plugin registry discovery, manifest and settings validation,
  permission checks, digest verification, bounded JSONL transport,
  cancellation, and terminal results.
- Implement `octacity-source-git` with the fixed safe checkout sequence.
- Isolate Git configuration and credentials.
- Enforce exact revisions, path rules, timeouts, output limits, and quotas.

Completion gate: the Git plugin materializes an exact fixture revision through
the published source protocol; malformed or untrusted plugins are rejected;
malicious fixture repositories cannot execute host hooks, filters, submodules,
or helpers; and credentials are absent from arguments, protocol output, and
logs.

### Phase 3: local job lifecycle and execution backends

- Define one retained backend contract suite before adding platform and OCI
  implementations.
- Verify the signed request and its lease binding at the coordinator boundary,
  then pass the verified `JobSpec` to one backend-neutral job owner before any
  filesystem or plugin activity. That owner materializes source, starts the
  exact selected backend, supervises `octa-runner`, and owns cleanup.
- Complete Linux `NativeBackend` with cgroup v2, Bubblewrap namespaces,
  seccomp, quota-backed storage, and enforceable network modes.
- Implement `OciBackend` with explicit `process` and `hypervisor` isolation.
- Implement containerd process isolation on Linux and Microsandbox hypervisor
  isolation for Linux guests on supported Linux and Apple Silicon macOS agents.
- Keep every engine's agent-owned journals, FIFOs, and metadata below the
  validated `state_root`. An explicitly configured containerd daemon owns its
  content and snapshot stores; OctaCity labels and removes only its own runtime
  objects. Never select a cloud backend, Podman Machine, ambient socket, or home
  directory implicitly.
- Prove the same bidirectional non-PTY runner protocol transport through every
  supported Native and OCI combination.
- For OCI, mount the workspace/state tree and complete Octa release bundle with
  the required permissions and apply image, resource, timeout, and network
  policies. Add the identity mount only with real identity provisioning.
- Execute the same real Octafile through Native and both OCI isolation tiers.
- Collect cumulative CPU, memory, disk, and I/O usage for the complete job
  boundary in every backend; report network counters only where accurate.
- Implement graceful cancellation, forced termination, orphan cleanup, and
  complete destroy for all supported combinations.
- Remove abandoned backend resources and only recognizably agent-owned job
  directories before accepting work after restart.

Completion gate: the same leased fixture succeeds through Native and the
supported OCI process/hypervisor matrix using the shared contract. Runtime and
isolation selection are signed and explicit, an unavailable isolation tier
never falls back, and no mode leaves a runner, plugin process, container, VM,
or job filesystem state behind. CPU, memory, disk, and I/O accounting includes
child plugin processes and terminal totals agree with the backend's
authoritative counters.

Implementation status: complete for the initial strict matrix. The signed
request and execution dispatch use the final top-level `native | oci` model.
OCI routes by an exact guest platform and `process | hypervisor` capability;
Microsandbox is an OCI hypervisor engine rather than a third top-level mode.
The backend-neutral job owner, Linux Native backend, Linux containerd process
engine, and Microsandbox hypervisor engine all implement the same retained
runner lifecycle.

The real contract has passed for Linux Native, Linux containerd with overlayfs
and runc v2, and a Linux Microsandbox guest on Apple Silicon macOS. Every run
used a signed job, digest-pinned image where applicable, the complete Octa
release, bidirectional runner JSONL, terminal resource accounting, graceful
cancellation, backend destruction, workspace removal, and a second orphan
cleanup pass. Exploratory warm success-and-cancel measurements were about 3.7
seconds for Native, 5.1 seconds for containerd, and 0.9 seconds for
Microsandbox with its RAM-backed root overlay; dedicated release workers must retain their
own comparable measurements. Exact provisioning and combined coverage commands
are documented in [`backend-contract-tests.md`](../backend-contract-tests.md).

Windows Native/containerd/Hyper-V and host-native macOS execution are future
matrix extensions. They are not Phase 3 claims and must pass this same contract
on dedicated workers before being advertised.

### Phase 4: coordinator transport and lease loop

- Implement the HTTPS `CoordinatorClient` without exposing HTTP types to the
  job state machine.
- Register the agent and inventory.
- Acquire a lease through cancellable long polling, resampling local disk
  admission between polls and advertising it as `accept_jobs`.
- Verify signed JobSpec and fencing data.
- Run independent heartbeat and drain handling.
- Implement bounded retries and idempotency keys.

Completion gate: protocol integration tests cover disconnects, retries,
timeouts, duplicate responses, lease expiry, fencing, and shutdown while
polling.

Implementation status: complete. `octacity-protocol` owns strict registration,
inventory, lease, fencing, heartbeat, host-snapshot, directive, and structured
error DTOs with checked golden examples. `octacity-coordinator` provides the
transport-independent client boundary, a bounded Reqwest HTTPS adapter,
same-key idempotent retries, signed lease polling, sticky drain handling, and
an independent heartbeat monitor that cancels on explicit cancellation,
fencing, shutdown, or the monotonic lease safety deadline. `octacity-inventory`
collects verified component inventory and cross-platform advisory host
capacity without coupling transport to runner, source, or backend adapters.
Integration tests exercise real TCP disconnects, retryable responses, response
correlation, body and time bounds, cancellation while polling, invalid
signatures, drain, fencing, and renewal failure. Production job acquisition is
exposed by the binary through the phase-5 durable lifecycle; phase 4 alone did
not execute leases because unreportable jobs would have been unsafe.

### Phase 5: durable events and complete lifecycle

- Implement the single-owner job state machine.
- Add append-only bounded event spool and contiguous acknowledgements.
- Preserve runner event schemas and sequence numbers.
- Add agent lifecycle events without mixing them into Octa events.
- Spool periodic resource samples and include the latest snapshot in heartbeat
  requests without making heartbeat the durable metrics channel.
- Make completion idempotent and fenced.
- Recover by cleanup, not job resumption, after agent restart.

Completion gate: killing the network during a large-output job produces no
loss or reordering; reconnection replays duplicates safely; lost leases cancel
the selected backend. Resource samples obey the same ordering, replay, and
deduplication guarantees as other agent lifecycle events.

Implementation status: complete. `octacity-lifecycle` owns the one-attempt
state machine above `octacity-job` and `octacity-coordinator`, while its
concrete spool uses synced immutable per-sequence records and a synced
contiguous acknowledgement cursor. Byte, record, batch, and in-memory channel
limits are explicit configuration. Runner events retain their original schema
and sequence inside fenced attempt envelopes; lifecycle and cumulative
resource samples remain separate agent events, and the latest sample is also
published through heartbeat snapshots. Event append and terminal completion
use bounded idempotent coordinator calls, completion waits for all event
acknowledgements and confirmed cleanup, and startup destroys backend/workspace
orphans before removing only journal-proven interrupted state. The production
`run` command now performs registration, cancellable polling, one-job
execution, drain, durable completion, and graceful signal handling. Protocol,
real-HTTP, spool-pressure, disconnect/replay, ordering, fencing, and recovery
tests enforce these boundaries. Fencing or expiry ends and cleans only the
current attempt; the long-lived daemon continues polling. Event delivery
retries only failures explicitly safe for another idempotent retry cycle and
surfaces permanent rejection to the lifecycle owner.

### Phase 6: secrets, artifacts, and reports

- Provision short-lived workload identity through both execution backends.
- Verify that Octa retrieves a Vault secret without exposing it to the agent
  protocol or logs.
- Revalidate artifact/report paths and quotas on the host.
- Stream files and deterministic directory archives with digests.
- Implement `S3ArtifactStore` in the protocol test server and verify it against
  MinIO.
- Upload through short-lived presigned targets without giving the agent S3
  credentials.
- Complete uploads before the fenced terminal completion.

Completion gate: a real job uses Vault, publishes an arbitrary-format report
and an artifact, and no secret appears in logs, events, results, spool, journal,
or uploaded metadata.

Implementation status: complete. A local profile
is resolved into a bounded private identity lease, revoked after backend
destruction, and exposed read-only at the same fixed path in Native,
containerd, and Microsandbox execution. Runner declarations are rebound to
their `run_id` and `task_id`, host-revalidated, frozen into private staging,
and uploaded through fenced begin/PUT/complete operations before workspace and
terminal completion. Directory artifacts have a bounded, cross-platform,
deterministic tar format; upload URLs, headers, redirects, deadlines, retries,
digests, count limits, and byte limits are independently enforced by the
agent. The narrow server-side `ArtifactStore` boundary has an S3-compatible
implementation that keeps credentials, buckets, and physical keys private,
verifies uploaded bytes instead of trusting ETags, and publishes them under
immutable keys. Unit, lifecycle, protocol-golden, and loopback transport tests
cover local boundaries. The phase-six contract test additionally runs the
real Octa runner against Vault and the real HTTP begin/PUT/complete flow
against MinIO, then checks retained job state and uploaded metadata for
protected bytes.

The Phase 6 HTTP server is deliberately a contract fixture, not a production
server or durability claim. The server roadmap must persist begin/complete
upload records and their idempotency keys in PostgreSQL before the server is
considered real; the S3 adapter remains unchanged behind `ArtifactStore`.

### Phase 7: Octa local and remote cache session

- Add a strict optional cache policy to the signed job contract containing
  only namespace and independent read/write permission; cache endpoints and
  bearer credentials are not repository-controlled inputs.
- Add agent configuration for the persistent cache root, local capacity,
  retained trust-scope count, allowed remote origins, Native environment
  identity strings, request deadlines, and transfer limits. Signed jobs may
  narrow but never enlarge these values.
- Add a fenced coordinator operation that returns a short-lived cache session
  credential. Store it in a private per-job file and revoke/remove it after
  runner shutdown without placing its value in serialized state.
- Construct the published Octa runner-protocol `CacheSessionSpec`. Use its
  mandatory local L1 and optional HTTP L2 simultaneously; do not introduce an
  OctaCity cache engine or S3 client in the agent.
- Map the L1, token, and optional CA paths into Native, OCI process, and OCI
  hypervisor execution without exposing host paths in the guest request.
- Extend inventory checks so an agent advertises cache support only when the
  installed runner declares the required runner and cache protocol versions.
- Forward Octa's semantic cache events and terminal cache outcome unchanged;
  do not emit per-blob agent events.
- Test a local hit, a remote hit across two isolated agent instances, remote
  degradation with a working L1, read-only and write-only grants, namespace
  isolation, corrupt remote content, token expiry, cancellation, and cleanup.

If the current runner protocol cannot carry an operator-selected L1 capacity,
extend that protocol before enabling the feature rather than relying on an
implicit agent hardcode.

Completion gate: agent A publishes one cacheable build through Octa; agent B,
with an empty L1 but the same authorized namespace and runtime identity,
restores it from the remote L2 without executing the task. Both agents retain
verified local L1 copies, and neither observes the cache bearer value.

Implementation status: component-complete; the release-level completion gate
remains part of Phase 9. The signed job carries only namespace and
read/write authority, while fenced begin/revoke operations supply a bounded,
short-lived remote grant. `octacity-cache-session` narrows that grant through
operator origin, network, runtime-identity, capacity, deadline, and transfer
policy. Octa applies per-scope GC limits; Microsandbox hard-limits each scope,
while process backends require an aggregate-bounded dedicated filesystem. It
provisions a persistent trust-scoped L1 plus a private per-job
bearer file and removes the bearer after runner shutdown. Native, containerd,
and Microsandbox project the same fixed guest paths, and runner protocol v3
carries explicit L1 watermarks without an agent-side cache implementation.
Inventory advertises only the exact installed Octa cache capabilities.
Lifecycle tests cover fenced ordering, failures, and cleanup, while Octa's published
two-process HTTPS contract proves remote publication, restore into an empty
second L1 without task execution, local retention, corruption repair, and
credential non-disclosure. This proves the data plane and agent composition
boundaries, but does not claim that two released agent processes and the
durable server vertical slice already exist. Phase 9 closes that final gate.

### Phase 8: hardening and packaging

- Add hardened service definitions for systemd, Windows Service Control
  Manager, and macOS launchd.
- Build signed Linux, Windows, and macOS Octa release artifacts with capability
  manifests and checksums; package the agent and host-native source plugins for
  every supported agent platform.
- Add startup orphan cleanup and disk-pressure behavior.
- Run dependency audit, fuzz protocol parsers, and test corrupt local state.
- Document installation, enrollment, rotation, upgrades, draining, and
  recovery.

Completion gate: every supported target produces a deterministic,
self-verifying archive whose configuration and service assets pass portable
validation. Exercising those assets against a clean machine and a real server
vertical slice belongs to the Phase 9 release matrix; Phase 8 does not simulate
that operational boundary with mocks.

Implementation status: component-complete. The Octa source pin targets
the published `v0.4.0` release containing the required machine-readable release
contract and matching `0.4.0` protocol crates. Deterministic
archives contain the agent, the host-native Git source plugin and its
digest-bound manifest, service
assets, an internal checksum inventory, and operator documentation for Linux
amd64/arm64, Windows amd64, and Apple Silicon macOS. Release workflows publish
separate SHA-256 files and Sigstore-backed GitHub build-provenance
attestations. The prepared Octa release workflow includes its generated runner
capability manifest and uses the same checksum and attestation boundary;
`.github/octa-source-revision` identifies that release commit, and the release
workflow validates its contract before packaging OctaCity.

The packaged service runs under a dedicated non-root identity. Linux runtime
privileges are opt-in drop-ins: Native receives only an operator-delegated
cgroup subtree, Microsandbox receives KVM access, and containerd socket access
is explicitly identified as root-equivalent host authority without exposing
the socket to repository code. The Windows package uses a real SCM dispatcher
and control handler, mapping service stop into the worker's cancellation token.
Startup destroys backend-owned orphans before
recovering journal-proven attempts. Idle admission reserves the complete
workspace, spool, output-staging, and next-scope cache growth allowance;
reclaims only inactive recognized cache scopes by host-only LRU metadata;
groups bind mounts by device or volume identity; exclusively owns its cache
root; fails closed on corrupt state; and observes shutdown between traversed
cache entries. Disk pressure is advertised on every long poll so assignments pause without
hiding drain, and Windows service tracing is routed to its registered
Application Event Log source rather than an unattached stderr stream.
Portable CI validates service assets and deterministic packages;
scheduled security jobs audit both lockfiles and fuzz all JSON/TOML protocol
decoders, shared JSONL frame decoders, and signed JobSpec verification. The
remaining released-machine end-to-end matrix is Phase 9 rather
than being simulated here.

### Phase 9: Agent Ready test matrix

Run black-box tests against a released Octa bundle and a server vertical slice
provided by the separate server roadmap. The server fixture must persist fenced
upload and cache authorization records in PostgreSQL, but implementing that
server persistence is not an agent-phase task:

- successful job and failed task;
- malformed or incompatible runner;
- invalid runner/plugin/image digest;
- invalid JobSpec signature and stale fencing token;
- cancellation during source acquisition, backend start, runner execution,
  upload, and shutdown;
- server disconnect and restart during event delivery;
- slow server and full event spool;
- large binary stdout/stderr;
- workspace paths with spaces and Unicode;
- malformed source plugins, malicious Git repositories, and artifact symlinks;
- Vault authentication, renewal, revoke, and redaction;
- agent crash followed by orphan cleanup and server requeue;
- host reboot during each active execution backend;
- repeated cleanup and completion requests;
- disk exhaustion and artifact quota violation;
- CPU, memory, disk, and I/O accounting for descendant processes;
- transient and repeated resource-sampling failures;
- duplicate resource samples after reconnect and final peak/cumulative totals;
- local cache hit and remote cache hit across separate agents;
- remote-cache outage, corrupt content, expired credential, namespace
  isolation, and read-only/write-only cache grants;
- installation on a clean machine followed by validation, service start,
  restart, an atomic version upgrade, drain through the server control plane,
  and complete service removal using the packaged assets and documented
  commands.

The initial end-to-end matrix covers Linux Native, Linux OCI process isolation,
and Linux OCI hypervisor guests on supported Linux and Apple Silicon macOS
agents. Future Windows and host-native macOS combinations enter the matrix only
after their implementations pass the same suite on dedicated workers. A
platform/isolation pair is not advertised merely because its code
cross-compiles.

Completion gate: the released-machine matrix completes the documented install,
validate, run, restart, upgrade, drain, and removal lifecycle without relying
on repository build-tree paths or administrator-owned job state.

## Server roadmap handoff

Dynamic machine provisioning is deliberately not an agent phase. The future
server owns the narrow `AgentProvider` lifecycle (`provision`, `observe`, and
`terminate`) and adapters for vSphere, Proxmox, and cloud APIs. A provider
creates a prepared machine with a short-lived enrollment token; the agent then
connects outbound through the same protocol as a statically enrolled agent.
Provider credentials, pool policy, template selection, machine fencing, and
idempotent VM termination remain server concerns and never enter
`ExecutionBackend` or the agent job state machine.

## Performance tests

Fix thresholds before the release run and preserve raw samples as CI artifacts,
not committed result files. Measure at least:

- idle agent CPU and RSS;
- long-poll request rate while idle;
- lease-to-source-start latency;
- source materialization latency;
- Native and OCI process/hypervisor start/destroy latency by platform;
- agent overhead around the same `octa-runner` fixture across the supported
  runtime matrix;
- sustained event throughput and peak memory with large output;
- event replay after a simulated outage;
- CPU and allocation overhead of five-second resource sampling;
- graceful and forced cancellation latency;
- artifact streaming throughput and memory use;
- 100 sequential jobs with no leaked processes, VMs, descriptors, or disk.

Required invariant: queues and memory stay bounded under a slow or unavailable
server. Any material regression against the previous released agent must be
investigated or explained before release.

## Definition of done

The agent is ready for the server's first production vertical slice when an
external server can:

- register and inventory the agent;
- grant a signed, fenced, expiring lease;
- materialize an exact revision through a digest-verified, operator-installed
  source plugin without executing repository-controlled code on the host;
- run the required Octa release and locked plugins through explicitly selected
  Native and OCI process/hypervisor modes without fallback;
- receive durable per-job resource samples and authoritative terminal CPU,
  memory, disk, and I/O totals for the complete execution boundary;
- receive ordered replayable events and exactly one terminal completion;
- cancel at any lifecycle phase and observe bounded cleanup;
- receive validated artifacts and arbitrary-format reports through an
  S3-compatible object store without exposing storage credentials to agents;
- run Octa's local L1 and optional remote L2 task-result cache through a
  bounded, namespace-isolated, short-lived cache session without reimplementing
  cache semantics in the agent;
- let Octa resolve Vault secrets through workload identity without leakage;
- survive agent, server, and network failures without accepting stale results;
- confirm that no backend state, process, credential, or workspace remains
  after the job.

## Explicitly deferred

- server queue and scheduler implementation beyond the protocol test server
  until the Agent Ready gate; they are the next implementation milestone;
- Web UI;
- multiple simultaneous jobs per agent;
- all dynamic agent providers, including vSphere, Proxmox, and cloud APIs;
- Podman OCI integration until it can satisfy the same lifecycle and isolation
  contract as the initial containerd and Microsandbox engines;
- Kubernetes executor;
- Docker socket passthrough;
- submodules and Git LFS;
- additional source plugins beyond Git;
- task-time VCS plugins and remote Octafile sources in Octa;
- agent-managed plugin or Octa downloads;
- automatic self-update;
- production remote cache API, authorization persistence, retention, and
  S3-compatible backing store, which belong to the server roadmap; the agent
  is verified against a protocol contract server;
- non-S3-compatible artifact stores;
- multipart artifact upload until object-size requirements justify it;
- arbitrary server-to-agent administration commands.

These features require real product demand. They must not weaken the explicit
trust boundary between Native, OCI process isolation, and OCI hypervisor
isolation.

## First coding milestone

Do not begin with registration screens or a broad framework. The first code
milestone is a retained end-to-end test and its production implementation:

```text
signed fixture lease
  -> exact fixture revision through octacity-source-git
  -> selected ExecutionBackend
      -> NativeBackend
      `-> OciBackend(process | hypervisor) by image digest
  -> identical octa-runner hello/start/events/finished contract
  -> graceful backend destroy
  -> verified empty process, VM, credential, and workspace state
```

The milestone is complete only when the runtime matrix passes the retained
contract suite and an unavailable OCI mode or isolation tier never selects a
weaker alternative. Then add the real long-poll lease loop, durable delivery,
S3-compatible uploads, and operational packaging around the same path.

[Back to the implementation-plan index](../agent-implementation-plan.md)
