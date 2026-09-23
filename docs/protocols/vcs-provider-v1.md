# VCS provider protocol v1

Status: wire contract implemented by `octacity-vcs-protocol`; verified registry
and bounded process hosting implemented by `octacity-server-vcs`; first Git
implementation provided by `octacity-vcs-git`.

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
  "repository_locator": "https://git.example.test/team/repository.git",
  "credential_handle": "credential-handle-01"
}
```

`repository_locator` is the credential-free locator from the immutable
repository snapshot. `credential_handle` is resolved inside the selected
adapter. Raw repository credentials never enter protocol JSON, locators, logs,
audit records, or core types. Debug formatting redacts both the locator and the
credential handle.

The language-neutral v1 requests are
[`list-references-v1.json`](../../server/protocols/octacity-vcs-protocol/fixtures/list-references-v1.json),
[`read-commit-v1.json`](../../server/protocols/octacity-vcs-protocol/fixtures/read-commit-v1.json),
[`list-tree-v1.json`](../../server/protocols/octacity-vcs-protocol/fixtures/list-tree-v1.json),
[`read-file-v1.json`](../../server/protocols/octacity-vcs-protocol/fixtures/read-file-v1.json),
and [`resolve-revision-v1.json`](../../server/protocols/octacity-vcs-protocol/fixtures/resolve-revision-v1.json).

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

## Verified registry and process host

The operator registry contains one real directory per adapter. Each directory
contains a strict `adapter.toml` and a normalized relative executable path. The
directory name and `adapter_id` must match. Registry discovery rejects symbolic
links, unsafe writable permissions where Unix permission bits are available,
oversized manifests or executables, incompatible protocol ranges, unknown TOML
fields, and executable digest mismatches.

Before every operation the host checks the advertised capability and verifies
the executable again. It starts one environment-cleared process, checks the
digest a second time before writing the request, sends and receives one bounded
newline-delimited frame, and requires a successful clean exit with no extra
stdout or oversized stderr. Request correlation and operation-specific response
semantics are checked by the protocol decoder. Timeout and caller cancellation
first send a cooperative `cancel` command and then terminate the complete
process group after a bounded grace period.

Host failures map to stable `invalid_request`, `unsupported`, `permanent`,
`transient`, `cancelled`, and `protocol_fault` classes. Adapter failures retain
the wire-level class listed above. Neither stderr nor credential handles are
included in host errors; credential handles are redacted from debug output.

## Git adapter installation and safety boundary

`octacity-vcs-git` implements all five v1 capabilities. Its installed
`adapter.toml` advertises protocol range `1..=1`, enables every VCS capability,
and pins the SHA-256 of the adapter executable in the normal registry format.
The executable expects a strict `git-adapter.toml` beside it:

```toml
config_version = 1
git_path = "/usr/bin/git"
credential_directory = "credentials"
allow_file = false
max_git_output_bytes = 4194304
max_repository_bytes = 536870912
max_blob_bytes = 67108864
```

`git_path` must resolve to an absolute, executable regular file that is not
writable by group or other users. The credential directory is a real private
directory confined below the adapter directory. The reserved handle
`anonymous` selects no credential configuration. Every other opaque handle
selects `<sha256(handle)>.gitconfig` below that directory; the selected file
must be a private regular file, not a symlink. Such a Git configuration is an
operator-owned trust input and may configure a reviewed credential helper.

The production transport allowlist contains only HTTPS. HTTPS locators must
have a host and cannot contain user information or a query string. Local
absolute paths are rejected unless `allow_file` is explicitly enabled; this is
intended for isolated testing and operator-controlled mirrors. Git receives an
empty environment, no terminal prompt, no system or ambient user config, a
fixed protocol allowlist, disabled hooks, disabled filters/LFS smudging,
disabled maintenance, and bounded output.

Metadata reads that need objects clone into a process-owned temporary **bare**
repository and use `rev-parse`, `cat-file`, and `ls-tree`. The adapter never
creates or returns a working tree. Repository hooks, attributes, executable
files, submodules, symlinks, and Octafiles are treated as object data and are
not evaluated or followed. The host timeout bounds the operation; operators
should additionally place the adapter temporary directory on a quota-limited
filesystem when cloning untrusted repositories. The configured post-clone
repository-size and blob-size limits prevent retained object data and response
memory from growing without a declared bound.

## V1 limits and strictness

- identifiers, references, revisions, cursors, and paths: 1024 UTF-8 bytes;
- complete encoded request or response: 2 MiB;
- reference or tree page: 256 entries;
- decoded file fragment: 1 MiB;
- implementation metadata: 32 entries;
- metadata value, commit message, author display, or diagnostic: 16 KiB.

Bounded textual fields reject control characters. The contract tests reject
unknown nested fields, incompatible versions, oversized requests and responses,
results exceeding the requested page or file limit, invalid retry
classification, mismatched result types, and malformed base64 content.
