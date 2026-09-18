# OctaCity Agent Implementation Plan

Status: in progress

## Goal

Build a small self-hosted agent that receives one leased job from OctaCity,
materializes an exact source revision through a trusted source plugin, executes
the released `octa-runner` through an explicitly selected Native or OCI
backend, delivers an ordered and replayable event stream, uploads declared
artifacts and reports, and completely removes the job after completion.

The agent is a supervisor and transport client. It does not parse Octafiles,
build DAGs, execute Octa plugins itself, resolve secret values, or contain a
second implementation of the Octa runtime.

The agent binary is portable across Linux, Windows, and macOS, but execution
support is capability-driven: an agent advertises only the guest platforms,
runtime modes, and isolation tiers its configured backends can actually
enforce. The initial strict matrix is Linux Native, Linux OCI process execution
through containerd, and Linux OCI hypervisor execution through Microsandbox on
supported Linux and Apple Silicon macOS hosts.

## Fixed decisions

- The agent is written in stable Rust using Tokio.
- The command line is parsed by `clap`; implementation modules emit structured
  `tracing` events and do not depend on a concrete collector.
- Octa remains a separate repository and a separately released product.
- The only execution integration is the versioned `octa-runner` process
  protocol. OctaCity uses the published `octa-runner-protocol` wire crate but
  never imports Octa's executor, parser, output, plugin, or runtime crates.
- All agent-to-server connections are outbound HTTPS. The server never connects
  to an agent; source plugins use only operator-allowed VCS endpoints.
- Job acquisition uses HTTPS long polling. WebSocket is not required for the
  first agent protocol.
- The first agent executes one job at a time.
- Runner execution always goes through one narrow `ExecutionBackend` contract.
  The two product modes are `Native` and `OCI`. An OCI request additionally
  selects `process` or `hypervisor` isolation and an immutable image digest.
  The requested mode and isolation tier are signed and explicit; there is no
  automatic fallback between OCI isolation tiers or from OCI to Native.
- Primary workspace acquisition uses a versioned OctaCity source-plugin
  protocol. The first trusted plugin is `octacity-source-git`; source plugins
  are distinct from Octa task plugins and are never loaded from a repository.
- An agent accepts only a signed, versioned `JobSpec`, not arbitrary remote
  commands.
- Installed Octa and plugin bundles are provisioned by the operator. The first
  agent is not a package manager or self-updater.
- Each Octa bundle includes the bounded output of `octa-runner capabilities`
  as `octa-runner-capabilities.json`. Inventory reads the manifest so a macOS
  host can validate a Linux guest release without executing it. The live runner
  still confirms its protocol versions through `Hello` after backend startup.
- Event delivery is at-least-once and idempotent. A bounded local spool allows
  replay after transient network failure.
- The agent reports host capacity separately from per-job resource usage. Job
  usage is measured by the selected backend for the complete process tree and
  is delivered through the same durable, ordered event stream.
- A lost or fenced lease cancels execution. A stale agent can never publish a
  successful terminal result for a newer attempt.
- Secret values are resolved by Octa providers inside the job. The agent only
  supplies workload identity, provider configuration, filesystem mounts, and
  allowed network access.
- Artifacts and reports are stored by the server in an S3-compatible object
  store. Agents receive short-lived upload instructions and never receive
  object-store credentials.
- Task-result cache semantics remain entirely inside Octa. The agent supplies
  a job-scoped runner cache session: an operator-bounded persistent local L1,
  a server-authorized namespace and access mode, and optional short-lived
  credentials for the remote HTTP L2. Local and remote caches operate together;
  the agent never computes action keys, interprets cache records, or proxies
  cache blobs.
- Machine provisioning and job execution are separate boundaries. Server-side
  `AgentProvider` implementations create or retire ready agent machines through
  VMware vSphere, Proxmox, or cloud APIs; statically enrolled agents require no
  provider adapter. Agent-side
  `ExecutionBackend` implementations execute one job inside an already-running
  agent machine. Neither boundary depends on or calls back into the other.
