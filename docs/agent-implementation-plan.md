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
|- crates/
|  |- octacity-agent/       # agent CLI and composition root
|  |- octacity-config/      # configuration parsing and intrinsic validation
|  |- octacity-execution/   # backend-neutral execution ports and DTOs
|  |- octacity-execution-native/ # strict Linux Native adapter
|  |- octacity-execution-oci/ # OCI process/hypervisor adapter and engine boundary
|  |- octacity-execution-microsandbox/ # Microsandbox OCI hypervisor engine
|  |- octacity-protocol/    # versioned server-agent DTOs and signatures
|  |- octacity-runner/      # Octa inventory, protocol client, and supervisor
|  |- octacity-source/      # trusted source registry and process host
|  |- octacity-source-plugin/ # source-plugin protocol and plugin SDK
|  `- octacity-source-git/  # first trusted source plugin
|- protocol/
|  |- agent/v1/             # server-agent JSON Schemas and examples
|  `- source/v1/            # source-plugin JSON Schemas and examples
|- tests/
|  `- fixtures/             # fake runner and Git repositories for tests
`- docs/
```

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
[`docs/protocols/signed-job-spec-v1.md`](protocols/signed-job-spec-v1.md).

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
  optional workload identity profile (rejected until identity provisioning is enabled)

outputs
  artifact size/count limits
  report size/count limits
```

The following are not allowed in `JobSpecV1`:

- host executable paths;
- host shell commands;
- arbitrary host mounts;
- arbitrary environment inheritance;
- server credentials;
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
enabled_runtime_modes
allow_native_execution
native_environment
native_linux_cgroup_root
native_linux_bubblewrap_executable
native_linux_readonly_paths
native_linux_pids_limit
oci_engines
allowed_upload_origins
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
[`crates/octacity-source-plugin/README.md`](../crates/octacity-source-plugin/README.md).

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
/run/octa-identity      future short-lived workload identity, read-only
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

## Resource accounting

Resource enforcement and resource accounting are separate responsibilities.
The backend must enforce every signed limit independently of whether a sample
is currently being delivered. Metrics are never treated as a security boundary
or used as a polling-based substitute for cgroups, VM limits, or filesystem
quotas.

Every `RunningExecution` exposes one backend-neutral cumulative snapshot:

```rust
pub struct ResourceUsage {
    pub observed_at_unix_ms: u64,
    pub elapsed_ms: u64,
    pub cpu_time_ms: u64,
    pub memory_current_bytes: u64,
    pub memory_peak_bytes: u64,
    pub disk_current_bytes: u64,
    pub disk_peak_bytes: u64,
    pub io_read_bytes: u64,
    pub io_written_bytes: u64,
    pub network_received_bytes: Option<u64>,
    pub network_transmitted_bytes: Option<u64>,
}
```

CPU time and byte counters are cumulative so the server can calculate rates
without inheriting backend-specific notions of percentages. CPU, memory, disk,
and I/O are required. Network counters are optional in protocol v1 because a
backend may enforce a network policy without obtaining accurate per-job byte
counts. Missing counters are represented as unavailable, never as zero.

Linux Native accounts for the entire job cgroup rather than the runner PID, so
plugin processes and all descendants are included. OCI process isolation
accounts for the whole container, and OCI hypervisor isolation accounts for the
whole utility VM or microVM. Disk usage covers all disk-backed writable
job-owned storage, including workspace and Octa state. RAM-backed filesystems
are charged to memory instead. Quota or backend accounting is preferred where
the backend exposes live usage; an off-thread workspace traversal is the
fallback for an enforced bind-mount quota that exposes no public usage counter.

While a job is running, the agent samples usage every five seconds. Each sample
is an agent lifecycle event with the job, attempt, lease fencing data, and event
stream sequence, and therefore follows the normal spool, retry,
acknowledgement, and deduplication rules. A heartbeat may carry the latest
sample for live visibility but is not its durable transport. Sampling stops
only after backend destruction, and the terminal job result includes final
cumulative CPU and I/O counters plus peak memory and disk usage. The server may
downsample retained time-series data without changing the terminal totals.

