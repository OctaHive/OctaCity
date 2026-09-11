# Server-agent transport protocol v1

Status: implemented by `octacity-protocol` and `octacity-coordinator` for
registration, lease acquisition, and active-lease heartbeat. Durable events,
completion, and upload operations are added in the following phases without
changing the fencing rules defined here.

## Boundary

The agent opens outbound HTTP connections only. Production endpoints require
HTTPS with normal certificate validation. Loopback HTTP is accepted solely by
the client constructor for local integration tests. Authentication uses a
Bearer enrollment credential read from a permissions-restricted file; it is
never accepted as a command-line value or written to logs.

All bodies are strict JSON objects. Unknown fields, unsupported
`protocol_version` values, oversized request or response bodies, and a response whose
`request_id` does not echo the request are rejected before they affect lease
state. Golden v1 documents live in [`protocol/coordinator`](../../protocol/coordinator).

## Idempotency and retries

Every request contains a unique `request_id`; the same value is sent in the
`Idempotency-Key` header. An automatic retry serializes the exact same body and
uses the same identifier. Servers must retain an idempotency record long enough
to return the same logical result when a response was lost.

The v1 client retries only these idempotent phase-4 operations:

- agent registration;
- lease acquisition;
- fenced lease heartbeat.

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

The request contains the current `registration_id` and the maximum whole-second
long-poll wait. A successful response has exactly one outcome:

- `lease`: a fenced assignment and signed JobSpec;
- `no_work`: no match before the poll ended, plus a bounded repoll delay;
- `drain`: stop acquiring work for this registration epoch.

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

## Error response

Non-success HTTP responses use `CoordinatorErrorResponse` with an echoed
`request_id`, stable `code`, bounded safe `message`, explicit `retryable` flag,
and optional `retry_after_ms`. Both the HTTP status and the flag must permit a
retry. Error bodies obey the same body-size and operation-deadline bounds as
successful bodies.