- Crate dependencies point from composition and orchestration toward contracts;
  concrete adapters never depend on their consumers or on sibling adapters.
  Dynamic boundaries are the `CoordinatorClient`, `ExecutionBackend`, the OCI
  engine boundary, the source-plugin process protocol, and server-side
  `AgentProvider` and `ArtifactStore`. Shared wire DTOs and configuration are
  leaf crates with data APIs, not artificial traits. Private implementation
  details remain concrete.

## System boundary

```text
OctaCity Server
    |
    | outbound HTTPS: lease, heartbeat, events, completion, uploads
    v
OctaCity Agent
    |- validates configuration and server identity
    |- inventories the installed Octa release
    |- verifies signed JobSpec and lease fencing
    |- invokes a trusted source plugin for an exact revision
    |- selects Native or OCI execution without fallback
    |- supervises the octa-runner protocol
    |- spools and forwards events
    `- validates and uploads artifacts/reports
            |
            v
      ExecutionBackend
          |- NativeBackend
          |   `- platform-native process sandbox and resource controls
          `- OciBackend
              |- process-isolated OCI container
              `- hypervisor-isolated OCI container or microVM
                    |- prepared workspace
                    |- read-only Octa release and plugins
                    |- workload identity
                    `- octa-runner
                          `- plugin manager and plugin processes
```

The host agent must never execute an arbitrary command taken from the
repository or `JobSpec`. On the host it may run only configured agent
components, operator-installed source plugins, the selected execution backend,
and the verified `octa-runner` when native execution is explicitly allowed.
Octa tasks and plugins run wherever the selected execution backend places the
runner. Untrusted jobs must require OCI `hypervisor` isolation or a disposable
agent machine; OCI `process` and Native remain lower trust tiers.

## Provisioning and execution modes

The server provisions capacity; the agent executes jobs. These are deliberately
different interfaces even when both happen to use virtualization:

```text
OctaCity Server
`- AgentProvider
   |- VsphereAgentProvider
   |- ProxmoxAgentProvider
   `- future cloud providers
          |
          `- ready VM or machine containing OctaCity Agent
                 |
                 `- ExecutionBackend
                    |- NativeBackend
                    `- OciBackend(process | hypervisor)