Failure to read one sample emits a bounded diagnostic and marks it unavailable;
it does not silently manufacture zero usage. Repeated accounting failure makes
the backend unhealthy and fails the job when required terminal totals cannot be
obtained.

## Runner supervision

Before starting a job, the agent:

1. Matches the signed requirements against its startup inventory.
2. Recomputes the runner and plugin digests from the read-only bundle.
3. Creates a request ID from the job and attempt.
4. Starts `octa-runner` through the selected execution backend.
5. Requires a valid `hello` within a short timeout.
6. Checks runner, event, plugin, and Octafile protocol compatibility.
7. Sends exactly one `Start` request using paths mapped by that backend.

During execution it:

- continuously reads stdout so runner backpressure remains bounded;
- rejects oversized, malformed, out-of-order, or incorrectly correlated
  protocol messages;
- preserves every valid runner event and its original sequence number;
- bounds captured stderr and treats it only as infrastructure diagnostics;
- maps server cancellation, lease loss, local shutdown, and wall timeout to an
  idempotent runner `Cancel`;
- escalates to backend termination after `graceful_cancel_timeout`;
- accepts exactly one terminal `Finished` as the execution result;
- creates an infrastructure result if the runner exits without a valid terminal
  message.

The agent never infers job success from the runner process exit code alone.

## Durable event delivery

The agent wraps runner and agent lifecycle events in an attempt stream:

```text
job_id
attempt
lease_id
fencing_token
stream_sequence
kind
payload
```

Runner events keep their own `schema_version`, `run_id`, and `sequence` inside
the payload. Agent lifecycle events cover source acquisition, backend
preparation, resource usage, upload, cancellation, and cleanup without
pretending to be Octa runtime events.

Write each envelope to a bounded append-only local spool before sending it.
The server acknowledges the largest contiguous `stream_sequence`. On reconnect,
the agent resends from the next unacknowledged sequence. Duplicate batches are
valid and server-side insertion is idempotent by `(job_id, attempt,
stream_sequence)`.

The spool has explicit byte and record limits. If it fills, reading runner
stdout pauses through normal backpressure rather than dropping or reordering
events. Heartbeats continue independently. Prolonged coordinator loss expires
the lease and cancels the selected backend, bounding disk and process lifetime.

Completion is sent only after all event batches and artifact completion records
are acknowledged and local cleanup is confirmed. The completion operation is
idempotent.

## Secrets and workload identity

The server and agent never resolve application secret values. A job references
an Octa secrets profile and logical secret names. The agent provides:

- a short-lived workload identity file at a backend-mapped runtime path
  (`/run/octa-identity` inside an OCI guest and a restricted per-job path for
  Native);
- a secrets profile containing provider addresses, roles, mounts, and identity
  paths but no resolved values;
- an outbound network policy allowing only the required identity and Vault
  endpoints.

The identity source is configured on the agent host and copied or mounted with
restrictive permissions for one job. Native execution does not inherit the
agent's complete environment or credentials. Identity material is removed
during cleanup and never appears in event payloads, diagnostics, command
arguments, or persistent job metadata.

Octa performs Vault login, renewal, secret reads, redaction, and revoke through
the already published secret-provider contract.

## Artifacts and reports

After a valid terminal result, the agent first stops and destroys the
`RunningExecution` so no runner or plugin process can continue changing the
workspace. It then removes the workload identity while retaining the host
workspace for upload. Only after this freeze does the agent read artifact and
report declarations, without interpreting report formats. Before upload it
independently validates:

- the path is relative to the logical workspace root;
- canonical resolution stays inside the workspace;
- symlinks cannot escape;
- the entry is a regular file or directory, not a device or socket;
- per-file, total-size, and count limits;
- files have not changed between validation and streaming where the platform
  can detect it.

