# Server production operations

This runbook defines the supported ingress, credential, retention, alerting,
and incident-recovery contract for the first OctaCity server release. The
management API has no operator authentication: network isolation is an
authorization boundary, not an optional hardening step.

## Transport and listener topology

The server currently accepts HTTP on its four independently configured
listeners. It does not load TLS certificates or private keys, so direct TLS
termination inside `octacity-server` is not available in this release. Adding
unrecognized TLS fields to the strict TOML configuration fails validation.

Two deployment topologies are supported:

| Topology | When to use it | Required controls |
| --- | --- | --- |
| Trusted reverse proxy | Recommended production topology | Bind every server listener to loopback or a private proxy-only interface. Terminate TLS at a trusted proxy. Publish only the Agent, cache, and webhook listeners that their clients need. Keep management private. |
| Direct trusted network | A private, encrypted service network already supplies transport protection | Bind each listener to a separately firewalled port. Set `acknowledge_unauthenticated_management = true` for a non-loopback management address. Never expose management to an untrusted network. |

Plain HTTP across an untrusted link is unsupported. If policy requires TLS in
the application process rather than a proxy or encrypted service network, do
not deploy this release until native TLS is implemented and qualified.

The corresponding CI-validated configurations are
[`server.reverse-proxy.example.toml`](../server.reverse-proxy.example.toml) and
[`server.trusted-network.example.toml`](../server.trusted-network.example.toml).
Replace sample hosts, paths, bucket, region, and key identifiers before use,
then run:

```shell
octacity-server validate /etc/octacity/server.toml
```

Validation reads no listener and fails before startup when required sections
are missing, listener addresses overlap, or a non-loopback management bind is
not explicitly acknowledged.

### Listener separation

Apply firewall and proxy policy per listener rather than routing all paths to
one port:

| Listener | Callers | Authentication | Exposure |
| --- | --- | --- | --- |
| `management_bind` | operators, health and metrics collectors | none in v1 | trusted operator network only |
| `agent_bind` | outbound-only Agents | single-use enrollment, then current registration credential and Lease fencing | Agent network only |
| `cache_bind` | authorized Octa runners | short-lived namespace-scoped cache bearer | Agent/workload network only |
| `webhook_bind` | configured VCS providers | provider verification over the exact bounded body and allowlisted headers | public only when a webhook integration is enabled |

Do not publish `/metrics`, `/health/*`, OpenAPI, or management paths through the
public webhook virtual host. Health probes are intentionally unauthenticated
and are not a substitute for the management firewall.

### Reverse-proxy trust

The server keys ingress admission to the accepted socket peer and deliberately
ignores `Forwarded`, `X-Forwarded-For`, `X-Real-IP`, and similar address
headers. This prevents a client from choosing its rate-limit identity. A proxy
therefore needs its own client-aware rate limits; the server's process-local
limit is an additional proxy-level bound when every request has the same peer.

At the proxy boundary:

- discard client-supplied forwarded-address headers before adding any
  proxy-owned headers needed by other systems;
- preserve webhook request bytes and only the provider headers configured for
  verification;
- enforce bounded request sizes and timeouts at least as strict as the server's
  published limits, without retrying non-idempotent requests unless the caller
  supplied the same `Idempotency-Key`;
- keep the proxy-to-server hop on loopback, a protected host network, or an
  authenticated encrypted link; and
- route each public origin to exactly one intended OctaCity listener.

OctaCity does not derive callback or cache URLs from `Host` or forwarded
headers. `webhook_public_base_url` is a credential-free HTTPS origin with no
path, query, fragment, or trailing slash. Loopback HTTP is accepted only for
development. `cache.endpoint` is always a credential-free HTTPS URL. Startup
rejects invalid values, and generated webhook callbacks append only the fixed
`/webhooks/v1/integrations/{integration_id}` path.

TLS certificates and their rotation belong to the reverse proxy or service
network. Rotate them with the proxy's normal overlap procedure and verify the
Agent, cache, and webhook origins independently before retiring the old chain.

## Signing and Agent credential rotation

Credential files must be regular, non-symlink files owned by the server
identity and restricted to that identity on Unix. Private signing material,
the Agent enrollment derivation key, and cache credential key are independent
32-byte keys; never reuse one for another purpose.

### JobSpec signing key

Use an overlap rotation because a JobSpec is verified by the Agent, not by the
server that signed it:

1. Generate a new Ed25519 private key and a new non-secret `key_id` in protected
   storage.
2. Add the new public key to every Agent's `server_signing_keys` while retaining
   the old public key. Drain, restart, and confirm each Agent registers.
3. Update every server replica to the new `signing.key_id` and private-key file
   with a rolling restart, waiting for readiness after each replica.
4. Keep the old public key on Agents until every Ready or leased JobSpec signed
   by it is terminal or cancelled. Blocked Jobs are signed only when they
   become Ready and therefore use the then-active server key.
5. Remove the old public key in a second drained Agent restart. Retire the old
   private key according to the protected backup policy.

Do not use only a wall-clock delay for step 4: a Ready Job may remain queued
longer than one signing validity window.

### Enrollment and registration credentials

Enrollment credentials are single-use and expire after
`agent_enrollment_lifetime_milliseconds`. A successful registration consumes
the enrollment credential and establishes a time-bounded registration epoch;
a later successful registration for the same Agent supersedes the old epoch.

