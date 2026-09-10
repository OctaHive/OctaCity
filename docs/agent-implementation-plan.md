# OctaCity Agent Implementation Plan

Status: in progress

## Goal

Build a small self-hosted agent that receives one leased job from OctaCity,
materializes an exact source revision through a trusted source plugin, executes
the released `octa-runner` through an explicitly selected native or isolated
backend, delivers an ordered and replayable event stream, uploads declared
artifacts and reports, and completely removes the job after completion.

The agent is a supervisor and transport client. It does not parse Octafiles,
build DAGs, execute Octa plugins itself, resolve secret values, or contain a
second implementation of the Octa runtime.

The first production target is Linux on `x86_64` and `aarch64`. Microsandbox
agents require hardware virtualization; explicitly native-only agents do not.
Windows and macOS agents can be designed after the Linux contract is proven.

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
  Version 1 includes `NativeBackend` and `MicrosandboxBackend` as real product
  modes. A requested backend is explicit and there is never an automatic
  fallback from isolated to native execution.
- Primary workspace acquisition uses a versioned OctaCity source-plugin
  protocol. The first trusted plugin is `octacity-source-git`; source plugins
  are distinct from Octa task plugins and are never loaded from a repository.
- An agent accepts only a signed, versioned `JobSpec`, not arbitrary remote
  commands.
- Installed Octa and plugin bundles are provisioned by the operator. The first
  agent is not a package manager or self-updater.
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
- Stable interfaces are introduced only at real architectural boundaries:
  coordinator transport, execution backend, source-plugin protocol, and
  server-side artifact storage. SDK wrappers and internal modules remain
  concrete until another behavior is required.

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
    |- selects Native or Microsandbox execution without fallback
    |- supervises the octa-runner protocol
    |- spools and forwards events
    `- validates and uploads artifacts/reports
            |
            v
      ExecutionBackend
          |- NativeBackend
          |   |- materialized workspace and per-job identity
          |   `- verified octa-runner and plugins on the agent host
          `- MicrosandboxBackend
              `- microVM
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
runner. Untrusted jobs must require an isolated backend.

## Repository shape

Start with one Cargo workspace and four crates:

```text
octacity/
|- Cargo.toml
|- crates/
|  |- octacity-agent/       # agent binary and concrete implementation
|  |- octacity-protocol/    # versioned server-agent DTOs and signatures
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
types, schema validation, and plugin-side helpers. It does not reuse the Octa
task-plugin protocol or import Octa internals.

Keep the rest in `octacity-agent` until a second real implementation requires
another crate. Internal modules are sufficient:

```text
config
coordinator
identity
inventory
lease
job
source
execution
native
microsandbox
runner
spool
artifacts
shutdown
```

The job owner depends on a narrow `CoordinatorClient` and `ExecutionBackend`.
The HTTPS coordinator and both execution backends implement these contracts.
VCS extensibility is provided by the process-level source-plugin protocol, not
one Rust trait implementation per VCS. The agent's source module remains one
concrete client for that protocol.

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
- installed source-plugin names, versions, protocols, schemas, platforms, and
  digests.

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
  provider parameters validated by its published schema

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
  backend: native or microsandbox
  OCI image by immutable digest when microsandbox is selected
  CPU and memory limits
  writable disk limit
  wall-clock timeout
  network policy
  workload identity profile

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

The server validates provider parameters against the source plugin's published
schema before signing the job. The agent repeats validation, verifies the
operator-installed manifest and binary digest, and invokes only the fixed
entrypoint resolved from that manifest.

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
enabled_execution_backends
allow_native_execution
native_cgroup_root
allowed_upload_origins
max_workspace_bytes
max_spool_bytes
poll_timeout
heartbeat_interval
lease_safety_margin
graceful_cancel_timeout
cleanup_timeout
```

