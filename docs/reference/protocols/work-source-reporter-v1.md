# Work Source and Reporter process contracts v1

`octacity-work-source-protocol` owns two independently versioned contracts.
`octacity-server-work-source` hosts them through the existing verified adapter
registry and process runner. These are server-side ticket integrations, separate
from Agent source materialization and the Delivery protocol.

Provider identities are open strings. Native ticket containers, item ids, display
keys and observed states are opaque bounded data. GitHub repository ids, GitLab
project ids and Jira project/item keys do not add variants to Factory execution.
The code Repository selected for Work remains independent of the ticket container.

## Installation and negotiation

An operator installs a regular executable and strict `adapter.toml` in a real
registry directory named for its adapter identity. The common registry checks
permissions, confinement and SHA-256; the host verifies the executable again for
each invocation. Work input cannot select an executable path or install a grant.

The installation envelope is version 1. An illustrative manifest is:

```toml
manifest_version = 1
adapter_id = "tracker"
executable = "provider"
executable_sha256 = "<verified-executable-sha256>"

[protocol.source]
min = 1
max = 1
[protocol.reporter]
min = 1
max = 1

[capabilities.source]
authorize = true
cancel = true
credential_class = "work_source_read"
configuration_digest = "<immutable-discovery-profile-sha256>"

[capabilities.reporter]
observe = true
project = true
cancel = true
idempotency = "observe_before_retry"
credential_class = "work_reporter_write"
configuration_digest = "<immutable-reporting-profile-sha256>"
```

Omit both sections for a role to leave it uninstalled. At least one role is
required in a Work adapter entry. A Source range incompatible with the host
does not disable a compatible Reporter range in the same valid installation.
Each operation negotiates against its own range before exposing a protected
handle. Version 1 selects version 1; invalid ranges fail closed. Negotiation
uses the verified operator manifest, not a provider-claimed unauthenticated hello.

Reporter publication requires observation and `observe_before_retry`. Source
read, Reporter write, repository read/Job and Delivery authority are distinct.
Handles name private installed grants rather than contain tokens or paths. The
provider must verify the stored grant class, resource/native identity, immutable
configuration and actual network destinations before reading credentials or
performing effects. A class field in a request does not itself grant authority.

Delivery retains its independent install/wire contract and registry. The same
provider binary can implement both protocols, but a Delivery-only manifest or
credential cannot enable a Work role. Manual Factory intake requires neither.

## Wire and operations

Each invocation starts one environment-cleared process for one operation. stdin
and stdout carry bounded JSON followed by a newline; stderr is bounded and is
never returned as provider diagnostics. Every object/enum is closed: unknown
fields, variants, versions and credential classes are rejected.

The Work envelope is `{"family":"source"|"reporter","message":...}`.
Each message has `protocol_version`, `request_id` and `command` or `outcome`.
Commands use `operation`/`payload`; outcomes use `status`/`payload`. Canonical
requests live in the protocol crate's `fixtures/scan-v1.json` and
`fixtures/project-v1.json`.

Source supports `scan`, explicitly advertised `authorize`, and `cancel`. A scan freezes its stable operation id,
installed profile/selector references, exact resource/read handle, network
ceiling, optional fixed update window, opaque cursor and item/byte ceilings.
One successful `page` echoes scan/resource/profile/selector and contains complete
normalized candidates plus an optional next cursor. Duplicate native item ids
within a page or substituted containers are rejected. An empty page is successful
discovery, distinct from a provider failure. A cursor is private provider data;
it cannot authorize an URL, redirect or a broader resource.

Each candidate carries container/item/display/update identities, title, optional
body, explicitly normalized fields, literal labels and native state. Text is
private untrusted input, never credentials, profiles, executable parameters,
admission or Flow authority. Provider DTO decoding and selection happen in the
adapter; translation into the retained application snapshot happens at the server
integration boundary. Comments, attachments and linked-resource fetches are absent.

A group scan supplies up to 63 `additional_selections` beside its primary
selector. The provider evaluates all of them against each same observed item,
returns their union, and includes exact `matching_selections` per candidate.
Missing, duplicate, empty or foreign proofs are rejected before checkpoint or
admission. A legacy single-selector page can omit these additive fields. The
server maps these observations to current members under the group fence; the
provider cannot choose a Factory target. Continuations bind the complete selector
set.

