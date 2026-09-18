# Agent Operations and Lifecycle

This part of the implementation plan covers resource accounting, supervision, delivery, security, outputs, local state, shutdown, and observability.

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

- a short-lived workload identity file copied into a private per-job host
  directory and exposed to the workload at the fixed runtime path
  `/run/octa-identity` in Native and OCI execution;
- a secrets profile containing provider addresses, roles, mounts, and identity
  paths but no resolved values;
- an outbound network policy allowing only the required identity and Vault
  endpoints.

The identity source is configured on the agent host. Startup validation checks
its owner, private access policy, and the ownership chain of its protected
parent before the provider creates a private copy for one job. Native execution
does not inherit the agent's complete environment or credentials. Identity
material is removed during cleanup and its verbatim value is redacted at the
runner-supervisor boundary before event payloads, diagnostics, results, or
persistent job metadata can observe it. Binary output redaction retains only a
bounded suffix per command stream, so an identity split across adjacent output
frames is matched before either part can become reconstructable durable data.

Redaction protects against accidental verbatim disclosure; it is not a sandbox
for a malicious workload that deliberately transforms a credential before
printing it. A job granted workload identity is therefore trusted to use that
identity, while runtime network policy and Vault policy limit what it can reach
and what the identity can authorize.

Agent-owned job, identity, lifecycle, and output-staging directories use mode
`0700` on Unix. On Windows they are created with a protected inheritable DACL
limited to the object owner, LocalSystem, and built-in administrators; config
validation rejects credential and identity paths granting access elsewhere.

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

## Octa task-result cache integration

Octa owns the complete cache engine: filesystem contracts, input snapshots,
action identity, local CAS, transactional restore, output publication, and the
layered local-first policy. OctaCity must use the published runner protocol and
HTTP cache protocol rather than duplicating any of those semantics.

For a cache-enabled job, the server authorizes only a logical namespace and
independent read/write permissions. The agent narrows that grant through local
policy, obtains a short-lived fenced cache credential, writes it to a private
per-job file, and builds the runner's `CacheSessionSpec`. The session contains:

- the authorized cache mode and namespace;
- an absolute agent-owned L1 directory outside the repository workspace;
- the immutable OCI image identity or operator-configured Native environment
  identity;
- an optional HTTPS remote endpoint, private token-file path, optional CA
  certificate path, request deadline, and transfer limit.

The remote endpoint must match an operator-configured origin. The token value
never enters `JobSpec`, command arguments, environment variables, runner
events, spool records, or logs. It is mounted read-only into OCI execution and
removed after the runner stops. A restricted job network policy must explicitly
permit the cache endpoint.

The persistent L1 is shared only within the same server-authorized trust
domain, project, platform, and runtime identity. Its filesystem capacity is
operator-bounded independently of workspace limits. Octa verifies all cache
metadata and content digests, so a corrupt entry is a miss or quarantine event,
never trusted output. A repository process may at worst evict or corrupt cache
data in its own authorized scope; it cannot write a trusted namespace belonging
to another scope.

## Local state and cleanup

Use distinct roots:

```text
state_root/
  agent.json
  jobs/<lease-id>/journal
  jobs/<lease-id>/events

work_root/
  jobs/<lease-id>/workspace

cache_root/
  v1/<opaque-authorized-scope>/
```

The cache root is persistent agent state, not job workspace state, and normal
job cleanup never removes it. Pull requests from untrusted forks receive an
isolated read-only or untrusted-write namespace. Physical directory names are
derived from bounded validated scope identities rather than using namespace
text as an unchecked filesystem path.

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

[Back to the implementation-plan index](../agent-implementation-plan.md)