At startup the agent canonicalizes roots, rejects overlapping unsafe paths,
checks permissions, verifies that credentials are not world-readable, checks
free disk space, validates the configured execution backends, Octa release, and
source-plugin registry, and fails before registration if a security
requirement is not met. Enabling `NativeBackend` requires the explicit
`allow_native_execution = true` setting; its presence is never inferred from a
missing or unavailable sandbox.

The process environment is not a hidden configuration layer. A minimal set of
deployment-specific overrides may be added only when an operational need is
demonstrated.

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
orphaned backend state and workspaces, retains enough diagnostics to report the
abandonment, and lets the server's lease TTL and fencing create a new attempt.
There is no duplicate execution recovery path in v1.

## Source acquisition plugins

Primary workspace acquisition happens before an Octafile can be loaded, so it
uses an OctaCity source-plugin protocol rather than the Octa task-plugin
protocol. A source plugin is an operator-installed executable with a versioned
manifest, protocol range, platform list, parameter schema, fixed entrypoint,
and SHA-256 digest.

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
executable SHA-256, settings schema, and plugin-specific settings. The agent
does not keep a second allowlist: installing a valid plugin below this
operator-controlled directory authorizes it. Removing its directory disables
it. Duplicate names, malformed manifests, path escapes, unsupported protocols,
digest mismatches, and unsafe directory permissions make startup validation
fail rather than silently hiding a broken plugin. Job content can select a
logical name but cannot register, configure, replace, or locate a plugin.

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
runner request for host paths and microVM paths without knowing which backend
it received. It also provides resource sampling plus bounded `wait`, `kill`,
and `destroy` operations while keeping its concrete native
or Microsandbox handle private. Orchestration must not inspect
`tokio::process::Child`, Microsandbox SDK handles, cgroups, or backend-specific
path construction.
The concrete Rust signatures may use pinned boxed I/O types, but the lifecycle
above is the complete behavioral boundary.

Version 1 includes:

- `NativeBackend`, which starts only the configured and digest-verified
  `octa-runner` and plugin bundle directly on a trusted agent host;
- `MicrosandboxBackend`, which starts the same release inside a microVM created
  through the pinned official Microsandbox Rust SDK.

The signed JobSpec selects one backend. The agent rejects a disabled,
unavailable, or incompatible backend. It never falls back from Microsandbox to
Native. Server scheduling must reserve Native for trusted projects and agents;
untrusted jobs require Microsandbox regardless of labels supplied by the
repository.

Every backend advertises which limits and isolation properties it can enforce.
It must either enforce each signed runtime requirement or reject the job; a
Native backend must never silently ignore a requested CPU, memory, filesystem,
identity, or network restriction.

Both implementations must expose the same runner lifecycle: bidirectional
non-PTY JSONL, bounded stderr, graceful cancellation, forced termination,
terminal status, resource sampling, and verified cleanup. A shared backend
contract suite runs against both. Native cleanup proves that no runner or
plugin process remains; Microsandbox cleanup additionally proves that no VM or
persisted sandbox state remains.

The microVM layout is fixed:

```text
/workspace             materialized source tree, read-write
/opt/octa               runner and plugins, read-only
/var/lib/octa           per-job execution state, read-write
/run/octa-identity      short-lived workload identity, read-only
```

If the high-level Microsandbox SDK does not expose the required bidirectional
process channel, use its official low-level agent client. Failure to provide
the contract makes Microsandbox unavailable; it must never select Native as a
fallback. Backend configuration defines CPU, memory, disk, wall-clock timeout,
image digest, and DNS/network policy where the backend can enforce them.

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

