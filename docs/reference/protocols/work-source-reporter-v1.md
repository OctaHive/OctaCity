# Work Source v1, Reporter v1/v2 and review v1 process contracts

`octacity-work-source-protocol` owns independently versioned Source, Reporter and
read-only review contracts.
`octacity-server-work-source` hosts them through the existing verified adapter
registry and process runner. These are optional server-side integrations, separate
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
max = 2

[capabilities.source]
authorize = true
preview = true
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
handle. The requested version must be advertised exactly; invalid ranges fail closed.
Source remains version 1. The Factory ordered projector requires Reporter version 2
and never downgrades to a bulk version-1 write. Negotiation
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

Source v1 optionally supports `preview`, advertised independently with
`capabilities.source.preview = true`. Omission defaults to false; a legacy scan
capability never implicitly authorizes preview. The request uses the exact
`ScanIntent` resource/read handle, selector set, cancellation id and page bounds.
The hosted management reader supplies no production cursor or scan window, takes
one page, and narrows the caller's item/byte limits by every installed profile.
Grant resolution and process execution share one original absolute deadline.

`previewed` echoes scan/profile/native-container identity and contains bounded
native summaries, an optional private continuation and one explanation for every
requested selector on every item. Summaries contain only `item_id`, `display_id`,
UTF-8 title (at most 512 bytes), native state and bounded literal labels. They
contain no body, mapped Artifact input, credentials, internal Issue or execution
instructions. Empty reasons mean a match; closed reasons are `state_mismatch`,
`required_label_missing`, `alternative_label_missing`, `excluded_label_present`
and `predicate_mismatch`. Missing/duplicate/foreign selectors, duplicate native
items, invalid summaries and an oversized encoded response fail closed. A preview
may include filtered items; an exclusive group with several matching members is
ambiguous. Neither result is an admission decision.

The GitHub adapter shares its bounded HTTPS metadata/listing path with Scan,
excludes pull requests, truncates titles on UTF-8 boundaries and issues GETs only.
Provider failures use the existing closed failure response; throttling retains
bounded retry advice and cannot become a successful empty preview. The hosted
reader discards private continuation bytes and exposes only `has_more`.

Both Source v1 and Reporter v1/v2 optionally support read-only `check`. Each role
must advertise `check = true` in its own capability section; omission defaults to
false and never enables a legacy adapter implicitly. A canonical request is
`fixtures/check-v1.json`. It carries `check_id`, the pinned provider configuration,
expected approved `endpoint`, exact resource/private role handle and trusted
network ceiling. It has no selector, cursor, native item, mapping, Factory input
or internal Issue identity. Reporter checks still require `work_reporter_write`;
they cannot borrow Source, VCS or Delivery credentials. The endpoint is an expected
installed target, not an arbitrary fetch URL.

`checked` echoes check/profile/native-container identity and reports separate
`configuration`, `authentication`, `resource_access` and `capability` states, plus
an optional retry hint. States are `ready`, `missing`, `invalid`, `denied`,
`conflict`, `unsupported`, `unverified`, `throttled` and `unavailable`. A Ready
aspect requires its preceding aspect to be Ready. No raw provider diagnostics,
token, handle or task snapshot is returned. An authenticated read does not prove
write readiness: adapters report Unverified when read-only evidence is insufficient
and never create a test Issue, label, branch or review. Checks grant no authority,
activate no polling/admission and advance no production cursor. Cancellation
addresses `check_id` using the role's existing cancellation target field.

Reporter version 1 supports `observe`, `project` and `cancel`. Observe/project repeat the
original projection id, exact container/item/display context, separate write
handle, profile, installed mapping, arbitrary status and network ceiling. Native
labels/workflow transitions stay in the pinned mapping. `observed` reports whether
current state matches it; `applied` requires a matching receipt. Responses echo
the exact operation, native item, profile, mapping and value. They cannot select
another item, rewrite a Factory result or turn partial effects into success.

Reporter version 2 supports `inspect`, `apply` and `cancel`. It rejects version-1
observe/project commands; version 1 rejects inspect/apply. Inspect carries the
same frozen intent and returns `planned`: an exact receipt, opaque SHA-256 field
identities and private conditional steps. Apply carries `{intent, step}`, checks
the current managed values, performs at most one native mutation and returns a
fresh `progressed` plan. A plan grants no write authority. Each step's fields
must include every affected managed group; identities must overlap across mapping
versions and relevant input bindings/installations for the same native item.
Fields must also namespace the canonical provider instance, container and item
to distinguish coincident native ids at unrelated providers. Native semantics
stay in the adapter. Each step must apply a commuting subset of the original
desired effects, including when it arrives late. Temporary transitions or workflow
routes that cannot meet this condition require a different future contract.

Plans allow up to 512 unique fields and 256 steps; each step has 1–256 unique
fields contained in the plan and an at-most-8192-byte token. Combined tokens are
at most 262144 bytes. Matching receipts have no remaining steps. Tokens are
redacted from Debug and are private process data, never telemetry or stored
Factory evidence. The existing total request/response ceilings still apply.

