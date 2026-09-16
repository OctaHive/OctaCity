# Agent provisioning protocol v1

Status: provider-neutral wire contract implemented by
`octacity-agent-provisioning-protocol`. The first release ships no production
provisioning adapter and does not advertise dynamic agent provisioning as
available.

## Purpose and boundary

This protocol reserves a future seam for provisioning machines without adding
vSphere, Proxmox, cloud, cluster, template, network, or datastore types to the
JobSpec, Agent, Pool, Orchestrator, or Placement Scheduler.

The provider adapter owns infrastructure credentials and provider-specific
configuration. A provisioned machine receives a single-use short-lived
enrollment bootstrap and then connects outbound through the normal agent
protocol. Dynamic agent provisioning does not create a second agent control
channel.

## Version and adapter negotiation

An `AdapterManifest` declares adapter identity, immutable executable SHA-256,
inclusive protocol range, and provision/observe/terminate capabilities.
Negotiation selects the newest common version. Invalid or disjoint ranges are
rejected before bootstrap or provider credential handles are supplied.

A conforming v1 lifecycle adapter implements all three lifecycle operations.
Possessing a conforming manifest does not enable the feature by itself; the
first server release intentionally installs no production adapter or real
infrastructure reconciliation loop.

## Encoding and envelopes

Messages are strict UTF-8 JSON. Every object rejects unknown fields. Requests
contain `protocol_version`, `request_id`, and a tagged `command`. Responses echo
the version and request identity and contain a tagged `outcome`. The complete
encoded request or response is limited to 64 KiB before JSON decoding. Parsing,
strict schema checks, and semantic validation form one decode operation.
Responses are accepted only when version and request identity match the
outstanding request.

The deterministic v1 request fixture is
[`provision-v1.json`](../../server/protocols/octacity-agent-provisioning-protocol/fixtures/provision-v1.json).

## Provider-neutral intent

`PoolIntent` contains only:

- server-owned `pool_id`;
- expected OS and CPU architecture;
- positive minimum logical CPU count;
- positive minimum memory bytes;
- positive minimum workspace disk bytes.

It deliberately has no provider template, image, cluster, network, datastore,
subscription, project, region, or placement-policy field. Such configuration
belongs to the selected adapter and is not project-readable protocol data.

`EnrollmentBootstrap` contains a host-owned
`enrollment_credential_handle` and absolute Unix-millisecond expiry. The handle
is resolved only inside the adapter host; the raw single-use credential is not
serialized into the provider-neutral message.

## Operations

### `provision`

Carries `operation_id`, stable `idempotency_key`, pool intent, and bootstrap.
Repeating an exact request after a lost response must return or converge on the
same provider machine instead of creating duplicate capacity.

### `observe`

Carries `operation_id` and opaque `machine_id`. It returns normalized lifecycle
state without provider-specific infrastructure data.

### `terminate`

Carries `operation_id`, `idempotency_key`, and opaque `machine_id`. Repeated
termination must converge on confirmed absence. It must not resurrect or
replace the target machine.

### `cancel`

Carries `target_operation_id`. Cancellation is cooperative and idempotent. A
provider may be unable to reverse already completed creation; subsequent
observe or terminate reconciles the normalized lifecycle.

## Normalized lifecycle

Machine results contain opaque machine identity, server pool identity, optional
observed platform, Unix-millisecond observation time, and one state:

- `pending`;
- `provisioning`;
- `running`;
- `terminating`;
- `terminated`;
- `failed`.

Successful outcomes are `machine`, `terminated`, or `acknowledged`. They never
contain bootstrap secrets, provider credentials, template names, networks,
clusters, or datastores.

## Failure classification and retry

Failures contain a stable code, bounded secret-free diagnostic, optional retry
delay, and one of `invalid_request`, `unsupported`, `permanent`, `transient`,
`cancelled`, or `protocol_fault`.

Only `transient` may include `retry_after_ms`, and the suggested delay cannot
exceed one hour. The future server host owns stricter attempt and elapsed-time
ceilings. Provision and terminate retries must preserve their idempotency keys.

## V1 limits and first-release behavior

- opaque identifiers: 256 UTF-8 bytes;
- complete encoded request or response: 64 KiB;
- failure diagnostic: 4096 UTF-8 bytes;
- adapter retry suggestion: at most one hour;
- CPU, memory, and disk requests: positive values;
- bootstrap expiry and observation time: non-zero Unix milliseconds.

Static-agent readiness and scheduling do not depend on this protocol or an
adapter. With no production adapter installed, management reports dynamic
agent provisioning unavailable while static pools continue to operate
normally.
