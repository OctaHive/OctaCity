# Webhook provider protocol v1

Status: wire contract, conformance fixtures, bounded process hosting, public
webhook ingress, and unmanaged and managed application/REST lifecycles are
implemented. Production provider adapters are intentionally not included.

## Purpose and boundary

This protocol connects the OctaCity webhook adapter host to one
operator-installed provider adapter. It is not the public webhook HTTP API.
The HTTP ingress preserves a bounded raw request, and the selected provider
adapter authenticates those exact bytes before it may return a normalized
repository event.

```text
provider HTTP delivery
  -> bounded webhook ingress
  -> durable raw receipt + server delivery identity
  -> replica-safe delivery worker claim
  -> selected adapter receives exact body + allowlisted headers
  -> adapter verifies provider authentication
  -> durable AuthenticatedRepositoryEvent deduplicated by integration_id + delivery_id
  -> trigger engine evaluates the canonical normalized event idempotently
```

The public callback returns `202 Accepted` with a server-owned `delivery_id`
after the raw receipt is committed; it does not wait for adapter execution or
Build creation. Only transient adapter failures are retried, using the
server-owned bounded exponential policy. Permanent failures and exhausted
retries retain a secret-free dead-letter code, diagnostic, and attempt count.
Raw body and header values are removed after normalization or dead-lettering.
Receipts belonging to a disabled integration become terminal `suppressed`
records before adapter execution; their raw body and headers are cleared too.

Provider request bodies, signature formats, administration credentials, and
SDK types remain inside the adapter. The normalized event contains no GitHub,
Gerrit, GitLab, or other provider-specific type.

## Version and adapter negotiation

Before sending credentials or delivery data, the host reads an
`AdapterManifest` containing:

- `adapter_id`;
- the lowercase SHA-256 of the immutable executable;
- an inclusive protocol range `{min, max}`;
- explicit capabilities for verification and managed registration operations.

Version zero and inverted ranges are invalid. The host and adapter select the
newest version in the intersection of their ranges. No intersection is a hard
compatibility failure and the host must not send secret handles, repository
data, headers, or webhook bytes.

V1 adapters must advertise `verify_delivery`. Managed `create`, `observe`,
`rotate`, and `delete` are independent optional capabilities.

## Installed adapter registry

The host discovers adapters only from an operator-owned registry. Each adapter
occupies one real directory whose name equals `adapter_id` and contains a
strict `adapter.toml` plus a normalized relative executable path. The manifest
declares version `1`, the executable SHA-256, protocol range, and every
capability. Unknown fields, symbolic links, path traversal, incompatible
versions, oversized manifests or executables, unsafe Unix write permissions,
and digest mismatches reject the registry entry.

Callers resolve both `adapter_id` and the configured executable digest. The
host hashes the canonical regular executable during discovery, immediately
before spawn, and again after spawn but before writing any request. A replaced
adapter therefore receives no webhook body, header, or credential handle. The
v1 host caps executable size at 256 MiB so verification work is bounded.

An installation manifest has this shape:

```toml
manifest_version = 1
adapter_id = "example"
executable = "adapter"
executable_sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[protocol]
min = 1
max = 1

[capabilities]
verify_delivery = true
create_registration = false
observe_registration = false
rotate_registration = false
delete_registration = false
```

## Process lifecycle

Each operation runs in a fresh process with an empty inherited environment,
the adapter directory as its working directory, and only bounded stdin,
stdout, and stderr pipes. The host checks the advertised capability before
spawn, sends one JSON line, accepts one correlated response line, rejects
extra stdout, caps retained stderr at 64 KiB, and never includes stderr in a
returned diagnostic because an adapter might write protected material there.

The operation timeout covers spawn verification, request delivery, response,
and clean process exit. Caller cancellation or timeout sends the protocol
`cancel` command when the original operation has an identity, permits one
bounded grace period, and then terminates and reaps the complete adapter
process group. A terminal response cannot override a cancellation or timeout
that has already begun.

Host failures use stable invalid-request, unsupported, permanent, transient,
cancelled, and protocol-fault classes. Valid provider `failure` outcomes keep
their protocol class and optional bounded retry advice unchanged; the durable
retry policy remains server-owned. Claims have an owner and expiry, so another
replica can resume verification or Trigger evaluation after a process crash.
The normalized provider identity is committed before Trigger evaluation, and
the Trigger Engine uses an integration-scoped stable identity, preventing a
restarted worker or repeated provider delivery from creating another
occurrence or Build.