Both roles return `acknowledged` only for their matching cancellation target.
Failures carry only a closed class and optional bounded retry hint; they contain
no raw provider response, URL or free-form diagnostic. The owner retains original
deadlines, retry budgets, outbox ordering and admission/checkpoint authority.

## Bounds, privacy and lifecycle

Requests are at most 64 KiB and responses at most 4 MiB, excluding the framing
newline. Check responses have an independent 8192-byte ceiling, enforced by the
process reader and decoder. A scan may narrow the complete received response ceiling, including JSON
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
publication. Reporter observe/project resolve a separate private version-1 mapping
under an exact identity/digest and apply only its explicit managed-label and
optional Issue-state effects. Missing repository labels require repair; unrelated
labels are preserved and final effects must be observed before `applied`.
Source worker composition is an explicit
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
Reporter outbox composition is an explicit trusted runtime installation through
`HostedFactoryWorkStatusProjector` / `FactoryRuntimeResources::work_reporter_dispatcher`.
It resolves the original settled Work source, checks the current accepted-action
outbox claim and full permission ceiling, then inspects/applies using the original
operation id and pinned provider/mapping identity under one bounded window.
Provider retry hints narrow the existing outbox policy without renewing its deadline.
A missing or invalid Apply response leaves effects uncertain; an invalid Inspect
response cannot authorize a write. Valid typed Conflict, Permanent,
InvalidRequest and Unsupported failures release the current write permit;
transient, cancelled, missing or invalid completion remains uncertain.
Expired owners cannot settle a replacement
claim. A restarted worker rechecks actual effects without replaying accepted actions.

The memory/PostgreSQL stores assign a per-native-item acceptance sequence atomically
with the accepted action/outbox publication, independent of clocks and worker
arrival. The technical bucket uses Project and opaque native container/item ids
across installations; admission keeps its original installation scope.
Provider instance isolation belongs to the opaque field identities.
Opaque field heads and write hazards are technical correlation state,
not a second Work/Flow authority. Unknown newer scopes block older writes;
known disjoint scopes remain independent. Before every Apply the host revalidates
accepted authority, the current outbox fence and field ownership. Per-field
revisions fence the read window, including a write that begins and finishes
between its two checks. Delivery requires a stable matching read with no
unresolved older overlapping write. A late old write is corrected by the latest
intent within its original time, output and retry bounds. Deadline expiry cannot
cancel an already accepted remote request: unknown older completion remains an
explicit conflict after its original window, never a falsely quiet observation.
Known late completion may release only its original permit, without new write
authority. No provider CAS or permanent protection from subsequent human edits
is claimed. Conflict/exhaustion leaves accepted Work/Run/Flow unchanged.

Native mapping semantics remain entirely inside each provider adapter; manual
admission requires none of them. Stage-local actions use the same original
projection contract before intermediate/nested pipeline completion; stage/action
provenance stays in accepted Factory records rather than entering provider wire
payloads or granting native routing authority. Integration administration and
management/Console authoring retain their subsequent tasks.

Management composition and privacy semantics are documented in
[Factory source preview and diagnostics](../factory-source-diagnostics.md).

## Optional input-owned effects under Reporter v2

An installed Reporter can explicitly advertise `custom_effects = true` alongside
an ordered v2 protocol range. It remains false when absent. This advertises wire
support only; native configuration, Project grants and each accepted action still
authorize the operation independently. The verified host rejects custom effects
before process dispatch when support is absent. Bulk v1 cannot carry custom effects.

`ProjectionIntent.effects` is optional and contains closed bounded `add`, `remove`
and nullable opaque `state` fields. Each add/remove list has at most 64 unique
literal labels, each literal/state has at most 256 UTF-8 bytes, and the lists must
not overlap. The field is absent for existing installed-only requests. A receipt
must echo the exact custom effects; omission or substitution cannot confirm them.
Ordered step tokens bind the entire intent, including these effects, so retry and
late-write reconciliation retain their original scope and finite deadline.

GitHub's private installed status mapping must explicitly set `editable: true`
and may declare `editable_states: ["open", "closed"]` or a subset. With no declared
states, custom mappings preserve native state. Existing `managed_label_groups`
remain the label ceiling. The provider reuses its shared checked effect validator for
case-insensitive native label identity, contradictory effects and bounds. It
rejects unmanaged labels, forbidden states and absent editing support before any
native request. These fields remain installation-owned; pipeline clients submit
neither private configuration JSON nor mapping paths. Factory freezes the typed
logical mapping in its original input version and dispatches only accepted action
effects through the same Reporter/outbox. No extra poller, write path or card command
producer is introduced.


## Independently installed review reader v1

The optional `review` family shares the verified Work adapter host and JSONL
server, not Source, Reporter or Delivery authority. Old manifests remain valid
without it. To opt in, install these additional manifest sections (the digest is
that of the private provider configuration, not a token):

