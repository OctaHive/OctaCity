# Single GitHub provider plugin

`octacity-github` owns GitHub provider translation. Delivery, read-only Work
Source listing and Work Reporter effects share this package, installation and
SDK transport. Their protocols and
credential classes remain independent; automatic intake and a concrete reporter
are being implemented by the `add-github-work-source` change.

The plugin uses [Octocrab](https://github.com/XAMPPRocky/octocrab) 0.54.3 over the
existing reqwest transport, with default SDK features disabled. Automatic SDK
retry, redirect, tracing and JWT features are disabled. Raw SDK responses are
bounded before decoding. Scope, candidate verification and observe-before-retry
remain OctaCity responsibilities rather than SDK assurances.

## Issue selector configuration

`issue_selector::IssueSelector` validates typed definitions through `new` or
bounded JSON through `from_config`, and evaluates observed Issue state/labels
locally through `matches`. A selector document has four required fields:

```json
{
  "state": "open",
  "all_of": ["ready-for-review"],
  "any_of": ["documentation", "enhancement"],
  "none_of": ["on hold"]
}
```

State is explicitly `open`, `closed` or `all`. Every `all_of` label must be
present; a nonempty `any_of` needs at least one observed alternative; no
`none_of` label may be present. Three empty arrays remove the label restriction,
including for unlabeled Issues. State filtering still applies. Names are whole
literal labels, including spaces, Unicode and punctuation; there is no regex,
search-query language or built-in approval/classification vocabulary.

Local safety limits are 32 entries per predicate, 256 UTF-8 bytes per label
and 64 KiB for the raw JSON document. These are adapter bounds, independent
of GitHub service quotas. Empty/whitespace-only names, control characters,
duplicates, required/excluded overlap and an alternatives set completely covered
by exclusions are rejected. Partly excluded alternatives and overlap between
required labels and alternatives remain valid. Unknown/missing fields,
unsupported states and malformed documents are rejected by the checked decoder.
Errors expose only a closed field/reason classification and never echo input.

Local name comparisons use Unicode lowercase forms for matching, duplicate
detection and contradiction checks while retaining configured spelling in
serialized definitions. GitHub documents its label-name parameter as
[case insensitive](https://docs.github.com/en/rest/issues/labels#remove-a-label-from-an-issue).
The helper does not perform transliteration, whitespace trimming or regex
interpretation of label names.

This module performs configuration validation and pure matching. Source/Reporter
process contracts are defined in the [Work integration protocol](../../../docs/reference/protocols/work-source-reporter-v1.md).
The same executable recognizes their independent families and cancellation,
validates installed Issue grants and returns normalized Source pages. Reporter
effects require a separately enabled Reporter repository, write grant and pinned mapping.
Installed configuration publication and worker activation have their own
implementation tasks. The provider-neutral `WorkSourceInput` holds immutable configuration
references; future GitLab/Jira adapters own their selector and state semantics
without introducing GitHub enums into the Flow interpreter.

## Installation and protected material

### Independent Issue scopes

Install a private `github-work.toml` beside the same provider executable for
Source/Reporter. It shares the API/TLS and private-grant validation helpers with
Delivery, but needs no Delivery configuration, Git executable or Artifact inputs:

```toml
config_version = 1
api_base = "https://api.github.com/"
api_version = "2026-03-10"
credential_directory = "credentials"
selection_directory = "selections"
mapping_directory = "mappings"

[capabilities]
source_repositories = ["project-tickets"]
reporter_repositories = ["project-tickets"]

[repositories."project-tickets"]
repository_id = "<native-github-repository-id>"
owner = "<github-owner>"
name = "<github-repository>"
```

Each role's repository list defaults to empty; installing one role grants
nothing to the other. Lists must contain unique installed aliases. At most 128
repositories/aliases are accepted, and the whole strict configuration is limited
to 64 KiB. `repository_id` here identifies the native ticket container, independent
of OctaCity's code Repository. Requests must match both its alias and native id.
The actual API host must fit the request's exact host ceiling before grant access.

Use separate opaque handles for `work_source_read` and `work_reporter_write`.
Each resolves only to a private `credentials/<sha256(handle)>.json` file:

```json
{"class":"work_source_read","token":"<operator-installed-token>","repositories":["project-tickets"]}
```

The Reporter file uses `work_reporter_write`. These classes cannot borrow each
other's grants, or Delivery/repository-read/Job grants. Token content is bounded
to 8192 bytes without control characters; the strict grant file is at most
64 KiB and its repository list at most 128 entries. Read/write authority at GitHub
must also be scoped appropriately when the operator provisions the tokens.

The Work profile digest is the SHA-256 of exact `github-work.toml` bytes and is
pinned independently from Delivery. Both Work roles use this file's digest;
their capability enablement and credential classes remain separate. Configuration
and private directory permissions are rechecked on every preparation, including
confinement of a nested credential directory. Changed configuration returns
`conflict`; an unreadable, nonprivate or invalid installation/grant returns
`permanent`. Missing role/installation returns `unsupported`; substituted native
identity or an insufficient host ceiling returns `invalid_request`. All failures
use closed classifications without paths, token values or raw diagnostics.

Credential contents are excluded from the digest so token rotation keeps the
profile pin valid. Optional `api_root_certificates_pem` has the same private-CA
bounds described below and is included in the Work digest. No network request
is made by `GithubWorkProvider::prepare`. Cancellation requires no Work grant.
Reporter outbox composition is an explicit trusted runtime installation;
ordinary manual Factory intake remains independent of these installations.

### Read-only connection checks

Source and Reporter advertise `check = true` independently in their verified
`adapter.toml` capability sections. The check validates the expected API endpoint,
pinned private profile, native resource and network ceiling before resolving the
selected role's private credential. It reads repository metadata to authenticate
and confirm the original native repository id. Source then performs one bounded
Issue-list GET (`state=all&per_page=1`); no task snapshot or cursor is retained.
Reporter performs only the metadata GET and leaves write capability Unverified,
even when returned repository permissions include push/admin. No mutation or
fallback to another credential is attempted. HTTP denial, throttling/retry advice
and unavailability remain separate safe readiness facts.

Trusted server composition supplies `IntegrationCheckProfile` and
`HostedIntegrationConnectionChecker` to the application check handler and protected
REST builder's `with_checks`. POST
`/api/v1/integrations/{integration_id}/checks` accepts only the reviewed metadata
version, resource reference and purpose. It requires separate connection
administration, view and execute rights; it may check a configured inactive
connection without enabling it or granting Project use. The response contains
native resource choices and readiness, never credentials. Manual Factory intake
and future manually created internal Issues need no connection check.

### Versioned status mappings

`status_mapping::StatusMapping::from_config` validates adapter-owned JSON without
credentials or provider calls. A private `mappings/<sha256(mapping.identity)>.json`
file is selected by an opaque installed identity and pinned by SHA-256 of its
exact bytes in the existing Reporter `mapping` reference. Replacing that file
cannot redirect an accepted intent; its digest must match on every invocation.

```json
{
  "config_version": 1,
  "managed_label_groups": {
    "editorial": ["ready / review", "needs context"],
    "category": ["documentation"]
  },
  "statuses": {
    "needs-human-eyes": {
      "add": ["ready / review", "documentation"],
      "remove": ["needs context"]
    },
    "archive": {"add": [], "remove": [], "state": "closed"},
    "resume": {"add": [], "remove": [], "state": "open"}
  }
}
```

Every name is editable. Exact status lookup has no approval/classification
vocabulary. Omitted/null state preserves the current state; only explicit `open`
or `closed` changes it. Groups declare disjoint ownership, not automatic
exclusivity: only the selected status's explicit additions/removals are applied.
State-only mappings can use an empty group map without inventing managed labels.
Labels within an effect must belong to a managed group and cannot be repeated
or occur on both sides, using the same Unicode lowercase comparison as selectors.
Unknown fields/versions/states, duplicate group/status keys, overlapping groups,
blank/control/oversized names and unmanaged labels are rejected. Literal `.` and
`..` labels are unsupported because URL dot-segment normalization cannot safely
address their individual removal endpoints.

Local bounds are 64 KiB per document, 32 groups, 64 labels per group, 128 managed
labels overall, 128 statuses, 64 additions/removals per status and 256 UTF-8 bytes
per name/value. Installed mapping files/directories use the same private-file,
regular-file and confinement checks as selectors. Source-only installations
need no mapping directory.

`observe` and `project` require the independent Issue-write grant and verify the
native repository id, stable Issue id and human-facing number. Pull requests are
rejected. Before any write, every label mentioned by the selected effect is
checked against the repository; missing labels require operator repair, never
automatic creation. The adapter uses GitHub's scoped
[add/remove label endpoints](https://docs.github.com/en/rest/issues/labels)
and optional [Issue state update](https://docs.github.com/en/rest/issues/issues#update-an-issue),
preserving unrelated labels. It reads at most 100 current Issue labels and
rejects incomplete/paginated observations without following provider URLs.
Projection skips already satisfied effects and rereads the Issue and its labels
before returning an exact `applied` receipt; a mismatch returns `conflict`.
Observation is read-only and returns whether the pinned effects currently match.
Transport retains bounded bodies, configured API/TLS trust, no redirects or SDK
retries, closed failures and rate-limit hints. Host timeout/cancellation bounds
the original operation. Multi-request writes are not atomic.

Tasks 4.1–4.5 supply adapter validation/native effects, recovery qualification and optional composition of
accepted `set_status` actions through the existing outbox. The provider-neutral
`HostedFactoryWorkStatusProjector` resolves the original Work source and exact
`FactoryReporterProfile` binding, verifies accepted action/claim and permission
ceilings, then inspects/applies through Reporter v2 with the original operation
identity. The trusted runtime installer can use
`FactoryRuntimeResources::work_reporter_dispatcher`; construction performs no
provider or credential I/O. Retries retain their original action deadline and
finite attempt policy, honoring provider retry delays as lower bounds.
Controlled HTTPS peers verify response loss after each removal/addition/state
update, partial convergence and a fresh native process recovering the same intent.
A lost, malformed or oversized successful write body, or invalid final read-back,
leaves confirmation retryable under the original outbox policy. Every retry first
observes; completed effects are skipped and unrelated labels remain untouched.
Missing or invalid Project process responses are likewise uncertain, while an
invalid Observe response cannot authorize a write. Exhaustion records failed
reporting without undoing external effects or accepted Factory history.
Reporter v2 advertises `max = 2` in `[protocol.reporter]`; Source remains v1.
The hosted Factory bridge requires v2 without downgrade. Read-only `inspect`
returns opaque managed fields and conditional steps; `apply` rechecks the original
intent and current managed snapshot, then performs at most one DELETE, POST or
PATCH and rereads the effects. It rejects consumed/drifted step tokens. Unrelated
human labels do not invalidate the conditional snapshot and are never replaced.
All members of an affected managed group must resolve to positive unique native
repository label ids before any write. Each v2 invocation reads one bounded
100-label repository catalog page and resolves selected labels absent from that
page through exact-name lookups. It neither follows catalog pagination nor
retains a cache across invocations, so fresh Issue reads and conditional checks
remain unchanged. A four-invocation inspect/inspect/apply/inspect cycle for one
change in a 64-label group uses 19 HTTP requests in the controlled peer.
Field identities include both native label
ids and normalized name aliases, preserving overlap across rename or same-name
recreation. Every field also includes the canonical API base, native repository
id and stable Issue id, independent of installation, credentials and mapping
versions. Issue state is a separate field. Native ids/name drift in the managed
Issue snapshot fails closed. These details remain private to this adapter.

Durable per-item acceptance order, field ownership and unresolved write permits
are shared across input bindings and installations by the Factory store.
Confirmed typed rejections release their write permit; unknown remote completion
retains its hazard. Every step is fenced;
newest-state reconciliation handles known late old writes within original bounds.
Unknown completion after expiry is an explicit conflict, not proof of remote
quiescence. No remote compare-and-swap is claimed. Stage-local contracts qualify distinct
configured success/escalation/failure effects before overall pipeline completion,
nested-stage provenance and retry reuse. The native executable qualifies different
arbitrary values within one pinned mapping, including removal-only effects and
optional state change while preserving unrelated labels. A stage with no configured
action writes nothing. Integration administration and management/Console authoring
retain their subsequent tasks.
Factory and the Work wire protocol retain opaque provider identities and arbitrary
statuses, so GitLab, Jira and other adapters can own their native effects without
adding GitHub labels or Issue-state enums to the shared execution model. Manual
Factory admission remains available without a Source or Reporter installation.

### Read-only Issue pages

`selection_directory` is optional; an installation without it retains explicit
`unsupported` Source scans. When present it must be a private confined directory.
An opaque selector identity resolves to
`selections/<sha256(identity)>.json`, a regular private file at most 64 KiB,
using the selector JSON schema above. Each scan pins the SHA-256 of its exact
bytes in `selection.sha256`; selector replacement requires repinning. Unsafe or
malformed selector files fail before HTTP, and digest mismatch is a conflict.
The directory itself is part of the installed Work profile.

`GithubWorkProvider::execute` and the actual process support one read-only page
per Source scan. They verify the repository's native id through the repository
endpoint, then use the
[repository Issues API](https://docs.github.com/en/rest/issues/issues#list-repository-issues)
with `state=all`, `sort=updated`, `direction=asc` and the requested page size
(1–100). Every returned Issue is checked against the local all/any/none/state
selector. Records carrying `pull_request` are excluded. Stable `id` is the item
identity; `number` is display context and `updated_at` is observation provenance.
Title, optional body, native labels and state stay private normalized data.
No comment, attachment or linked resource is fetched.

An optional inclusive millisecond update window is enforced locally. The API
`since` value starts one whole second before a positive lower bound to include equal
timestamps at its exclusive boundary. An initial window starting at Unix epoch
omits `since`: GitHub can return an empty page for an epoch/pre-epoch sentinel
even when existing Issues are present. The original inclusive upper bound is
still enforced locally. The server's provider-neutral polling
owner retains pages and overlapping/reconciliation contexts under the existing
scan fence; see [bounded polling](../../../docs/architecture/factory-extension-boundaries.md#bounded-source-polling).
The optional trusted Source worker composes this protocol with ordinary admission. A
paginated API does not provide a snapshot of concurrent GitHub edits.

Each raw JSON body is bounded to 4 MiB by both declared Content-Length and
streamed bytes before decoding. Candidate and complete normalized response
bounds can be narrowed by the request. Malformed data, invalid identities,
duplicate candidates and exceeded bounds produce a classified failure with no
partial page or silent truncation. Transport/provider failures are distinct
from a successful empty page. Denied/missing resources require operator repair;
rate limits and server outages are transient. The SDK performs no implicit retry.

Pagination uses a provider-owned opaque page cursor bound to the original scan,
profile, resource, selector, window and page size. A `Link` continuation must have
the exact installed HTTPS origin (including port), repository API path and query,
changing only to the immediate next page. Link headers are bounded to 8 KiB.
Extra parameters, credentials, fragments,
foreign origins and duplicate/looping continuations are rejected. The next URL
is constructed locally; neither the caller cursor nor response links become
network authority. Redirects are disabled. API/TLS trust and the explicit API
version use the same pinned SDK transport as Delivery.

Cancellation stops an in-flight read and preserves the original scan correlation.
Only a matching Source cancellation can stop that scan; another role/operation
is a protocol fault. The verified host retains absolute deadlines and process
tree cleanup. No Source operation admits Work, updates an Issue or starts a worker.

### Delivery scope

Install the binary with `adapter.toml` under the common operator adapter registry.
`octacity-server-delivery` validates the executable digest, Delivery version,
capabilities, `delivery_write` class and immutable configuration digest before
passing any handle. The common host clears the environment, bounds JSONL and
stdout/stderr, and terminates the process tree on timeout/cancellation. The
composition bridge also bounds calls by the current observer lease and original
attempt deadline. Cancellation addresses the active original operation before
acknowledging its stop.

The installed directory contains a private `github-plugin.toml`:

```toml
config_version = 1
api_base = "https://api.github.com/"
api_version = "2026-03-10"
git_path = "/absolute/operator-selected/git"
git_sha256 = "<verified-executable-sha256>"
credential_directory = "credentials"
input_directory = "inputs"

[repositories."repository:project-repository"]
repository_id = "<internal-repository-id>"
owner = "<github-owner>"
name = "<github-repository>"
remote = "https://github.com/<github-owner>/<github-repository>.git"
```

Optional `api_root_certificates_pem` adds at most eight individual PEM
certificates, at most 16 KiB each, to system trust for the HTTPS API client.
Hostname verification remains enabled. This setting is part of the immutable
configuration digest; changing it requires repinning the provider configuration.
It does not change Git's separate TLS trust or executable configuration.

The HTTPS tests load this operator configuration through `GithubPlugin::load`
and observe a real local TLS peer without mutating the private transport.
Untrusted certificates, hostname mismatch, malformed or oversized trust entries,
and stale configuration digests fail before an HTTP request is accepted.

Directories must be private (0700 on Unix); configuration/protected files must be
regular private files (0600). Caller-selected filesystem paths are never opened.
A handle selects `credentials/<sha256(handle)>.json` containing `class`, `token`
and permitted repository aliases. Only a stored `delivery_write` grant can
satisfy Delivery: read, repository Job and reporting grants cannot be reclassified
by changing a request. Tokens stay inside this trusted process, are zeroed in
owned buffers, and never appear in arguments, responses or diagnostics.

Artifact handles select `inputs/<sha256(handle)>.bin`. The deployment materializer
verifies Artifact ownership and atomically stages the exact accepted bundle and
canonical capture manifest privately. The adapter checks size and digest again.
`FactoryDeliveryMaterializer` is the explicit deployment integration seam, like
the existing ordinary Build compiler; it receives no forge token. Live deployment
materialization/qualification belongs to the separately planned installation work.

The SHA-256 of exact configuration bytes is advertised and frozen in each request.
Credential-file contents are excluded, allowing token rotation. Repository,
origin, executable or configuration changes cannot redirect an accepted request.
The server freezes the full target policy as `FactoryDeliveryPolicy::reference()`
and checks Project/Repository, plugin, network and secret-profile ceilings before
materialization or spawn. The frozen request carries the permitted exact hosts;
the provider compares its actual API and Git remote hosts with that ceiling before
reading a token or doing network activity. Both `api.github.com` and `github.com`
are required for the example installation above.

The reusable target policy declares `base_reference` and
`candidate_reference_prefix`. The server appends the stable operation digest to
the configured prefix to freeze one exact branch per logical publication. Different
Work operations use distinct branches under the same installed policy; replay
keeps the original branch. No Flow or node name is used to construct it.

## Publication and replay

Observe checks the candidate branch and matching review identity. Multiple
reviews, a changed head or another delivery marker are conflicts. New/open
publication additionally requires the original base to remain current. An
externally merged review may have an advanced base: its accepted candidate,
repository, refs and marker must still match, and its observed review base is
retained separately from the original accepted base. An unqualified paginated
list is rejected. An exact branch without a PR is a repairable partial publication.
An owned open PR with different title/body returns `repair_required`; the common
host sends that state through publication so it cannot silently skip repair.

Publish imports the retained bundle into a private ephemeral bare repository and
checks its canonical manifest, candidate, direct predecessor and original base
ancestry. It runs no checkout, repository hook, helper or user Git configuration.
An empty explicit push lease cannot overwrite a remote head. The adapter observes
again before creating a bounded PR. It repairs existing owned metadata under
the same review identity, then observes again. Merging is absent from the protocol;
an externally merged review is only an observation. Closed/conflicting reviews
cannot qualify publication.

Network Git failures, including lost push acknowledgments, are transient failures;
local integrity/ancestry checks remain conflicts. Replay observes the same stable delivery
identity before another write; it cannot choose another candidate, silently rebase
or create another PR. Accepted common node records retain branch identity,
verified remote head, review identity/state and receipt digest. These observations
never authorize a transition: configured gates, human responses and promotion
policies remain authoritative.

## Verification boundaries

Protocol tests use canonical fixtures and scope-substitution cases. Provider tests
use real Git capture/bundles/remotes and a controlled HTTP forge boundary; installed
configuration still requires HTTPS. Process tests invoke the actual binary, and
host tests exercise common verified process mechanics. Live HTTPS GitHub account
qualification is separate and never runs against a user's repository in this suite.

Task 3.5 composes Source through the provider-neutral runtime extension; it does
not enable discovery by default. Worker installations require Source
`authorize = true` in the verified adapter manifest. This read-only operation
rechecks the current private configuration, native resource, read grant class and
allowed resource aliases without listing Issues or moving a cursor. Remote token
revocation is also detected by the next API read; local grant removal/class/scope
changes deny cached admission and replay immediately.

Exclusive groups supply the full set of pinned selector references. Each Issue
is evaluated once against every selector; the response includes exact matching
references and no Factory target. The opaque cursor binds all selectors.

Rate limits retain `Retry-After` seconds and, for an exhausted primary quota,
`x-ratelimit-reset` as a lower bound (the later bound wins). Missing hints wait at
least one minute. Invalid hints or delays beyond the one-hour wire bound require
repair rather than an early retry. The runtime retains these bounds across restarts
and applies its own finite backoff/attempt policy independently of other inputs.