```

An `AgentProvider` starts a prepared machine image whose agent connects outbound
to the server. Pool policy decides whether the machine is persistent, retired
after an idle timeout, or restricted to one job and destroyed. Provisioners do
not execute Octafiles and are not part of the job sandbox. This permits a
disposable Windows VM to execute one Native job without pretending that the
vSphere or Proxmox API is an agent runtime.

`NativeBackend` executes against the host toolchain without a guest OS:

| Host | Native containment | Initial status |
|---|---|---|
| Linux | cgroup v2 for limits/accounting, Bubblewrap namespaces, seccomp, and quota-backed writable storage | supported |
| Windows | no backend until one implementation enforces the complete signed resource and isolation contract | unavailable |
| macOS | no backend until one implementation enforces the complete signed resource and isolation contract | unavailable |

MXC may later be one implementation detail for Windows or macOS host process
containment, but its public ProcessContainer and Seatbelt policies alone do not
enforce every v1 CPU, memory, writable-disk, and network requirement. OctaCity
therefore does not advertise an MXC-backed Native capability or fall back to an
unrestricted host process. Windows and macOS builds currently require a
prepared or disposable machine whose advertised backend really supports the
requested workload.

`OciBackend` executes an immutable OCI image and treats isolation as an
independent required property:

| Agent host | `process` | `hypervisor` |
|---|---|---|
| Linux | containerd with runc/crun | a configured microVM-capable OCI engine, initially Microsandbox and later optionally containerd with a VM runtime |
| Windows | planned containerd with process-isolated runhcs | planned containerd with Hyper-V-isolated runhcs |
| macOS | unsupported as a native container boundary | a Linux guest through a supported virtualization backend, initially Microsandbox on Apple Silicon |

Podman may be added as another OCI engine where it can satisfy the same
contract. Podman Machine on Windows or macOS is a Linux VM and must advertise
Linux guest capability; it cannot advertise a Windows or macOS guest. A macOS
agent can run a Linux guest through Microsandbox, but a macOS build needs a
future strict Native backend or a disposable macOS machine because there is no
macOS OCI guest mode.

The scheduler matches the signed guest platform, architecture, runtime mode,
OCI isolation tier, and backend capabilities. A request for `hypervisor` can
never run through runc/crun or process-isolated runhcs, and an unavailable OCI
backend can never fall back to Native.

## Repository shape

Use one Cargo workspace with dependencies directed from the agent composition
root toward focused components and from adapters toward narrow execution
ports:

```text
octacity/
|- Cargo.toml
|- agent/
|  |- octacity-agent/       # agent CLI and composition root
|  |- octacity-config/      # configuration parsing and intrinsic validation
|  |- octacity-coordinator/ # bounded coordinator transport and lease client
|  |- octacity-execution/   # backend-neutral execution ports and DTOs
|  |- octacity-execution-containerd/ # Linux containerd process engine
|  |- octacity-execution-native/ # strict Linux Native adapter
|  |- octacity-execution-oci/ # OCI process/hypervisor adapter and engine boundary
|  |- octacity-execution-microsandbox/ # Microsandbox OCI hypervisor engine
|  |- octacity-identity/    # private per-job workload identity lifecycle
|  |- octacity-inventory/   # verified registration and host capacity snapshot
|  |- octacity-job/         # source, identity, runtime, and cleanup ownership
|  |- octacity-lifecycle/   # durable fenced attempt lifecycle and event spool
|  |- octacity-output/      # immutable output validation and upload pipeline
|  |- octacity-private-fs/  # private cross-platform filesystem primitives
|  |- octacity-runner/      # Octa inventory, protocol client, and supervisor
|  |- octacity-source/      # trusted source registry and process host
|  |- octacity-source-plugin/ # source-plugin protocol and plugin SDK
|  `- octacity-source-git/  # first trusted source plugin
|- server/
|  |- core/
|  |  `- octacity-artifact-store/ # backend-neutral server storage port
|  |- infrastructure/
|  |  `- octacity-artifact-s3/ # S3-compatible storage adapter
|  `- tests/
|     `- octacity-phase6-contract-tests/ # cross-product integration contract
|- shared/
|  |- octacity-protocol/    # versioned server-agent DTOs and signatures
|  `- protocol-fixtures/
|     `- coordinator/       # server-agent golden JSON documents
|- cli/                     # reserved for public command-line clients
|- tests/
|  `- fixtures/             # fake runner and Git repositories for tests
`- docs/
```

The source-plugin wire contract remains owned and documented by
`octacity-source-plugin`; it is not duplicated under `protocol/`.

`octacity-protocol` is justified because both the real agent and the upcoming
server consume the same wire contract. It contains only serializable protocol
types, validation, signing/verification, and schemas. It must not contain
database models, scheduler logic, HTTP clients, or agent state.

`octacity-source-plugin` is a separate contract because source acquisition
happens before an Octafile exists. It contains bounded JSONL messages, manifest
types, strict validation, and plugin-side helpers. It does not reuse the Octa
task-plugin protocol or import Octa internals.

`octacity-agent` is the composition root. Component crates never depend on it.
`octacity-config` validates configuration-owned invariants but does not create
source registries, runner inventories, or execution backends. The agent creates
and connects those concrete components.

`octacity-execution` owns the narrow `ExecutionBackend` and
`RunningExecution` ports. `octacity-runner` consumes those ports without
knowing which backend implements them. Native and OCI adapters depend on the
execution port and never on each other. Concrete OCI engines depend inward on
the OCI engine contract, not on orchestration or sibling engines. This keeps
the dependency graph acyclic and makes an unavailable isolation tier an error
rather than a reason to fall back to Native execution.

The runner layer verifies the signed Octa requirement and converts its trusted
installation inventory into a `RunnerProgram` containing only the executable,
plugin directory, and lock-file paths required to start a process. An execution
adapter receives that value through `ExecutionBackend::start`; it cannot reach
back into runner inventory or protocol logic. Consequently
`octacity-execution-native` depends only on `octacity-execution`, while
`octacity-runner` also depends only on that same port rather than on any
concrete backend.

VCS extensibility is provided by the process-level source-plugin protocol, not
one Rust trait implementation per VCS. `octacity-source` is the single trusted
host for that protocol; provider executables depend only on
`octacity-source-plugin`.

## Server-agent protocol v1

### Transport