## Encoding and envelopes

Messages are strict UTF-8 JSON. Every object rejects unknown fields. Enum tags
and operation names use `snake_case`.

The complete encoded request or response is limited to 2 MiB and must be
rejected before JSON decoding. Decoding is one operation: enforce the total
limit, parse the strict envelope, and validate all semantic bounds. Callers
must not treat a successfully parsed but unvalidated envelope as a message.

A request has this envelope:

```json
{
  "protocol_version": 1,
  "request_id": "request-01",
  "command": {
    "operation": "verify_delivery",
    "payload": {}
  }
}
```

A response echoes `protocol_version` and `request_id`, then returns one tagged
`outcome`. The host must compare both fields with the outstanding request; a
response cannot be applied to another request merely because its operation
type matches.

The golden v1 delivery request is
[`verify-delivery-v1.json`](../../server/protocols/octacity-webhook-provider-protocol/fixtures/verify-delivery-v1.json).
The same fixture directory contains strict create, observe, rotate, and delete
registration requests used by adapters as the managed-registration conformance
set.

## Operations

### `verify_delivery`

The payload contains:

| Field | Meaning |
| --- | --- |
| `operation_id` | Stable identity of this in-flight operation |
| `integration_id` | Server-owned webhook integration identity |
| `verification_material_handle` | Host-owned handle, not raw verification material |
| `headers` | Allowlisted transport headers with normalized names |
| `body_base64` | RFC 4648 base64 of the exact raw request bytes |

The decoded body is limited to 1 MiB. The adapter must authenticate the exact
decoded bytes before returning `authenticated_event`. Failed authentication is
an error and must not produce a normalized event.

An `AuthenticatedRepositoryEvent` contains the integration-scoped
`delivery_id`, normalized `event_kind`, repository identity, optional reference
and immutable revision, optional provider time and display-only actor data, and
a bounded opaque metadata map. Metadata is data only; it cannot become an
authorization decision or a deduplication identity.

### Managed registration

The optional operations are:

- `create_registration`;
- `observe_registration`;
- `rotate_registration`;
- `delete_registration`.

Their common payload contains `operation_id`, `integration_id`, an
`idempotency_key`, the provider-neutral `callback_url`, and a host-owned
`credential_handle`. Raw administration credentials never appear in JSON.
Create, rotate, and delete must converge under exact replay with the same
idempotency key.

The normalized registration result contains only `integration_id`, an opaque
remote `registration_id`, `callback_url`, and one of `active`, `disabled`, or
`missing`.

### `cancel`

Cancellation carries `target_operation_id`. It is cooperative and idempotent:
acknowledgement means that the target is cancelled or already terminal. A
cancelled target may instead return the classified `cancelled` failure.

## Outcomes and failures

Successful outcomes are:

- `authenticated_event`;
- `registration`;
- `acknowledged` with the stable operation identity.

Failures contain a stable `code`, a bounded secret-free `diagnostic`, and one
class:

| Class | Retry meaning |
| --- | --- |
| `invalid_request` | Change the request; do not retry unchanged |
| `unsupported` | Selected adapter cannot perform the operation |
| `permanent` | Repository, integration, or configuration cannot succeed unchanged |
| `transient` | The exact idempotent operation may be retried within server limits |
| `cancelled` | Cooperative cancellation completed |
| `protocol_fault` | Peer violated framing, version, or schema rules |

Only `transient` may include `retry_after_ms`. The server owns the attempt and
elapsed-time ceilings; an adapter cannot request unbounded retry.

## V1 limits and security rules

- identifiers: 256 UTF-8 bytes;
- complete encoded request or response: 2 MiB;
- decoded delivery: 1 MiB;
- allowlisted headers: 64;
- normalized metadata entries: 32;
- header or metadata value: 4096 UTF-8 bytes;
- failure diagnostic: 4096 UTF-8 bytes.

Control characters are rejected in bounded textual fields, and supplied
header names must be lowercase ASCII tokens. Diagnostics must
not contain raw bodies, signature material, credential values, or provider
administration secrets.

The Rust contract and its golden fixture are tested for round-trip stability,
unknown nested fields, incompatible versions, cancellation, limits, and retry
classification.