`authorize` carries the original Source operation id, pinned profile, exact
resource/read handle and network ceiling, without a selector, cursor or ticket
text. `authorized` echoes the operation, native container and profile. The host
requires `capabilities.source.authorize = true` before exposing the handle. The
plugin rechecks its current private installed read grant and resource scope; this
is independent of new discovery and grants no management/admission permission.
Cancellation uses the same original Source id. Older discovery-only plugins stay
loadable but cannot supply the current grant check required by the polling worker.

Reporter supports `observe`, `project` and `cancel`. Observe/project repeat the
original projection id, exact container/item/display context, separate write
handle, profile, installed mapping, arbitrary status and network ceiling. Native
labels/workflow transitions stay in the pinned mapping. `observed` reports whether
current state matches it; `applied` requires a matching receipt. Responses echo
the exact operation, native item, profile, mapping and value. They cannot select
another item, rewrite a Factory result or turn partial effects into success.

Both roles return `acknowledged` only for their matching cancellation target.
Failures carry only a closed class and optional bounded retry hint; they contain
no raw provider response, URL or free-form diagnostic. The owner retains original
deadlines, retry budgets, outbox ordering and admission/checkpoint authority.

## Bounds, privacy and lifecycle

Requests are at most 64 KiB and responses at most 4 MiB, excluding the framing
newline. A scan may narrow the complete received response ceiling, including JSON
whitespace. Each page contains at most 100 candidates; each encoded candidate is
at most 1 MiB. Titles are at most 4096 UTF-8 bytes; bodies/normalized field values
at most 262144 bytes; fields at most 32; labels at most 64 with 256 bytes each.
Opaque continuations are at most 4096 bytes. Identifiers/handles are at most 1024
bytes; display/update ids at most 256; native states at most 128. Retry hints are
at most one hour. Caller policies can narrow ceilings; provider data cannot raise
them. Inputs are rejected rather than silently truncated.

Credential handles, locators, private candidates and cursors have redacted Debug
representations. Normalized snapshots remain private data subject to the Factory
preparation/Artifact retention policy, not telemetry or Work evidence by default.

The common host enforces an absolute operation timeout, cancellation grace,
framing/correlation checks and whole-tree cleanup, including inherited pipes.
Cooperative cancellation, when advertised, addresses only the original scan or
projection and contains no credential handle. When it is unavailable the host
still force-stops the process tree. Late terminal output cannot replace the
original cancellation/timeout outcome. Failures remain local to the input/effect.

The GitHub executable recognizes these families independently of Delivery and
acknowledges matching cancellation. Its private `github-work.toml` enables Source
and Reporter repositories separately and pins the exact native container/API
profile. It resolves independent read/write grant files only after profile,
resource and network validation; profile drift, unsafe files and grant substitution
return closed failures without provider calls. See the provider's
[installation format](../../../server/infrastructure/octacity-github/README.md#independent-issue-scopes).
Source scans resolve a private pinned selector and return one normalized Issue
page with an opaque continuation, using the same pinned HTTPS SDK transport as
Delivery. Native repository identity, local selector checks, pull-request
exclusion, byte ceilings and strict pagination confinement precede candidate
publication. Reporter effects remain `unsupported`; their mappings belong to
subsequent implementation tasks. Source worker composition is an explicit
trusted runtime extension.
Protocol support alone neither advertises an active Issue capability nor starts
polling or provider writes.

The application polling owner retains an original normalized page under the
existing scan fence before processing it. Its inclusive overlapping incremental
window and periodic full reconciliation have distinct stable pass identities;
provider continuations are never transferred between those contexts. Item, page
and canonical normalized-byte budgets bound each invocation independently of the
wire ceilings above. Unfinished private pages have a fixed retention deadline;
cleanup discards their cursor and forces a conservative rescan, with ordinary
Factory admission replay suppressing duplicate Work. The optional runtime Source
worker composes verified reads/current grant checks, exact current group selection,
retained preparation and ordinary admission under this fence. See the
[composition boundary](../../architecture/factory-extension-boundaries.md#optional-source-worker-composition).
Reporter effects and operator-facing activation remain subsequent tasks.