- HTTPS with normal certificate validation.
- Bearer agent credential read from a permissions-restricted file, never from
  command-line arguments.
- JSON request and response bodies with explicit `protocol_version`.
- Request IDs on every call.
- Explicit lease-admission state on every long poll, so disk pressure pauses
  assignments without disconnecting the agent from drain control.
- Timeouts and response-size limits on every endpoint.
- Retry only operations defined as idempotent.
- Exponential backoff with jitter and a server-provided upper bound.

Initial endpoint groups:

```text
POST /api/v1/agents/register
POST /api/v1/agents/{agent_id}/leases:acquire
POST /api/v1/leases/{lease_id}/heartbeat
POST /api/v1/leases/{lease_id}/events
POST /api/v1/leases/{lease_id}/artifacts:begin
POST /api/v1/leases/{lease_id}/artifacts:complete
POST /api/v1/leases/{lease_id}/complete
```

The exact paths may change before protocol v1 is published, but the operations
and their idempotency must be represented in the schemas first.

### Registration and inventory

At startup the agent invokes `octa-runner capabilities` without executing a
job and reports:

- agent version and protocol versions;
- agent ID and configured labels;
- operating system and architecture;
- enabled execution backends and their availability;
- CPU, memory, disk, and virtualization availability;
- installed `octa-runner` version, SHA-256, build commit, protocols, event
  schemas, Octafile versions, and features;
- installed plugin names, versions, platforms, capabilities, and digests;
- installed source-plugin names, versions, protocols, platforms, and digests;
  schema descriptors may be added with a future manifest version.

Registration does not make the server trust self-reported security properties.
It supplies scheduling data. The agent independently validates every selected
runner, plugin lock, source plugin, applicable image digest, and signed job
before execution.

Registration includes static capacity such as logical CPU count and total
memory. Each heartbeat includes an advisory host snapshot: available CPU,
available memory, free space on the work and state filesystems, active job, and
backend health. The server may use these values for scheduling and draining,
but they are not trusted as proof that signed job limits are enforced.

### Lease and fencing

A lease contains at least:

```text
lease_id
job_id
attempt
fencing_token
issued_at
expires_at
signed_job_spec
```

Every heartbeat, event batch, artifact operation, and completion request carries
`lease_id`, `attempt`, and `fencing_token`. The server rejects stale fencing
tokens. The agent treats an explicit rejection, expiry, or inability to renew
within the configured safety margin as cancellation.

Heartbeats are independent from event delivery so a blocked or slow event
upload cannot silently expire a healthy lease. Every successful heartbeat
returns one explicit directive: `continue`, `cancel`, `fenced`, or `drain`.
Cancellation latency is therefore bounded by the configured heartbeat interval
without requiring a stateful WebSocket connection.

### Signed JobSpec

The implemented signed payload contract is specified independently in
[`docs/protocols/signed-job-spec-v1.md`](../protocols/signed-job-spec-v1.md).

Avoid JSON canonicalization. The server signs the exact serialized payload
bytes and sends an envelope:

```json
{
  "key_id": "server-signing-key-1",
  "algorithm": "ed25519",
  "payload": "base64-encoded-job-spec-json",
  "signature": "base64-encoded-signature"
}
```

The agent verifies the signature, key ID, size, version, timestamps, job ID,
attempt, and lease binding before deserializing or acting on the payload.
Verification keys are provisioned in agent configuration and support an
explicit overlap during rotation.

`JobSpecV1` contains execution intent, not arbitrary agent commands:

```text
source
  provider
  required plugin version and digest
  immutable revision
  optional mutable ref used only as a bounded lookup hint
  provider parameters validated authoritatively by the selected plugin

octa
  required version
  runner SHA-256
  runner protocol
  event schema
  plugin protocol
  required plugin digests

execution
  octafile path
  task commands
  variables
  arguments
  concurrency
  parallel
  failfast
  secrets profile path

runtime
  mode: native or oci
  guest platform and architecture
  isolation: process or hypervisor when OCI is selected
  OCI image by immutable digest when OCI is selected
  CPU and memory limits
  writable disk limit
  wall-clock timeout
  network policy
  optional workload identity profile resolved from operator configuration

outputs
  artifact size/count limits
  report size/count limits
  per-output byte limit

cache (optional)
  server-authorized namespace
  independent lookup and publication permission
```

