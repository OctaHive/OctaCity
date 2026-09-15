# Cache session protocol v1

Status: implemented by `octacity-protocol`, `octacity-coordinator`,
`octacity-cache-session`, and `octacity-runner`.

## Ownership boundary

Octa is the task-result cache engine. It computes action identities, snapshots
inputs, verifies local and remote content, restores outputs transactionally,
and publishes successful results. OctaCity does not duplicate that logic and
does not proxy cache blobs through the agent or coordinator API.

OctaCity owns authorization and placement:

- signed `JobSpecV1.cache` selects a namespace and read/write permissions;
- the coordinator binds a short-lived session to the current lease fence;
- agent configuration limits read/write authority, per-scope L1 capacity and
  retained scope count, allowed L2 origins,
  Native runtime identities, request timeouts, and transfer concurrency;
- the execution backend maps agent-owned paths to fixed runner paths;
- runner protocol v3 carries the narrowed session to Octa.

The effective topology is always a persistent verified local L1 and, when
authorized, an HTTP L2. A network-disabled job keeps L1 and silently omits L2.
A restricted job may use L2 only when the endpoint host appears in its signed
network allowlist.

## Fenced coordinator operations

```text
POST /api/v1/leases/{lease_id}/cache:begin
POST /api/v1/leases/{lease_id}/cache:revoke
```

`cache:begin` repeats the registration epoch and complete `LeaseFence`, and
carries the signed `CachePolicy`. The response echoes `request_id`, returns an
opaque revocable `session_id`, an opaque `scope_id` for physical L1 isolation,
and optionally an HTTPS endpoint, bearer credential, and expiry.

`cache:revoke` repeats the same registration and fence plus `session_id`.
Both calls use stable operation-specific idempotency keys. The server must
reject stale fencing tokens and return the same logical response when the
exact request is retried.

The `scope_id` is not a repository path. The agent hashes it together with the
immutable runtime identity to choose a private directory below `cache.root/v1`.
Thus two jobs share physical L1 state only when the server authorizes the same
trust scope and their OS, architecture, and tool environment are identical.
Octa applies each scope's capacity and GC watermarks. Microsandbox also places
a hard quota on the persistent cache volume. Native and containerd require the
configured cache root to be a dedicated filesystem no larger than
`max_bytes × max_scopes`, which keeps arbitrary cache-mount writes away from
the agent state disk and gives process isolation a physical aggregate bound.
Reaching the scope-count limit fails
closed until an operator or the Phase 8 disk-pressure policy removes an old
scope; the agent never silently deletes a possibly active cache directory.

## Credential handling

The remote bearer never enters JobSpec, environment variables, command-line
arguments, runner events, the durable event spool, or terminal results. The
agent writes it to an owner-only file below the job root and supplies only the
file path through runner protocol v3. Native, containerd, and Microsandbox
mount the file read-only at `/run/octa-cache/token`; L1 is writable at
`/var/cache/octa`, and an optional CA is read-only at
`/run/octa-cache/ca.pem`. Because that CA controls the cache TLS trust root,
the agent accepts it only from an owner-protected file whose directory chain
does not allow replacement by an untrusted local principal.

After runner shutdown the agent deletes the bearer before output processing,
revokes the server session, and only then proceeds toward terminal completion.
If execution infrastructure fails unexpectedly, backend and workspace cleanup
precedes server revocation. A lost revocation response is safe to retry; the
short expiry remains the final bound if the agent disappears entirely.
The grant must remain valid for the complete remaining job timeout plus one
final remote request deadline, otherwise the agent rejects it before execution.

## Failure semantics

Remote transport failure is handled by Octa's layered cache policy: an
available L1 remains usable and cache unavailability cannot turn unverified
bytes into a hit. Digest mismatch, corrupt content, namespace mismatch, and an
expired credential are misses or explicit cache diagnostics according to the
Octa cache protocol. The agent forwards Octa's semantic events unchanged and
does not emit per-blob lifecycle events.
