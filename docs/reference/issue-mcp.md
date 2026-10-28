# Read-only Issue MCP

The optional `POST /mcp/issues` endpoint supplies OctaCity Issue investigation to
native MCP clients. It reuses `IssueHandlers`, authorized application queries,
Issue revisions and the existing store. It installs no Source, Reporter,
execution or Issue mutation operation. Server configuration and private runtime
files own its installation; there is no token-issuance API or credential database.

## Install a reader

Add a top-level entry to the private server TOML and mount its credential file:

```toml
[[issue_mcp_readers]]
id = "project-triage"
project_id = "0199a6f4-d56c-7440-9aa2-6a320f862795"
credential_file = "/run/secrets/octacity-issue-mcp-triage"
expires_at_unix_ms = 1793221200000
# Optional exact Issue allowlist. Omit for all Issues in this Project.
# issue_ids = ["0199a6f4-d56c-7440-9aa2-6a320f862799"]
```

Set expiry for the installation's intended lifetime; the example timestamp is
2026-10-28 21:00 UTC. An explicitly empty `issue_ids = []` grants no Issue
visibility. IDs contain at most 64 lowercase ASCII letters, digits, `_` or `-`.
A server accepts at most 128 unique readers and 256 Issue IDs per reader.
Inline token values, write permissions and unspecified Projects are rejected.

The private file contains `ocmcp.<id>.<64 lowercase hexadecimal digits>`, with an
optional trailing newline. Generate the 32-byte secret with an operating-system
cryptographic random generator. On Unix the existing private-file owner requires
a regular file owned by the server's effective user, without group/other access;
symbolic links are rejected. Runtime-file validation checks this installation
without contacting an MCP, Issue, model or provider service. Offline validation
rebases the credential reference with the other server runtime files.

Configure the Agent's selected native MCP connection with this endpoint and its
own protected bearer reference. Keep AI-provider, Source and Reporter credentials
separate. Do not put token bytes in a Role/Profile, prompt, command argument,
public result or log. Native AI-plugin execution qualification is task 4.3 of
`add-factory-triage-pipeline`; this endpoint alone does not establish an executable
triage preset.

Every call verifies the current private file and expiry. Removing the file,
changing its permissions or rotating its contents rejects the old token
immediately, including the recheck after an outstanding query. Changing the
installed Project/resource scope uses the ordinary server configuration update
and restart. The server never accepts a Project supplied in tool arguments or
borrows trusted-network management authority for an MCP caller.

## Protocol and tools

The endpoint implements the JSON-response Streamable HTTP subset of MCP
`2025-11-25`: `initialize`, `ping`, `tools/list`, `tools/call` and notifications.
The client can reject a negotiated version it does not support. Unsupported
explicit `MCP-Protocol-Version` headers fail with HTTP 400. There is no SSE
stream, server-initiated request or OAuth enrollment flow; `GET` returns 405. Clients must send JSON and a preinstalled bearer. Browser `Origin`
headers are rejected, and responses carry `Cache-Control: no-store`.

Initialization issues an opaque `Mcp-Session-Id`. Echo it on subsequent calls:
request IDs and cancellation are isolated by verified connection/Project and
session, allowing parallel native clients to reuse their own numeric request IDs.
Session metadata contains no bearer or cached grant. Sessions are bounded to
1,024 entries globally and 64 per verified connection, with a 30-minute lifetime;
an unknown, foreign, closed or expired session
returns 404, requiring reinitialization. Sessionless calls remain possible but
share the verified connection's request-ID namespace. Authentication and current
read grants are checked independently of session correlation on every call.
Correlation is process-local; a proxy serving multiple replicas must route a
session and its cancellations to the process that issued it. This task qualifies
the single-server endpoint.

Terminate a native client with authenticated `DELETE /mcp/issues` and its
`Mcp-Session-Id` header. No JSON body is required. The current connection owner
receives HTTP 204; the session is removed, its quota slot is released immediately
and its outstanding reads are cancelled. Another connection cannot terminate it.
A missing session header returns 400; unknown, foreign or expired sessions return
404. Admission and termination share one lock, so finishing a delayed request
body cannot start a read under an already closed session. The finite lifetime
also cleans up abandoned sessions when new clients initialize.