The following are not allowed in `JobSpecV1`:

- host executable paths;
- host shell commands;
- arbitrary host mounts;
- arbitrary environment inheritance;
- server credentials;
- cache bearer credentials, endpoints, or physical cache paths;
- resolved secret values;
- source-plugin executable paths or unregistered plugin names;
- mutable source refs without an immutable resolved revision;
- mutable OCI tags without a resolved digest.

The server validates the provider-independent source fields before signing the
job. Provider-specific parameters remain subject to authoritative validation
by the selected plugin. A future manifest version may publish schemas for
earlier server and UI feedback, but those schemas do not replace plugin-side
validation. The agent verifies the operator-installed manifest and binary
digest and invokes only the fixed entrypoint resolved from that manifest.

## Agent configuration

Use one explicit TOML configuration file. Command-line flags select the file
and support harmless diagnostics such as `--version`; they do not duplicate all
configuration fields.

Required configuration includes:

```text
agent_id
server_url
credential_file
server_signing_keys
labels
work_root
state_root
octa_release_root
source_plugins_dir
workload_identity_profiles
cache.root
cache.capacity.max_bytes
cache.capacity.high_watermark_bytes
cache.capacity.low_watermark_bytes
cache.max_scopes
cache.allow_read
cache.allow_write
cache.allowed_remote_origins
cache.native_environment_identities
cache.request_timeout_seconds
cache.max_parallel_transfers
enabled_runtime_modes
allow_native_execution
native_environment
native_linux_cgroup_root
native_linux_bubblewrap_executable
native_linux_readonly_paths
native_linux_pids_limit
oci_engines
allow_unrestricted_network
allowed_network_hosts
allowed_upload_origins
max_archive_entries
upload_timeout_seconds
max_output_limits
max_workspace_bytes
max_spool_bytes
max_spool_records
event_batch_max_bytes
event_batch_max_records
event_channel_capacity
poll_timeout_seconds
coordinator_request_timeout_seconds
coordinator_max_body_bytes
retry_initial_delay_milliseconds
retry_max_delay_seconds
retry_max_attempts
heartbeat_interval_seconds
lease_safety_margin_seconds
graceful_cancel_timeout_seconds
cleanup_timeout_seconds
runner_hello_timeout_seconds
resource_sample_interval_seconds
resource_sample_timeout_seconds
max_accounting_failures
```

`allow_unrestricted_network` defaults to false. A restricted host requested by
a signed job must also appear in `allowed_network_hosts`; the job cannot widen
the operator-owned egress policy. Likewise, every signed `OutputLimits` value
must fit within `max_output_limits`, so a compromised coordinator cannot spend
more local staging space than the agent operator authorized.

Cache capacity and transport limits are operator policy. The signed job selects
only a namespace and a subset of locally allowed read/write access. Native
environment identities are configured by the operator; OCI identities are
derived from the already verified immutable guest image.

At startup the agent canonicalizes roots, rejects overlapping unsafe paths,
checks permissions, verifies that credentials are not world-readable, checks
free disk space, validates the configured execution backends, Octa release, and
source-plugin registry, and fails before registration if a security
requirement is not met. Enabling `NativeBackend` requires the explicit
`allow_native_execution = true` setting; its presence is never inferred from a
missing or unavailable sandbox.

Linux Native settings are required only when Native is enabled. OCI engine
settings are likewise explicit: containerd and Microsandbox are never
discovered from an ambient socket, environment variable, home directory, or
executable search path. Microsandbox configuration includes exact `msb` and
`libkrunfw` paths and its metrics sampling interval. Containerd configuration
includes its process and open-file ceilings. Startup capability probes
determine the exact guest platforms and isolation tiers the configured engines
can enforce.

Operational supervision limits belong to agent configuration: runner
handshake and accounting intervals, accounting-call timeouts, tolerated
accounting failures, process ceilings, and OCI file-descriptor ceilings are
operator policy. Wire framing and decoded-payload limits remain versioned
protocol constants because changing them changes interoperability and memory
safety rather than deployment tuning.

