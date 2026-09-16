# Artifact transfer protocol v1

Status: shared wire contract implemented by `octacity-protocol`; logical
artifact lifecycle and the object-store interface remain separate server
modules.

## Purpose and boundary

This protocol describes logical artifacts, reports, and immutable redacted log
chunks without exposing a storage backend. It is shared by agent and server
product areas. S3 buckets, keys, access keys, SDK responses, generations, and
ETags are never protocol identities.

The artifact contract complements the server-agent output endpoints: those
endpoints authenticate the current lease and map fenced output operations to
this provider-neutral logical transfer model. The server's artifact domain owns
publication and retention state; a byte-store adapter implements physical
transfer and verification.

## Version and encoding

Peers advertise an inclusive `ArtifactProtocolRange` and select the newest
common version. Version zero, inverted ranges, and disjoint ranges are rejected.

Messages are strict UTF-8 JSON. Unknown fields and unknown enum variants are
rejected. Requests contain `protocol_version`, `request_id`, and a tagged
`command`. Responses echo the version and request identity and contain a tagged
`outcome`. The complete encoded request or response is limited to 256 KiB
before JSON decoding. Parsing, strict schema checks, and semantic validation
form one decode operation. Responses are accepted only when version and
request identity match the outstanding request.

The golden v1 request is
[`begin-upload-v1.json`](../../shared/protocol-fixtures/artifact/begin-upload-v1.json).

## Immutable content identity

`ArtifactContentIdentity` is the pair:

- exact `size_bytes`;
- lowercase 64-character SHA-256 of the exact bytes.

An ETag is never accepted as content identity. Completion must independently
verify the exact byte count and SHA-256 for the authorized immutable generation
before publication.

Logical kinds are:

- `artifact` for a user-visible produced file or archive;
- `report` for machine-readable output with an open format identifier;
- `log_chunk` for bounded immutable redacted stdout or stderr bytes.

Unknown valid report formats remain opaque strings; the server does not use a
closed provider-format enum.

## Operations

### `begin_upload`

Reserves one logical upload using `operation_id`, `idempotency_key`, opaque
`artifact_id`, bounded name, kind, media type, optional report format, content
identity, and bounded opaque metadata. Exact replay must resolve to the same
logical pending upload rather than another published output.

The successful `transfer` outcome returns the logical artifact identity,
optional pending `upload_id`, and an opaque short-lived capability. The
capability contains a URL, required signed headers, and Unix-millisecond expiry.
It is sensitive: Debug output redacts the URL and all header values.

### `complete_upload`

Names `operation_id` and the pending `upload_id`. Success returns `published`
only after independent verification and the authoritative publication mutation.
A mismatch returns an `integrity` failure and the bytes remain unavailable.

### `abort_upload`

Revokes a pending upload idempotently. Success returns `aborted`. Repeating an
abort cannot make an object visible again.

### `begin_download`

Names the logical `artifact_id`. A successful `transfer` outcome contains only
a bounded short-lived capability; it never reveals a permanent object key or
storage credential.

### `cancel`

Targets an in-flight operation identity. Cancellation is cooperative and
idempotent. It does not roll back an already published immutable object.

## Failure classification

Artifact failures use:

| Class | Meaning |
| --- | --- |
| `invalid_request` | Invalid logical request or policy input |
| `unsupported` | Operation or transfer mode is unavailable |
| `permanent` | Logical object or authorization cannot succeed unchanged |
| `transient` | Exact idempotent request may be retried within server limits |
| `cancelled` | Cooperative cancellation completed |
| `integrity` | Stored bytes differ from declared immutable identity |
| `protocol_fault` | Peer violated version, framing, or schema rules |

Only `transient` may include `retry_after_ms`. Diagnostics are bounded and must
not include presigned URLs, signed header values, access keys, bucket names, or
physical object keys.

## V1 limits and security rules

- identifier, name, media type, format, or metadata key: 256 UTF-8 bytes;
- complete encoded request or response: 256 KiB;
- metadata or signed-header entries: 32;
- metadata or signed-header value: 4096 UTF-8 bytes;
- opaque transfer URL: 8192 UTF-8 bytes;
- SHA-256: exactly 64 lowercase hexadecimal characters.

Control characters are rejected in bounded textual fields. The server-agent
transport and byte-store interface may impose smaller deployment limits. Log
chunks must be redacted before either object storage or search indexing; this
transfer protocol does not authorize storing unredacted log data.
