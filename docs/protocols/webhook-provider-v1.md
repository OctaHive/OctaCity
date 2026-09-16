# Webhook provider protocol v1

Status: wire contract implemented by `octacity-webhook-provider-protocol`;
bounded process hosting and production provider adapters are not implemented.

## Purpose and boundary

This protocol connects the OctaCity webhook adapter host to one
operator-installed provider adapter. It is not the public webhook HTTP API.
The HTTP ingress preserves a bounded raw request, and the selected provider
adapter authenticates those exact bytes before it may return a normalized
repository event.

```text
provider HTTP delivery
  -> bounded webhook ingress
  -> selected adapter receives exact body + allowlisted headers
  -> adapter verifies provider authentication
  -> adapter returns AuthenticatedRepositoryEvent
  -> trigger engine deduplicates integration_id + delivery_id
```

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

The golden v1 request is
[`verify-delivery-v1.json`](../../server/protocols/octacity-webhook-provider-protocol/fixtures/verify-delivery-v1.json).

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

Control characters are rejected in bounded textual fields. Diagnostics must
not contain raw bodies, signature material, credential values, or provider
administration secrets.

The Rust contract and its golden fixture are tested for round-trip stability,
unknown nested fields, incompatible versions, cancellation, limits, and retry
classification.