The process environment is not a hidden configuration layer. Native execution
starts with an empty environment and receives only the operator-owned
`native_environment` map. That map must define `PATH` so normal Octa tools can
be discovered without inheriting the agent service's credentials or ambient
deployment state. Job variables remain separate and are passed only through
the signed runner request.

## Single-job state machine

One Tokio task owns the mutable job state. Heartbeat, shutdown, and transport
tasks send typed messages to that owner. Do not spread job transitions across
shared `Arc<Mutex<_>>` objects.

```text
Idle
  -> Leasing
  -> Preparing
  -> Running
  -> Freezing
  -> Uploading
  -> Cleaning
  -> Completing
  -> Idle

Preparing | Running | Freezing | Uploading
  -> Cancelling
  -> Cleaning
  -> Completing
  -> Idle
```

Every transition is persisted in a small local job journal before its external
side effect. On agent restart, active jobs are not resumed: the agent destroys
orphaned backend state and workspaces, emits a structured operator diagnostic
containing the safe local attempt identifier, last valid phase, unacknowledged
event count, and completion-presence flag, and then lets the server's lease TTL
and fencing create a new attempt. Raw event payloads and server-controlled job
text are never copied into that recovery diagnostic.
There is no duplicate execution recovery path in v1.

## Source acquisition plugins

The canonical source-plugin v1 process specification, message examples, and
implementation checklist live in
[`agent/octacity-source-plugin/README.md`](../../agent/octacity-source-plugin/README.md).

Primary workspace acquisition happens before an Octafile can be loaded, so it
uses an OctaCity source-plugin protocol rather than the Octa task-plugin
protocol. A source plugin is an operator-installed executable with a versioned
manifest, protocol range, platform list, fixed entrypoint, SHA-256 digest, and
provider-specific operator settings.

`source_plugins_dir` is a self-contained, operator-managed registry:

```text
source_plugins_dir/
|- git/
|  |- plugin.toml
|  `- octacity-source-git
`- mercurial/
   |- plugin.toml
   `- octacity-source-mercurial
```

Each immediate child has one `plugin.toml` containing its logical name,
version, protocol range, supported platforms, relative executable path,
executable SHA-256, and plugin-specific settings. The v1 manifest does not
publish provider JSON Schemas; the plugin validates provider-specific values.
The agent does not keep a second allowlist: installing a valid plugin below
this operator-controlled directory authorizes it. Removing its directory
disables it. Duplicate names, malformed manifests, path escapes, unsupported
protocols, digest mismatches, and unsafe directory permissions make startup
validation fail rather than silently hiding a broken plugin. Job content can
select a logical name but cannot register, configure, replace, or locate a
plugin.

The agent sends a bounded request containing the assigned empty destination,
validated provider parameters, operator-controlled settings from the verified
manifest, credential handles, limits, and cancellation context. The plugin
emits structured progress and diagnostics and finishes with the exact
materialized revision and provenance. It cannot choose another host path,
execution backend, Octa release, or agent command.

Source plugins are trusted host components because they need network and
filesystem access before the job runtime starts. They must therefore:

- be installed in the operator-controlled plugin registry, never by repository
  content;
- be selected by logical name and verified digest, never by a JobSpec path;
- write only below the workspace assigned by the agent;
- receive credentials through restricted temporary files or handles, never
  command-line arguments or repository-controlled environment variables;
- support cancellation, timeouts, bounded output, and a structured terminal
  result;
- remove credentials before returning and never include values in diagnostics;
- return immutable revision identity suitable for audit and cache keys.

`octacity-source-git` is the first implementation. It runs a fixed Git sequence
without a shell: initialize an empty repository with isolated configuration,
add the validated remote, fetch an exact commit or bounded ref hint, verify the
commit, and check it out detached. It disables hooks, host configuration,
repository filters, submodules, and Git LFS; rejects local/file transports
unless the operator explicitly allows them; and enforces workspace quota.

Additional VCS implementations use the same source-plugin protocol. Task-time
VCS operations and remote Octafile includes belong to a future Octa feature and
are deliberately not implemented by the OctaCity agent.

## Execution backends

Job orchestration always uses a narrow execution interface:

```rust
#[async_trait::async_trait]
trait ExecutionBackend: Send + Sync {
    async fn start(
        &self,
        runner: &RunnerProgram,
        request: StartExecution,
    ) -> Result<Box<dyn RunningExecution>>;
    async fn cleanup_orphans(&self) -> Result<()>;
}