`NativeBackend` accounts for the entire job cgroup rather than the runner PID,
so plugin processes and all descendants are included. `MicrosandboxBackend`
accounts for the whole microVM through its runtime boundary. Disk usage covers
all writable job-owned storage, including workspace and Octa state, and should
come from enforced quota or backend accounting rather than repeated recursive
directory walks.

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
  (`/run/octa-identity` inside Microsandbox and a restricted per-job path for
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
- Define source-plugin v1 manifests, messages, schemas, cancellation, and
  terminal results.
- Define the server-side `ArtifactStore` boundary and S3 object model.
- Record the security and trust model in an ADR.
- Pin Rust MSRV and dependency policy.

Completion gate: invalid signatures, unknown protocol versions, oversized
payloads, stale leases, and unknown fields are rejected by contract tests.

### Phase 1: agent process and inventory

- Create the Cargo workspace and the four initial crates.
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

### Phase 2: coordinator transport and lease loop

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

### Phase 3: source-plugin host and Git provider

- Implement source-plugin registry discovery, manifest and settings validation,
  permission checks, digest verification, bounded JSONL transport,
  cancellation, and terminal results.
- Implement `octacity-source-git` with the fixed safe checkout sequence.
- Isolate Git configuration and credentials.
- Enforce exact revisions, path rules, timeouts, output limits, and quotas.
- Create the per-job journal and source cleanup procedure.

Completion gate: the Git plugin materializes an exact fixture revision through
the published source protocol; malformed or untrusted plugins are rejected;
malicious fixture repositories cannot execute host hooks, filters, submodules,
or helpers; and credentials are absent from arguments, protocol output, and
logs.

### Phase 4: execution backends and runner vertical slice

- Define one retained backend contract suite before implementing either
  backend.
- Implement `NativeBackend` with explicit enablement and no ambient environment
  or credential inheritance.
- Implement `MicrosandboxBackend` through the pinned official Rust SDK.
- Prove the same bidirectional non-PTY runner protocol transport through both.
- For Microsandbox, mount workspace, Octa release, state, plugins, and identity
  with the required permissions and apply image, resource, timeout, and network
  policies.
- Execute the same real Octafile through both backends.
- Collect cumulative CPU, memory, disk, and I/O usage for the complete job
  boundary in both backends; report network counters only where accurate.
- Implement graceful cancellation, forced termination, orphan cleanup, and
  complete destroy for both.

Completion gate: the same leased fixture succeeds through Native and
Microsandbox using the shared contract. Backend selection is signed and
explicit, an unavailable Microsandbox never falls back to Native, and neither
mode leaves a runner, plugin process, VM, or job filesystem state behind. CPU,
memory, disk, and I/O accounting includes child plugin processes and terminal
totals agree with the backend's authoritative counters.

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

- Add systemd service and hardened unit settings.
- Build signed Linux `x86_64` and `aarch64` release artifacts with checksums.
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

The KVM end-to-end suite runs on dedicated Linux CI runners. Unit and protocol
tests run on normal Linux CI. Windows and macOS may compile shared protocol
code but are not advertised as supported agents in v1.

## Performance tests

Fix thresholds before the release run and preserve raw samples as CI artifacts,
not committed result files. Measure at least:

- idle agent CPU and RSS;
- long-poll request rate while idle;
- lease-to-source-start latency;
- source materialization latency;
- Native and Microsandbox start/destroy latency;
- agent overhead around the same `octa-runner` fixture on both backends;
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
  Native and Microsandbox backends without fallback;
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

- server queue and scheduler implementation beyond the protocol test server;
- Web UI;
- multiple simultaneous jobs per agent;
- agent pools and autoscaling;
- Windows and macOS execution backends;
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

These features require real product demand. They must not complicate the first
Linux agent or weaken the explicit trust boundary between Native and isolated
execution.

## First coding milestone

Do not begin with registration screens or a broad framework. The first code
milestone is a retained end-to-end test and its production implementation:

```text
signed fixture lease
  -> exact fixture revision through octacity-source-git
  -> selected ExecutionBackend
      -> NativeBackend
      `-> MicrosandboxBackend by image digest
  -> identical octa-runner hello/start/events/finished contract
  -> graceful backend destroy
  -> verified empty process, VM, credential, and workspace state
```

The milestone is complete only when both backends pass the retained contract
suite and Microsandbox failure never selects Native. Then add the real
long-poll lease loop, durable delivery, S3-compatible uploads, and operational
packaging around the same path.
