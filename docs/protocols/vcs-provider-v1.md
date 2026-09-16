# VCS provider protocol v1

Status: wire contract implemented by `octacity-vcs-protocol`; bounded process
hosting and the first Git adapter are not implemented.

## Purpose and boundary

This protocol gives the server bounded read-only access to repository metadata
without coupling core logic to Git or a hosting provider. It supports reference
discovery, immutable commit metadata, tree browsing, bounded file reads, and
one-time resolution of an allowed mutable reference.

The adapter must treat repository-controlled names, messages, paths, and bytes
as data. It must not evaluate an Octafile, invoke repository scripts, load a
repository plugin, or materialize a working tree for build execution.

## Version and adapter negotiation

An `AdapterManifest` declares `adapter_id`, immutable executable SHA-256,
inclusive protocol range, and the exact supported operations. Negotiation
selects the newest common version. Invalid or disjoint ranges are rejected
before the host supplies a credential handle or repository request.

V1 capabilities are `list_references`, `read_commit`, `list_tree`, `read_file`,
and `resolve_revision`. The host must not dispatch an operation the adapter did
not advertise.

## Encoding and envelopes

Messages are strict UTF-8 JSON. Every object rejects unknown fields. Requests
contain `protocol_version`, `request_id`, and a tagged `command`; responses echo
the version and request identity and contain a tagged `outcome`. The complete
encoded request or response is limited to 2 MiB before JSON decoding. Parsing,
strict schema checks, and semantic validation form one decode operation.
Responses are accepted only when version and request identity match the
outstanding request.

Repository operations include a nested `repository` object:

```json
{
  "operation_id": "operation-01",
  "repository_id": "repository-01",
  "credential_handle": "credential-handle-01"
}
```

`credential_handle` is resolved inside the selected adapter host. Raw
repository credentials never enter protocol JSON, logs, audit records, or core
types.

The golden v1 request is
[`resolve-revision-v1.json`](../../server/protocols/octacity-vcs-protocol/fixtures/resolve-revision-v1.json).

## Operations

| Operation | Required fields | Result |
| --- | --- | --- |
| `list_references` | repository, optional cursor and prefix, bounded page size | ordered `references` page |
| `read_commit` | repository, immutable revision | normalized `commit` |
| `list_tree` | repository, immutable revision, optional path/cursor, bounded page size | ordered `tree` page |
| `read_file` | repository, immutable revision, path, offset, maximum bytes | base64 `file` fragment |
| `resolve_revision` | repository, allowed mutable reference | exact `resolved` revision |
| `cancel` | target operation identity | `acknowledged` or `cancelled` failure |

Reference and tree cursors are opaque. Clients must not parse them or construct
new cursors from provider state. Pages contain at most 256 entries. One file
fragment contains at most 1 MiB of decoded bytes and declares whether more data
remains.

`resolve_revision` is the boundary between mutable selection and immutable
Build input. Once accepted, a Build records the returned revision and never
silently follows later branch or tag movement.

## Normalized data

Reference results contain a full name, `branch`, `tag`, or `other` kind, and an
immutable revision. Commit results contain revision, bounded ordered parents,
message, optional display-only author, optional Unix-millisecond commit time,
and bounded opaque metadata.

Tree entries contain a repository-relative path, optional size, and one of
`file`, `directory`, `symlink`, or `other`. Returning a symlink does not
authorize the server to follow it on the host filesystem. File data is base64
and is never interpreted or executed by the server. Paths use normalized `/`
separators and reject absolute paths, empty components, `.` and `..`.

## Cancellation and idempotency

Every operation has an `operation_id`. `cancel` targets that identity and is
cooperative and idempotent. Read operations and revision resolution are safe to
retry only under the host's bounded retry policy. A resolution retry must
preserve the same request semantics; the authoritative Build records only the
accepted immutable result.

## Failure classification

Failures contain a stable code, bounded secret-free diagnostic, optional retry
delay, and one of:

- `invalid_request`;
- `unsupported`;
- `permanent`;
- `transient`;
- `cancelled`;
- `protocol_fault`.

Only `transient` can carry `retry_after_ms`. Authentication failure, missing
repository, forbidden reference, and invalid path are not automatically
transient. The server applies retry only to idempotent operations and retains
its own attempt and elapsed-time limits.

## V1 limits and strictness

- identifiers, references, revisions, cursors, and paths: 1024 UTF-8 bytes;
- complete encoded request or response: 2 MiB;
- reference or tree page: 256 entries;
- decoded file fragment: 1 MiB;
- implementation metadata: 32 entries;
- metadata value, commit message, author display, or diagnostic: 16 KiB.

Bounded textual fields reject control characters. The golden contract tests reject
unknown nested fields, incompatible versions, oversized requests, invalid
retry classification, and malformed base64 content.