Regular files are streamed with a SHA-256 digest. Directories use one documented
deterministic archive format. The server returns short-lived upload targets only
after an authorized `artifacts:begin` call bound to the current fencing token.
Upload retries are idempotent by artifact ID and digest.

Reports use the same byte upload path plus their producer-owned `format`
identifier. The agent does not contain a `JUnit`, `Cobertura`, or `SARIF` enum.

### S3-compatible object storage

PostgreSQL stores artifact/report metadata, ownership, digests, sizes, and
upload state. Artifact and report bytes live in an S3-compatible object store.
The initial server implementation uses one configured S3 API endpoint and must
work against AWS S3 and MinIO; other compatible services require configuration,
not new agent code.

The server owns a narrow `ArtifactStore` interface for object naming, upload
authorization, completion, download authorization, and deletion. Its first
implementation is `S3ArtifactStore`. S3 SDK types, bucket names, credentials,
and object keys never cross into domain or server-agent protocol types.

The upload flow is:

1. The agent calls `artifacts:begin` with lease fencing, logical metadata,
   expected size, and SHA-256.
2. The server persists an upload record and returns a short-lived presigned PUT
   target bound to an opaque artifact ID.
3. The agent verifies the target origin against its configured allowlist, then
   streams bytes directly with bounded retries and no S3 credentials.
4. The agent calls `artifacts:complete`; the server verifies current fencing,
   recorded size, digest, and object-store state before publishing the object.

The first version may impose a single-PUT size limit. Multipart upload is added
only when required, but its eventual parts and completion remain hidden behind
the same begin/complete protocol. Local development and integration tests use
MinIO rather than a separate filesystem storage behavior.

## Local state and cleanup

Use distinct roots:

```text
state_root/
  agent.json
  jobs/<lease-id>/journal
  jobs/<lease-id>/events

work_root/
  jobs/<lease-id>/workspace
  cache/<trust-domain>/<project>/<platform>/
```

The first version shares persistent Octa state only inside the same project and
trust domain. Pull requests from untrusted forks receive an isolated cache
namespace. Do not add remote-cache code until the server has a real backend.

Cleanup is idempotent and ordered. Freezing after runner completion performs
steps 1 through 3 before artifact validation; final cleanup performs the
remaining steps after upload:

1. Stop or kill the selected `RunningExecution`.
2. Remove persisted backend state.
3. Revoke or delete workload identity.
4. Remove temporary Git credentials.
5. Remove the workspace without following symlinks.
6. Retain the bounded event spool and journal until server acknowledgement.
7. Send terminal completion only after cleanup has been confirmed.
8. Remove the local job directory after completion is acknowledged.

Startup performs the same cleanup for every incomplete journal before acquiring
a new lease.

## Graceful agent shutdown

On SIGTERM or service stop:

- stop acquiring leases immediately;
- notify the server that the agent is draining;
- cancel the active runner;
- wait for the configured grace period;
- force-kill and destroy the active `RunningExecution` if needed;
- attempt to flush terminal diagnostics;
- exit non-zero if backend cleanup could not be confirmed.

The service manager may apply an outer deadline, but the agent must normally
finish cleanup before that deadline.

## Observability

Emit structured tracing logs with agent, job, attempt, lease, and request IDs.
Never log bearer credentials, signatures' payload bytes, Git credentials,
workload identity, environment values, or runner protocol payloads before their
secret redaction guarantees are established.

The initial binary accepts a `tracing-subscriber` filter through `--log-filter`
and `RUST_LOG`, and writes compact or JSON events to stderr through
`--log-format`. Modules use only `tracing` call sites, allowing a service build
to replace the subscriber or forward events without changing job logic. CLI
arguments must never carry credentials or secret values.

Initial metrics:

- registration and lease request outcomes;
- lease heartbeat latency and failures;
- active job state and duration;
- source acquisition, backend start, runner, upload, and cleanup durations;
- event spool bytes, records, and acknowledged sequence;
- event and artifact retry counts;
- cancellation reason and escalation count;
- orphan cleanup count;
- agent CPU, available memory, filesystem free space, and backend health;
- job CPU time, current and peak memory, current and peak writable disk, and
  cumulative I/O;