For routine rotation, issue a fresh enrollment credential with a new
`Idempotency-Key`, atomically replace the Agent's protected credential file,
drain the Agent, and restart it. Confirm the new registration before deleting
the old local credential. Never copy one enrollment credential to multiple
Agents.

If a registration credential may be compromised, force-drain the Agent,
terminate its service, allow its fenced Lease to settle, and re-enroll from a
known-clean host. Reassignment of an idle Agent revokes its current
registration. If an unused enrollment bearer leaks, remove Agent ingress from
the affected network until that single-use credential is consumed by the
intended Agent or reaches its server-controlled expiry.

The server-side enrollment derivation key makes exact lost-response replay
possible. Rotate it only in a coordinated maintenance window: stop enrollment
issuance, preserve existing registration access, wait for outstanding
enrollment credentials to expire, deploy the new key to every server replica,
and use new idempotency identities for subsequent issuance.

## Automatic retention and Build Result holds

At Build acceptance, OctaCity converts the immutable effective Project policy
into separate absolute deadlines for metadata, logs, artifacts, and reports.
Changing Project policy later does not rewrite those deadlines. The retention
worker hides logical data before deleting search documents, manifests, and
object bytes, and safely resumes interrupted work.

Inspect the authoritative deadlines and hold state:

```shell
curl --fail-with-body \
  https://management.internal.example.test/api/v1/builds/BUILD_ID/retention
```

Place a permanent hold by omitting `expires_at_unix_ms`, or a time-bounded hold
by supplying an absolute Unix-millisecond expiry:

```shell
curl --fail-with-body -X POST \
  https://management.internal.example.test/api/v1/builds/BUILD_ID/retention/hold \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: example-hold-key' \
  --data '{"reason":"incident 2026-0042 evidence","expires_at_unix_ms":1798761600000}'
```

Release the active hold using the returned hold version as an `If-Match`
precondition:

```shell
curl --fail-with-body -X POST \
  https://management.internal.example.test/api/v1/builds/BUILD_ID/retention/hold/release \
  -H 'Idempotency-Key: incident-2026-0042-release' \
  -H 'If-Match: "HOLD_VERSION"'
```

A hold covers the complete Build Result. It does not move or recompute the
original deadlines. Releasing or expiring the last hold makes already-overdue
components eligible for the next retention pass. Once the first visibility
transition has begun, placing a hold returns a stable conflict and cannot
resurrect partially deleted data.

Held data continues to count toward Project quotas and physical PostgreSQL,
object-store, search-index, and backup capacity. Capacity forecasts must add
the bytes of all active permanent and time-bounded holds to normal retention
windows. Do not treat a future hold expiry as immediately free space: allow for
the retention poll interval, bounded batches, retries, object-version cleanup,
and backup expiration. Reject or release nonessential holds before storage
exhaustion; never bypass quota accounting or delete provider objects directly.

## Alerting

Scrape `/metrics` only through the trusted management network and alert on:

- a non-200 `/health/ready` response for longer than the configured readiness
  refresh and dependency recovery budget, while `/health/live` distinguishes a
  dependency outage from a dead process;
- increases in `octacity_server_worker_runs_total` with `outcome="failure"`, especially
  `worker=retention`, `log_index`, `lease_expiry`, `schedule`, or
  `internal_trigger`;
- sustained `octacity_server_worker_retries_total` growth and failed store, object,
  cache, webhook, or VCS adapter operations;
- HTTP failure and rejection rates, ready-Job growth, Lease failures, and log
  search responses whose committed watermark stays ahead of indexed-through;
- PostgreSQL, object-store, search, and backup capacity, including noncurrent
  object versions and held Build Results; and
- certificate expiry, failed backup reconciliation rehearsals, and failure to
  complete the signing-key overlap inventory.

Page on readiness loss, repeated retention failure near a capacity threshold,
missing referenced objects, signing-key compromise, or unintended management
reachability. Ticket bounded transient retries only while capacity and retry
deadlines remain safe. Logs and alerts may contain request IDs, stable
component names, and error classes, but never credentials, presigned URLs,
private paths, webhook bodies, or repository content.

## Incident recovery

1. Remove affected ingress at the firewall or proxy before changing durable
   state. If management was exposed, treat all accepted management mutations
   during the interval as potentially hostile because v1 has no actor login.
2. Preserve safe logs, request IDs, audit facts, outbox state, Build IDs,
   configuration version, release digest, and timestamps. Do not copy secrets
   into the incident record.
3. Drain or fence affected Agents. Rotate signing and Agent credentials using
   the procedures above; cancel and retry Builds whose signed intent cannot be
   trusted.
4. For database or object-store loss, keep every listener offline and follow
   [`backup-restore.md`](backup-restore.md). PostgreSQL and the
   configured object prefix are one recovery unit.
5. Run `octacity-server reconcile-restore` before startup, start one replica,
   wait for readiness, verify retention and search freshness, then restore
   ingress and additional replicas.
6. Confirm the cause is removed, rotate proxy certificates or provider
   credentials when implicated, and retain non-secret recovery evidence.

Never repair a retention incident by editing PostgreSQL rows or deleting
bucket keys manually. Those actions bypass logical visibility, tombstones,
digest verification, quota accounting, audit, and idempotent recovery.
