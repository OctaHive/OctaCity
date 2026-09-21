# Server-agent transport protocol v1

Status: implemented by `octacity-protocol`, `octacity-coordinator`, and
`octacity-lifecycle` for registration, lease acquisition, heartbeat, durable
events, cache-session authority, artifact/report upload authorization, and
terminal completion.

## Boundary

The agent opens outbound HTTP connections only. Production endpoints require
HTTPS with normal certificate validation. Agent configuration and the client
constructor accept loopback HTTP solely for local integration tests.
Authentication uses a Bearer enrollment credential read from a permissions-restricted file; it is
never accepted as a command-line value or written to logs.

All bodies are strict JSON objects. Unknown fields, unsupported
`protocol_version` values, oversized request or response bodies, and a response whose
`request_id` does not echo the request are rejected before they affect lease
state. Golden v1 documents live in
[`shared/protocol-fixtures/coordinator`](../../shared/protocol-fixtures/coordinator).

## Idempotency and retries

Every request contains a unique `request_id`; the same value is sent in the
`Idempotency-Key` header. An automatic retry serializes the exact same body and
uses the same identifier. Servers must retain an idempotency record long enough
to return the same logical result when a response was lost.

The v1 client retries only idempotent operations:

- agent registration;
- lease acquisition;
- fenced lease heartbeat;
- fenced event append;
- fenced cache-session begin and revocation;
- fenced artifact/report upload begin and completion;
- fenced terminal completion.

Retries are bounded by an operator-configured attempt count and exponential
delay with jitter. `RegisterAgentResponse.max_retry_delay_ms` supplies an
additional server ceiling for later calls. A server may require a longer delay
with `Retry-After` or `retry_after_ms`; the effective delay still cannot exceed
the local and registration ceilings. Authentication, validation, and other
non-retryable responses fail immediately.

## Registration

```text
POST /api/v1/agents/register
```

`RegisterAgentRequest` carries the complete startup-validated inventory:

- agent release, identity, labels, host platform, and static capacity;
- exact Native or OCI platform/isolation routes;
- installed Octa runner digest and supported schemas/protocols;
- digest-verified task and source plugins.

Source-plugin operator settings and credentials are deliberately absent. The
server returns an opaque `registration_id` representing this process epoch and
its retry-delay ceiling. Registration inventory is advisory scheduling data;
the agent still enforces every signed job requirement locally.

## Lease acquisition

```text
POST /api/v1/agents/{agent_id}/leases:acquire
```

The request contains the current `registration_id`, a 1–60 second whole-second
long-poll wait, and `accept_jobs`. The agent sets `accept_jobs` to false while
local disk admission is paused. The coordinator must then return only
`no_work` or `drain`; this keeps registration liveness and operator drain
available without assigning a job the agent cannot materialize. A successful
response has exactly one outcome:

- `lease`: a fenced assignment and signed JobSpec;
- `no_work`: no match before the poll ended, plus a bounded repoll delay;
- `drain`: stop acquiring work for this registration epoch.

Placement reads one global durable queue in priority, enqueue-order, and stable
Job-identity order. It verifies Pool membership and accepting state, Pool and
single-Agent concurrency, exact labels, platform, runtime/isolation route,
registered capacity, Octa protocols and digest, task/source plugin identities,
and cache capability. Incompatible entries are skipped without changing their
priority. The selected queue row is locked with `FOR UPDATE SKIP LOCKED`; the
Lease commit wins before an assignment is returned. Waiting happens outside a
transaction and is followed by fresh registration authorization and another
authoritative claim, so notifications are only wake-up hints.

That commit removes the queue entry, marks the Job leased, and records the
selected Pool version, registration epoch, issue time, expiry, and a one-way
SHA-256 verifier for the opaque fence in the same transaction as idempotency,
audit, and outbox evidence. The raw fence is returned only in the committed
assignment and is never written to audit or outbox payloads. A failed commit
therefore leaves the Job ready and produces no assignment response.

A lease contains `lease_id`, `job_id`, positive `attempt`, opaque
`fencing_token`, `issued_at`, `expires_at`, and `signed_job_spec`. Before a
lease reaches job orchestration the agent:

1. checks all fencing identities and the validity interval;
2. requires usable lifetime beyond its configured safety margin;
3. verifies the JobSpec Ed25519 signature;
4. binds the signed `job_id` and `attempt` to the lease;
5. validates the authenticated JobSpec fields and timestamps.

An invalid, expired, or incorrectly bound assignment is never executed.

## Heartbeat and fencing

```text
POST /api/v1/leases/{lease_id}/heartbeat
```

The heartbeat always includes the current registration, complete
`LeaseFence`, and a validated advisory host snapshot. Heartbeats run in a task
independent of event delivery so event backpressure cannot silently consume the
lease.

Every successful heartbeat returns exactly one directive:

- `continue { expires_at }`: renew and continue;
- `cancel`: cancel the active job;
- `fenced`: cancel immediately because this attempt no longer owns the fence;
- `drain { expires_at }`: renew the active lease but make draining sticky so no
  subsequent job is acquired.

Wall-clock expiry is translated into a monotonic local safety deadline. A
transient heartbeat failure is not proof of lease loss, but inability to renew
before that deadline cancels the complete source/runner/backend operation.
Stale agents therefore cannot continue indefinitely or later publish success
for a newer attempt.

## Host snapshots

Registration reports logical CPU count, total memory, work/state filesystem
capacity, and hypervisor availability. Heartbeats report estimated available
CPU, available memory, filesystem free space, active job identity, and backend
health. Snapshot values are bounded by registered capacity. They are advisory
and never replace cgroup, quota, container, or VM enforcement.

While a job is active, `active_job.resource_usage` carries the latest
cumulative sample for live scheduling and diagnostics. The same sample is an
ordered durable agent event; heartbeat is not its storage channel.

## Durable attempt events

```text
POST /api/v1/leases/{lease_id}/events:append
```

Every envelope repeats `job_id`, `attempt`, `lease_id`, and `fencing_token`,
then adds a positive `stream_sequence` and an immutable
`occurred_at_unix_ms`. A batch is non-empty, bounded, and
strictly contiguous. Runner events retain their original event-schema version,
timestamp, category, data, and runner sequence inside the envelope. Agent
lifecycle and resource events use a separate tagged type and cannot be
mistaken for Octa events.

The agent syncs each immutable spool record before it is eligible to send. The
server inserts idempotently by `(job_id, attempt, stream_sequence)` and returns
the largest contiguous sequence accepted from the submitted batch. A lost
response therefore causes a harmless duplicate replay, not a gap. The agent
syncs this acknowledgement before reclaiming record files. Byte and record
limits apply to the unacknowledged prefix; once full, runner consumption pauses
through its bounded channel while heartbeat remains independent.

## Terminal completion

```text
POST /api/v1/leases/{lease_id}/complete
```

Completion repeats the lease fence and registration epoch and carries a stable
`completion_id`, the last acknowledged event sequence, terminal status,
runner results, and the final cumulative resource snapshot when available.
The agent persists this exact document before sending it. The server must
validate current fencing and treat repeated `completion_id` values
idempotently.

The operation starts only after every event is acknowledged and local cleanup
has succeeded. On restart the agent does not resume execution: it first asks
all configured backends to destroy orphans, then removes only state directories
whose valid journal proves agent ownership. Unknown files are left untouched.

## Cache-session authority

```text
POST /api/v1/leases/{lease_id}/cache:begin
POST /api/v1/leases/{lease_id}/cache:revoke
```

Both requests carry the current registration epoch and complete lease fence.
The begin operation exchanges the signed logical cache policy for an opaque
scope and optional short-lived HTTP L2 credential; revocation invalidates that
session after the runner stops. The credential is never included in durable
agent state. Detailed narrowing, path mapping, and failure semantics are in
the [cache-session v1 specification](cache-session-v1.md).

## Artifact and report uploads

```text
POST /api/v1/leases/{lease_id}/artifacts:begin
PUT  <short-lived presigned object URL>
POST /api/v1/leases/{lease_id}/artifacts:complete
```

After the runner and its complete process boundary have stopped, the agent
copies each declared output into private immutable staging. It independently
normalizes the workspace-relative path, rejects escapes and unsafe filesystem
types, applies the signed count and aggregate-byte limits while writing, and
computes SHA-256 over the exact bytes that will be sent. Directory artifacts
use the versioned deterministic format described in
[`directory-artifact-format-v1.md`](../directory-artifact-format-v1.md).

The fenced begin request carries a stable `upload_key`, logical artifact or
report metadata with its runner `run_id` and `task_id`, exact size, transport
media type, and digest. Its response
contains an opaque `upload_id`, an expiring presigned PUT URL, and bounded
required headers. The agent accepts only an operator-allowlisted origin,
disables redirects, rejects credential-bearing or transport-owned headers, and
retries by reopening the same staged file. It never receives an object-store
access key or secret key.

The fenced complete call names only the opaque upload record. The server must
verify the object size and digest recorded by begin before making it visible.
Terminal lease completion cannot start until every output has been confirmed
and local staging and workspace cleanup have succeeded.

## Error response

Non-success HTTP responses use `CoordinatorErrorResponse` with an echoed
`request_id`, stable `code`, bounded safe `message`, explicit `retryable` flag,
and optional `retry_after_ms`. Both the HTTP status and the flag must permit a
retry. Error bodies obey the same body-size and operation-deadline bounds as
successful bodies.

Every fenced endpoint uses `lease_fenced` when a newer fencing token owns the
attempt and `lease_expired` when the lease has expired. These two codes are
permanent and authoritative; the agent preserves them as lease-loss outcomes
even when the rejection arrives before the next heartbeat.