The installed gateway forwards Authorization only on the exact `/mcp/issues`
route; ordinary Console credential stripping remains in place. Publish this route
through the existing TLS gateway and network controls, without adding browser
login or exposing the private management listener.

| Tool | Arguments | Observation |
| --- | --- | --- |
| `search_issues` | `query`, optional `limit` and `cursor` | Case-insensitive title matches from one visible Issue page, revision-bearing summaries, continuation and page coverage |
| `read_issue` | `issue_id`, optional positive `version` | Current or exact retained Issue revision, including its existing content-availability metadata |

Search examines at most one authorized page per call. It applies visibility
before ordering, cursor positioning, filtering and coverage. A page with zero
matches may still have a continuation. Follow `next_cursor` with the same query
and current connection scope. An empty query browses bounded visible pages.
This surface has no full-text index or server-side duplicate decision.

Successful tool results include `structuredContent` and equivalent JSON text.
`scope` identifies the installed Project and observation kind. `content_digest`
is SHA-256 of the JSON observation before adding the digest and
`observed_at_unix_ms`; timestamps use server observation time. Search coverage
reports `scanned_entries`, `has_more` and `consistent_snapshot: false`. Coverage
is page-local and does not prove global absence or an atomic snapshot across
successive pages. Exact reads preserve the store's real Issue version.

The adapter explicitly selects detail fields: `id`, `project_id`, `number`,
`version`, `applicability_version`, `content_available`, `content`, `workflow_id`,
`workflow_version`, `status_id`, `created_at` and `updated_at`. `content` contains
`title`, `description`, `acceptance` and `priority`. Nonempty `planning` exposes
`definition_version`, `assignee`, `labels`, `due_date`, `values` and `parent_id`;
`values` retains the existing closed property-value grammar. Adding fields to the
internal store revision does not add them to this wire contract. Empty planning
remains omitted, preserving the existing response shape.

| Bound | Value |
| --- | --- |
| Query / cursor | 256 / 1,024 UTF-8 bytes |
| Page size | Default 25, maximum 50 visible entries |
| Request / encoded response | 16 KiB / 256 KiB |
| Whole request deadline | 30 seconds, including authentication and body reads |
| Active tool calls | 64; cancellation remains available at capacity |

There are no partial successful payloads on overflow. Tool failures have
`isError: true` and a closed `structuredContent.status`: `not_available` for
hidden, foreign or absent details; `unavailable` for backend lookup failure;
`cancelled` for cancellation; `too_large` for an oversized encoded result.
These failures supply no empty page or absence coverage. Malformed arguments and
unknown/mutation tools produce JSON-RPC errors. Invalid, expired or revoked
credentials return HTTP 401 without result metadata; overload returns 429 and a
whole-request timeout returns 504.

Cancellation notifications refer to the original JSON-RPC request ID in the same
verified connection/session. They drop the pending read future and release its
registration. A completed query cannot release data after its connection, Project,
visibility or expiry changed during the asynchronous read.

Implementation ownership: [protocol adapter](../../server/api/octacity-server-api-mcp/src/lib.rs),
[scoped application reads](../../server/application/src/management_security/issue_read.rs),
[server installation](../../server/app/src/runtime/issue_mcp.rs).

## Native execution qualification

Role/Profile MCP selections use the ordinary compiler, managed Build and Agent
credential materialization. The qualified executor is the Octa 0.8.0 Codex plugin
with Codex CLI 0.161.0. Each connection needs its own approved credential purpose
and resource scope; AI-provider and Reporter credentials cannot substitute for it.
The materializer rechecks current connection authority after Context reads and
input publication, including retained-package recovery. A Role source without
connection authorization support fails closed for selected MCP connections.

The opt-in native test starts this real Issue router over authorized in-memory
Issue handlers and connects the plugin's native client through a test TLS proxy.
It also selects an independent finite stdio peer. No paid inference or live
provider mutation is required:

```sh
OCTACITY_TEST_CODEX_BIN=/absolute/path/to/codex-0.161.0 \
OCTACITY_TEST_CODEX_PLUGIN_BIN=/absolute/path/to/octa_plugin_codex-0.8.0 \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -j2 -p octacity-server-api-mcp --test native_triage \
  -- --ignored --nocapture
```

This exercises native search/detail calls, hidden-candidate refusal, independent
HTTP/stdio credentials, resource/write refusal, malicious candidate text,
ambient-setting isolation and current token revocation. The finite model chooses
prescribed tool calls; it does not qualify semantic duplicate judgments or prove
that a model will ignore every prompt injection. Actual authority remains enforced
outside the model. Other AI plugins and ready-made external server releases need
their own qualification. See the [triage execution evidence](../../openspec/changes/add-factory-triage-pipeline/mcp-execution-verification.md)
for the consumed packaged Agent evidence and precise verification boundaries.

## Retained investigation evidence

An ordinary Build binding can explicitly select the independent
`factory_mcp_evidence_verifier()` pin (`native-mcp-evidence`, version `v1`). Use
`factory_mcp_conclusion_shape(assessment)` for its closed stage output schema.
The compiler supplies the model with the assessment and native-value claim
fields; the result owner adds `evidence`, each claim's `reference`, and
`verification` after reading the original immutable Build Artifacts. Selecting
this verifier requires a retained Role with selected MCP connections. Legacy or
missing native capture is an explicit unavailable result.

A model-visible claim names the native connection, tool, exact arguments and
consumed value. For example, a claim about an Issue detail encoded in the first
MCP text content block has this form:

```json
{
  "connection": "issues",
  "tool": "read_issue",
  "arguments_json": "{\"issue_id\":\"0199a6f4-d56c-7440-9aa2-6a320f862799\"}",
  "json_text": 0,
  "pointer": "/issue/version",
  "value_json": "3"
}
```

`arguments_json` and `value_json` contain JSON values using the selected server's
native schema. `pointer` is a JSON Pointer into the consumed response; optional
`json_text` first decodes the indicated standard MCP text block as JSON. The
server requires one unique matching captured call/value. The model does not need
to know a future Artifact ID. Invented candidates, altered values, ambiguous
matches and substituted Artifact/record/digest references are rejected.
This is the single model-facing claim grammar; the earlier reference-only
experimental form is rejected. Decoded JSON values and the final retained
document pass the same sensitive-material checks as the incoming envelope.

The native version-2 trace retains actual iterative queries and responses,
including candidate identifiers, remote revision/scope/coverage fields when
supplied, and local observation times. The owner binds them to the original
Role package, connection configuration/resource, Build/Attempt/Job, trace and
provenance Artifacts. Sanitized response digests identify the retained bytes;
they do not invent a remote revision. At most 64 calls and 64 claims/references
are accepted; overflow fails without silently dropping observations. Existing
Job output limits and the 1 MiB Flow-data limit also apply.
Claim pointers share the ordinary Flow mapping limit of 256 UTF-8 bytes. The
model and retained schemas share the same typed claim definition and bounds.
Every native start/completion record must contain its actual `arguments` field;
explicit JSON null is distinct from missing capture.

`verification.status: observed` means that the cited fields were captured and
matched. It does not establish a duplicate decision. Empty or failed results,
malformed content and interrupted calls remain distinct. Without usable claims,
the stage receives `needs_review`; a cancelled execution supplies no successful
semantic result. Coverage stays `unknown`, and `global_absence_proven` and
`duplicate_proven` stay false regardless of model assertions. Stage eligibility
policy must separately assess the retained candidate and coverage obligations.
An error or malformed response can retain an inspectable candidate field while
still requiring review. A candidate claim with no supporting result is rejected.
A field actually present in a truncated response can be observed; unknown
coverage and false proof flags still prevent treating that observation as
duplicate or absence proof.

The [evidence verification report](../../openspec/changes/add-factory-triage-pipeline/mcp-evidence-verification.md)
records Red/Green cycles, ordinary signed-Job/Artifact regressions and a test that
feeds actual published plugin/native-client captures into the public verifier.