```toml
[protocol.review]
min = 1
max = 1

[capabilities.review]
checks = true
cancel = true
credential_class = "vcs_read"
configuration_digest = "<sha256-of-installed-provider-configuration>"
```

`observe`, `authorize`, optional `checks`, and same-read `cancel` are the closed
operations. Merge, publication, projection, transition and execution commands do
not exist. `authorize` checks private installed repository authority without
provider calls; it returns no metadata. Version/profile/class/capability checks
precede process dispatch. The existing `vcs_read` credential class is separately
required; ticket read/update and Delivery classes cannot substitute it.

The bounded intent pins a server-owned `read_id`, provider configuration digest,
installed resource alias, native code repository, native repository-scoped review
key, independent credential handle and permitted network hosts. Check pages also
pin the original head, opaque continuation and caller-narrowed item/byte bounds.
The wire accepts provider-neutral identities; GitHub uses a positive canonical PR
number scoped to the verified native repository and a forty-hex-character commit
SHA for checks. Display strings, ticket keys, branches and returned URLs never
select an endpoint. Response subjects must echo the original read/profile/native
repository/review; check heads must also match exactly.

Frames retain the common 64 KiB request and 4 MiB response ceilings. Each check
page is capped at 100 records and the caller's lower item/encoded byte limits.
Missing facts and unknown provider states/conclusions remain unknown. A partial
page, an empty list or a provider `success` conclusion cannot establish Factory
acceptance, merge permission or an Issue transition. Check pages contain bounded
IDs, names, visible producer, normalized lifecycle/conclusion and safe navigation;
provider outputs, annotations, bodies and logs are excluded.

The concrete GitHub reader extends the existing private `github-work.toml` with
`capabilities.review_repositories = ["installed-code-alias"]`. The existing
`repositories` table pins that alias to owner/name/native repository ID. Source
and Reporter aliases remain independently configured. The same existing private
credential directory and `GithubCredentialManager` repository-read class are used;
there is no ambient-token fallback or new token form. Existing private file and
network confinement checks apply.

GitHub verifies the current repository ID, then reads one exact PR or one commit
check-runs page through the shared bounded HTTPS transport. PR base-repository and
number correspondence and check head correspondence are checked. Pagination
reuses Source's logical cursor and Link validation. Its context includes original
read/profile/repository/review/head and bounds; Link values are validated and
never fetched as network authority. Each invocation makes one bounded attempt;
rate-limit advice remains a closed failure with a bounded retry hint. The caller
owns any subsequent explicit retry or continuation, under its original budget.
The shared process host bounds deadlines, cooperative cancellation and process
tree cleanup. GitHub review-approval aggregation and legacy commit-status
aggregation are not implemented; review/check summary fields remain absent rather
than inferring a complete result from one check page.

`HostedIssueCodeReviewObservations` implements the existing application observation
port. Its explicit profiles reuse `ManagedIntegrationAccess` with
`IntegrationPurpose::RepositoryRead`, exact installation/configuration/native
repository/executable pins and the accepted management security partition. Current
managed grants are checked before and after the provider read under one original
deadline. Observation time is captured before dispatch. Metadata visibility is
local and separate from protected reads, so revocation can block refresh while
retaining already visible history. Optional local Repository and exact frozen
publication-policy mappings prove lineage correspondence without provider calls;
missing mappings keep that provenance unavailable. No mapping grants forge writes.

`ReadIssueCodeReviewChecks` exposes one original-head page to installed consumers.
Explicit review attach/refresh can retain that bounded page as normalized external
CI in the existing atomic review observation journal; it is not Factory evidence
and does not synthesize acceptance or required-check policy. The [Issue review HTTP
contracts](../issues.md#review-attachment-and-retained-http-observations) compose the
existing retained observation port. [Console review cards](../issues.md#console-review-cards-and-explicit-attachment)
use those retained reads and explicit commands. The [typed Checks inspection](../issues.md#subject-bound-checks-and-artifacts-http-inspection)
reads retained original-head facts without provider dispatch; its
[Console pane](../issues.md#console-checks-and-artifacts) uses the same bounded
observations and preserves historical heads. Construction,
server startup and retained-data queries do not create a poller, provider call,
Factory target, Work or Run. Manual tasks require none of these optional profiles.

Qualification covers the native GitHub adapter against a controlled HTTPS peer
and its real JSONL process, plus a fake MR vocabulary through the same protocol,
process host and managed-grant reader. It is not live GitHub qualification and
makes no native GitLab/Jira support claim. See the
[6.11 verification record](../../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-6.11.md).
The provider endpoints follow GitHub's
[PR read](https://docs.github.com/en/rest/pulls/pulls#get-a-pull-request) and
[commit check-runs read](https://docs.github.com/en/rest/checks/runs#list-check-runs-for-a-git-reference)
contracts.