- per-job network bytes when the selected backend can measure them accurately;
- resource-sampling failures and unavailable counters.

A local diagnostic command may print configuration validation and inventory,
but the production daemon exposes no unauthenticated network listener.

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
are documented in [`backend-contract-tests.md`](backend-contract-tests.md).

Windows Native/containerd/Hyper-V and host-native macOS execution are future
matrix extensions. They are not Phase 3 claims and must pass this same contract
on dedicated workers before being advertised.

### Phase 4: coordinator transport and lease loop

- Implement the HTTPS `CoordinatorClient` without exposing HTTP types to the
  job state machine.
- Register the agent and inventory.
- Acquire a lease through cancellable long polling.
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

### Phase 7: hardening and packaging

- Add hardened service definitions for systemd, Windows Service Control
  Manager, and macOS launchd.
- Build signed Linux, Windows, and macOS Octa release artifacts with capability
  manifests and checksums; package the agent and host-native source plugins for
  every supported agent platform.
- Add startup orphan cleanup and disk-pressure behavior.
- Run dependency audit, fuzz protocol parsers, and test corrupt local state.
- Document installation, enrollment, rotation, upgrades, draining, and
  recovery.

Completion gate: a clean machine can install, validate, run, restart, drain,
upgrade, and remove the agent using documented commands.

### Phase 8: Agent Ready test matrix

Run black-box tests against a minimal real server implementation of protocol
v1 and a released Octa bundle:

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
- duplicate resource samples after reconnect and final peak/cumulative totals.

The initial end-to-end matrix covers Linux Native, Linux OCI process isolation,
and Linux OCI hypervisor guests on supported Linux and Apple Silicon macOS
agents. Future Windows and host-native macOS combinations enter the matrix only
after their implementations pass the same suite on dedicated workers. A
platform/isolation pair is not advertised merely because its code
cross-compiles.

### Phase 9: server-side agent providers

- Define the narrow `AgentProvider` lifecycle around provision, observe, and
  terminate operations. Draining remains scheduler state, not a provider API.
- Implement `VsphereAgentProvider` and `ProxmoxAgentProvider`; statically
  enrolled agents bypass this boundary, and cloud providers are independent
  adapters.
- Keep provider credentials and APIs on the server. A provisioned machine gets
  a short-lived enrollment token and initiates its own authenticated outbound
  connection.
- Learn scheduling capabilities only from an authenticated agent registration,
  not from template labels supplied by a provider.
- Support persistent, idle-retired, and single-job disposable pool policies.
- Fence a lost machine before requeueing its job, and make termination
  idempotent without treating a timeout as proof that a VM is gone.

Completion gate: vSphere and Proxmox can each create a prepared Linux or
Windows agent from an immutable template, run one fenced job, return its
artifacts and events, and destroy the disposable machine without coupling
provider APIs to `ExecutionBackend` or the job state machine.

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
- let Octa resolve Vault secrets through workload identity without leakage;
- survive agent, server, and network failures without accepting stale results;
- confirm that no backend state, process, credential, or workspace remains
  after the job.

## Explicitly deferred

- server queue and scheduler implementation beyond the protocol test server
  until the Agent Ready gate; they are the next implementation milestone;
- Web UI;
- multiple simultaneous jobs per agent;
- additional dynamic agent providers beyond vSphere and Proxmox;
- Podman OCI integration until it can satisfy the same lifecycle and isolation
  contract as the initial containerd and Microsandbox engines;
- Kubernetes executor;
- Docker socket passthrough;
- submodules and Git LFS;
- additional source plugins beyond Git;
- task-time VCS plugins and remote Octafile sources in Octa;
- agent-managed plugin or Octa downloads;
- automatic self-update;
- remote cache backend;
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