#[async_trait::async_trait]
trait RunningExecution: Send {
    fn paths(&self) -> &ExecutionPaths;
    fn take_io(&mut self) -> Result<ExecutionIo>;
    async fn sample_usage(&mut self) -> Result<ResourceUsage>;
    async fn wait(&mut self) -> Result<ExecutionExit>;
    async fn kill(&mut self) -> Result<()>;
    async fn destroy(self: Box<Self>) -> Result<()>;
}
```

`RunningExecution` is the backend-neutral interface returned to orchestration.
It owns the runner stdin, stdout, and stderr channels and returns paths mapped
into that execution environment. This lets one supervisor construct the same
runner request for host paths and OCI guest paths without knowing which backend
it received. It also provides resource sampling plus bounded `wait`, `kill`,
and `destroy` operations while keeping its concrete native
or OCI-engine handle private. Orchestration must not inspect
`tokio::process::Child`, containerd tasks, Microsandbox SDK handles, cgroups, or backend-specific
path construction.
The concrete Rust signatures may use pinned boxed I/O types, but the lifecycle
above is the complete behavioral boundary.

The product includes:

- `NativeBackend`, which starts only the configured and digest-verified
  `octa-runner` and plugin bundle through the host platform's Native containment;
- `OciBackend`, which starts the same release from an immutable OCI image using
  the exact signed `process` or `hypervisor` isolation tier.

The signed JobSpec selects one backend. The agent rejects a disabled,
unavailable, or incompatible backend. It never falls back between OCI
isolation tiers or from OCI to Native. Server scheduling must reserve Native
for trusted projects and agents; untrusted jobs require hypervisor-isolated OCI
execution or a disposable machine regardless of labels supplied by the repository.

Linux Native relies on the one-job-per-agent invariant and requires `work_root`
itself to be quota-backed. It combines cgroup v2 limits and accounting with
Bubblewrap namespaces, seccomp, and explicit filesystem/network policy. No
Windows or macOS Native implementation is advertised in v1: adding one
requires a real adapter and the same retained contract, not a
lowest-common-denominator wrapper or an advisory policy.

`OciBackend` owns the OCI lifecycle while engine adapters translate that
lifecycle to containerd, Microsandbox, or a later Podman integration. This
boundary is justified by multiple real engines; orchestration never depends on
an engine API. Engine capability discovery records guest platform, architecture,
and effective isolation. Merely accepting an OCI image or running inside Podman
Machine is not evidence of hypervisor isolation for the individual job.

Every backend advertises which limits and isolation properties it can enforce.
It must either enforce each signed runtime requirement or reject the job; a
Native backend must never silently ignore a requested CPU, memory, filesystem,
identity, or network restriction.

All implementations must expose the same runner lifecycle: bidirectional
non-PTY JSONL, bounded stderr, graceful cancellation, forced termination,
terminal status, resource sampling, and verified cleanup. A shared backend
contract suite runs against every supported host/isolation combination. Native
cleanup proves that no runner or plugin process remains; OCI cleanup additionally
proves that no container, VM, snapshot, or persisted engine state remains.

The OCI guest layout is fixed:

```text
/workspace             materialized source tree, read-write
/opt/octacity/octa      runner and plugins, read-only
/workspace/.octacity    per-job execution state under the same disk quota
/run/octa-identity      short-lived workload identity, read-only
```

The Microsandbox engine uses a RAM-backed OCI root overlay capped within the
VM memory allocation. This prevents writes to `/tmp`, `/root`, or another image
path from bypassing the signed disk boundary: `/workspace` is the only
disk-backed writable mount and its guest writes are quota-limited.

If an OCI engine does not expose the required bidirectional process channel,
resource enforcement, accounting, or cleanup lifecycle, that engine/isolation
combination is unavailable. Backend configuration defines CPU, memory, disk,
wall-clock timeout, immutable OCI image reference, and DNS/network policy. A
backend must never pretend that cgroups alone enforce filesystem or network
policy, or that a process container provides hypervisor isolation.

[Back to the implementation-plan index](../agent-implementation-plan.md)
