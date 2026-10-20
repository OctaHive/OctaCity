# Internal Issue application contracts

An Issue is a persistent Project-owned task. It can exist as a manual draft
without a provider, credentials, repository, Factory configuration or execution.
Its business status does not establish a Factory Decision or execution permission.

## Canonical primary source association

`IssueSourceHandlers` provides optional application commands over the existing
`WorkSourceObservations` read capability and the narrow `IssueSourceStore` port.
Manual Issue handlers retain no Source dependency. The memory and PostgreSQL
adapters share their existing Project lock, ordinary Issue creation policy,
initial workflow and atomic revision/replay/audit publisher. Optional bounded
import/refresh uses the same installed observation capability and Source progress
machinery described below.

`IssueSourceSelection` pins a Project, installed Source binding and verified
native resource/item. Its canonical key reuses `source_native_issue`,
`source_security_partition` and `source_native_correlation`. Only the accepted
Project/security partition, installation identity, native resource and native
item participate. Installation/configuration versions, pipeline inputs,
connection display names, credentials, native display keys, task text and URLs
do not determine canonical identity. Renaming or rotating a compatible installed
connection cannot create another task for that native key.

`ImportIssueSourceCommand` consumes one bounded authorized read and atomically
resolves or creates the canonical Issue. Initial text is a bounded ordinary
draft, with no fabricated acceptance or execution target. Competing imports
resolve one Issue; a different native key or a manual task with identical text
remains separate. An existing link is returned without refreshing requirements,
changing status or admitting Work. Each accepted import key retains its original
selection and receipt, including when it resolves an already existing link.
Initial status history identifies Source origin while using the ordinary
workflow initial status, with no native-state-derived transition.

`AttachIssueSourceCommand` uses separate Issue `Administer` authority, current
Issue visibility, an exact Issue version and a review of the existing field
policy. The existing Source reader owns current installed resource/read grants;
the application checks the returned Project, partition, binding, native identity
and observation lifetime before publication. The shared clock is read again
after asynchronous access; an expired or future observation cannot publish a
link. An Issue has at most one primary
link, and a native key belongs to at most one Issue in that Project/partition.
Duplicate attachment returns a finite conflict with the canonical association
only when visible; it never merges histories. Exact replay retains its original
link and time after later Issue edits. Reusing a key with a changed selection,
or submitting stale review, conflicts.

Attachment preserves text, workflow, status, field ownership and execution
history. The retained link starts with pull enabled for explicitly authorized
refresh, but attachment starts no polling and activates no field ownership,
push synchronization or retroactive reporting. Supported mappings and their
activation have a separate reviewed contract.
`GetIssueSourceLinkQuery` reads retained metadata under Issue visibility and the
accepted partition, with no provider call or read-time creation. Hidden canonical
imports are denied before replay/audit publication. Denied reads and conflicts
leave manual tasks and their ordinary create/edit contracts usable.

PostgreSQL retains primary links and bounded `IssueSourceObservation` receipts
in additive tables. A receipt pins the exact installed Source/configuration
references, verified plugin, transport/selector/snapshot digests and original
read/deadline times. It contains no native body or credentials. Each distinct
accepted observation remains independently addressable in its original
Project/security partition; replay neither replaces it nor renews its deadline.
These configuration pins retain read provenance, not authority to activate
field ownership or an external status mapping.

The ordinary `associate_issue_work` transaction checks settled, verified Source
preparation provenance against the retained canonical link and the accepted
Work security partition. A Work from another native task cannot be attached to
that Issue. Separate inputs may attach their independently admitted original
Work/Run pairs to one canonical Issue. Association retries reuse the shared
replay/audit/outbox transaction after rollback, connection loss or a lost reply;
they do not admit another Work or Run. Source preparation recovery still uses
its existing journal. Legacy Source Artifact provenance is retained without
inventing an Issue requirements revision. Manual Work association keeps its
ordinary contract when no verified Source preparation exists.

## Bounded Source import and refresh

`IssueSourceReadHandlers` runs an explicitly authorized finite read for a Project
and exact installed Source profile, optionally targeting one retained Issue
link. It needs no Factory target, repository or admission/Reporter port.
Construction starts no worker. Existing enabled execution inputs retain their
independent admission semantics; an Issue import, refresh or pull echo starts
no execution.

The consumer and ordinary Factory intake share one bounded polling driver and
the existing scan lease/fence, page/window/reconciliation progress and retry
schedule model. PostgreSQL retains its separately scoped private journal.
Restart resumes original unprocessed observations and opaque continuations;
page/item/byte limits still apply. Page expiry cannot exceed the earliest
original observation deadline. Expired private tails are discarded before a
fresh read, without renewing old provenance. The installed read grant is
rechecked before processing a retained item without listing another page.
Project/Issue visibility and current mutation rights remain independent.

`IssueSourceRefreshState` retains an accepted receipt and at most one pending
changed receipt. Opaque native update identities are not sortable revisions.
A changed snapshot remains `uncertain` until a later independent read under
the same installed binding confirms it. Under the same reviewed mapping,
duplicate, unchanged and stale reads create no Issue revision; unchanged reads
can advance the safe watermark. A newly published mapping can reconcile an
unchanged snapshot from a new independent read strictly after publication.
Only reconciled authorized observations replace declared source-owned title or
description through the ordinary requirements/applicability policy. Acceptance,
planning and frozen execution inputs stay under their existing owners. Explicit
status translation uses the shared transition contract described below.

`SetIssueSourcePullCommand` changes explicit link pull enablement under an exact
monotonic generation and scoped replay. Disabling pull stops that retained-link
read; reenabling cannot revive an old owner or its retained page. Mapping
publication also advances this generation without changing pull enablement.
A separately installed broader read profile may refresh the same native link
when the task no longer matches an execution selector. A published mapping pins
its exact read profile: another profile retains a replayable
`read_profile_mismatch` skip without advancing accepted or pending freshness.
The remaining discovery page can continue after restart. Disabling an execution
input does not disable link refresh. An independently authorized discovery pass may encounter
a paused link and retain its import observation, but does not refresh its fields.
If pull is disabled after discovery's preliminary check, publication retains a
replayable `pull_disabled` result without advancing accepted or pending freshness.
The remaining discovery page stays runnable, including after restart; reenabling
pull does not turn replay of that skipped operation into a refresh.
Rate limits, bounded retry exhaustion, disabled access and provider failures
retain local task/history and do not block unrelated manual tasks. The HTTP and
Console controls expose explicit discovery, attachment, refresh, pull enablement
and separate ownership/status mapping review, as described below.

## Identity and revisions

`CreateIssueCommand` accepts a Project and `IssueContent`: title, description,
optional acceptance text and optional byte-sized planning priority. A title is
canonical nonblank single-line text, bounded to 128 UTF-8 bytes. Description and
acceptance are each bounded to 65,536 UTF-8 bytes and reject NUL. Empty description
and incomplete or absent acceptance are valid drafts. These are requirements,
not a prompt/command language. Execution validates its own required inputs later.

Creation atomically allocates an opaque `IssueId` and a positive Project-local
number displayed as `ISS-1`, `ISS-2`, and so on. The display key is scoped by its
Project; it does not identify or authorize a native provider task. A failed or
replayed creation does not consume another number. The Issue pins the Project's
selected exact workflow version and its initial status at acceptance. Subsequent
workflow publication/default selection does not repin that Issue.

`EditIssueCommand` replaces only the editable content under an exact
`IssueVersion`. `GetIssueQuery` reads either the current head or an exact retained
revision within the owning Project. Edits preserve identity, display key,
creation time, workflow and status. Closed input documents reject status,
workflow, actor, source ownership and execution setters. Prior revisions remain
immutable and exact replay returns the original receipt after later edits.
Requirements and organizational reasons are omitted from generic `Debug`
diagnostics, audit facts and outbox notifications.

## Explicit business moves

`TransitionIssueCommand` uses a separate authorization action and an exact Issue
revision, workflow ID/version, starting status and declared transition ID. The
transaction checks that direct edge in the pinned policy, its manual origin,
current field ownership and evidence requirements. It does not infer a path,
interpret names or select the latest workflow. Manual commands carry no trusted
execution evidence, so evidence-required edges fail closed; no client-supplied
boolean can assert acceptance or passing checks.

Creation uses `Create` on the Project-owned `Issue` collection and checks visible
Project scope. Content edits use `Update` on an Issue, business moves use separate
`Publish`, and dedicated reads use `View`. These grant no workflow `Administer` or
execution/control authority.

A successful move publishes one revision and one `IssueStatusHistoryEntry`
containing before/after status, workflow pin, transition, optional reason bounded to 128 UTF-8 bytes,
actual management actor and original time. Creation records initial placement
with no preceding status or edge. Text edits do not append a status event.
`ListIssueStatusHistoryQuery` reads at most 200 records in revision order, with
an exclusive revision continuation. Current/retained/history reads cause no write.

Create, edit and transition replay are partitioned by operation and accepted
management security scope. Changed intent conflicts. Exact replay returns the
original revision/history even after later moves, under current authorization
and visibility. Stale Issue/workflow/status preconditions, foreign targets,
forbidden origins/edges, source-owned status and unavailable evidence publish
no partial mutation. Organizational movement never starts, retries or cancels
Work/Run, calls a provider, completes a gate or grants delivery authority.

## Ownership and retained projection intent

`PublishIssueSourceMappingCommand` is an optional application contract over
`IssueSourceMappingStore`, requiring Issue administration, current visibility
and an authorized installed Source reader. Each primary link has immutable
mapping versions separate from Issue revisions and workflows. Publication
reviews both the exact Issue version and the previous mapping reference (or
its absence), and atomically installs the derived `IssueFieldPolicy` with
mapping/replay/audit/outbox state. It edits no task content. Historical mappings
and original operation receipts survive replacement and restart.

Each field has one authority: source-owned pull, OctaCity-owned push or
unsynchronized; omitted fields are unsynchronized. Title and description support
pull, and status supports pull or explicitly enabled push. Other managed
directions are rejected. Status mapping separately pins the exact workflow and
stable status IDs. Its predicates use bounded literal native states and labels
advertised by the installed capability; native names never select a workflow
or imply an open/closed business status. Push uses the existing finite Reporter
state/label effect contract. Enabling it additionally requires current
connection visibility/use and an independent `TaskUpdate` resource grant;
`TaskRead` alone cannot activate writes.

Source-owned status blocks manual and pipeline moves. An eligible observation
must match one distinct internal target and a permitted direct Source edge.
The ordinary transition policy enforces ownership, workflow, evidence and
applicability rules, with no invented intermediate moves or execution evidence.
Unmapped, ambiguous, workflow-incompatible, forbidden, evidence-required and
not-fresh observations retain explicit conditions without a status move. An
echo creates no revision or status event. OctaCity-owned status treats native
divergence as data. An applied move retains exact mapping, observation and
native-correlation references in ordinary status history. Text and status from
one observation share one atomic Issue revision and history publication.

`IssueFieldPolicy` is retained trusted association state, separate from ordinary
create/edit/move input. Source-owned fields cannot be replaced locally; unchanged
source-owned values do not block unrelated edits. An approved push policy pins
an immutable mapping and the locally owned fields allowed for projection.
Source ownership and push authority cannot overlap.

When an authorized edit or move changes a selected push field, the same
transaction retains an `IssuePushIntent` identifying the original Project,
Issue revision, exact mapping and changed fields. It contains no credentials or
provider request. A later mapping replacement cannot retarget this intent and
replay cannot create another one. Unlinked manual tasks retain no push intent.
Accepted push intents now retain their original `IssueSourceDesiredState` in the
ordinary atomic mutation publication: primary link, exact writer, workflow/status,
configured finite Reporter effects, scoped command key, actual actor and optional
Work/Run/Flow/cycle/attempt/action provenance. The mapping pin and Issue revision
remain on the enclosing intent. An unmapped status retains absent effects rather
than inventing a native state. Unsupported remote metadata fields stay unavailable.
Old intents without this material are not retrofitted from later attachment or
mapping state. Comments, model prose, link existence, mapping publication and pull
echoes create no new outbound status action.

Configured Issue actions retain their push mapping review in the existing sealed
command record, including an explicitly absent mapping for local-only actions.
Legacy commands without a push review remain local, including pending replay after
later attachment. The shared transition policy rejects a changed mapping before a
fresh move; completed command replay still returns its original outcome. Direct Factory
source effects retain their original Source admission/item and cannot derive a
target from a later Issue link. Where that same original native item is known,
a consecutive configured status-action chain rejects overlapping card/direct
label or native-state producers. Inherited input effects use the same finite
mapping; an opaque legacy effect scope requires a single producer when the card
has managed effects. Separate managed fields remain allowed. Native resource
aliases do not bypass the same-item check.

Issue delivery reuses the existing Reporter worker, append-only outbox and native
ordering journal. Outbox ownership identifies either a Run or an accepted Issue
revision; a manual task needs no synthetic Work, Run or Factory target. Original
desired material stays in the atomic mutation event. The worker claims it under
the existing bounded fence and uses the same Inspect/Apply protocol, field-group
ordering, observation generations and late-write hazards as direct Factory
effects. It rechecks accepted material and live ownership before each native
write. An unknown response is recovered by observing the same original intent;
mapping replacement cannot change its native item, writer or effects.
The verified Source observation and link retain an optional bounded `display_id`
for the existing Reporter route contract. It remains separate from stable native
identity; old links without that original locator grant no guessed provider call.

Explicit installed `FactoryReporterProfileDraft.issue_delivery` supplies the
existing finite retry policy, per-dispatch budget and a total window measured
from original acceptance. These settings participate in the immutable writer
profile binding. Claims, retries and restart cannot renew the original window.
Work-only projectors leave Issue events untouched. Temporary failure to read
accepted material leaves a recoverable claim; provider failure cannot undo the
Issue transition or accepted Factory result. Profiles carrying
`ManagedIntegrationAccess` retain its current TaskUpdate checks before each
process invocation; a Source read grant does not supply writer credentials.

Migration 0085 resumes original pre-5.2 desired material in bounded atomic batches
without reconstructing legacy intents from current links or mappings. Repeated
migration is idempotent. Readiness also detects an unfinished publication phase;
Issue claims wait until all retained inputs are published. Migration 0086 retains
one recovery cursor and completion marker, committed with each batch. Completed
recovery checks read that marker; ordinary dispatch does not rescan publication
history. Original intent reads use the existing desired-publication index.
When a newer Issue
effect or a different producer already occupies the native bucket, or another
Issue link makes its ownership ambiguous, recovery retains the original input
without an ordering permit and dispatch fails closed. New commands publish their
shared order atomically with the accepted revision.

Reporter terminal rows retain safe conflict, forbidden, unsupported or exhausted
reasons independently of the Issue status and accepted execution results. A
conditional-write conflict and an unsupported operation are terminal; a temporary
dependency failure retries only within the original installed attempt/time bounds.
Legacy generic failures remain readable. No provider diagnostics or credentials
are stored in the reason. Managed connection revocation or disablement is checked
before each process invocation, including observation before a retry.
New managed mapping publications retain the original writer connection/version
and configuration alias privately with accepted intents. Delivery resolves that
original authority through the existing connection resolver or an exactly matching
managed profile; an identical legacy binding cannot bypass current grant checks.
This private authority is excluded from public mapping/choice documents.

Explicit resolution publishes a new reviewed mapping version and one new desired
intent through the same atomic publication and Reporter ordering path. It requires
the current Issue version, current mapping pin and exact terminal outbox receipt
of the failed operation in the visible original native-link/security scope. The
command rechecks independent installed Source/writer grants and supported effects.
Its intent links back to the failed operation; the old input and terminal receipt
remain unchanged. Resolution neither adds a status-history action nor changes the
Issue revision, requirements, business status or accepted execution. Concurrent or
stale reviews conflict, and scoped replay returns the original policy/effect.
The new effect still uses conditional Inspect/Apply, fencing and finite installed
bounds. There is no unconditional overwrite or reopen-old-operation control.

`POST /api/v1/projects/{project_id}/issues/{issue_id}/synchronization/resolve`
uses Issue administration, `If-Match` for the current Issue revision and an
idempotency key. Its closed body contains `mapping` (the ordinary reviewed mapping
request) and `review` (`operation_id`, `expected_record`). The no-store response
contains the new mapping/reference and `projection` operation identity; publication
returns 201 and exact replay 200. Bodies cannot supply credentials, endpoints,
execution instructions or a force flag. The capability is installed separately
from ordinary manual-task services. Ordinary mapping publication starts no Reporter or
polling loop and creates no resolution effect.

`GET /api/v1/projects/{project_id}/issues/{issue_id}/synchronization/projections`
reads original Issue-owned desired-state publications and their latest existing
Reporter receipts without provider access. Issue visibility and the original
native security partition apply before projection or pagination. The bounded
`limit` and opaque `after` cursor use the shared history/cursor mechanisms;
continuation binds Project, Issue, security scope and current visibility.
`live-reporter-v1` declares live operation membership in stable operation-identity
order and independently current delivery facts. It provides neither a complete
provider snapshot nor a frozen membership watermark: refresh from the first page
to discover newly accepted operations. There is no total or aggregate acceptance
verdict derived from a page. Responses use `no-store`.

Each item exposes the exact operation/receipt, original Issue revision, mapping,
native correlation, workflow/status, configured effects, acceptance time and safe
optional Work/Run/action provenance. Resolution lineage remains explicit. Latest
pending/claimed/delivered/failed state, finite failure reason, retry ordinal and
technical times remain independent of business status and execution results.
Private writer authority, credentials, provider diagnostics, task bodies,
security scopes and command replay keys are excluded. PostgreSQL composes the
existing mutation publication and Reporter journal; an optional read service is
installed independently of Source profiles or dispatch workers.

Qualified fresh/unchanged Source reads retain the opaque native status and only
labels referenced by the original mapping alongside its exact observation and
mapping references in the existing freshness record. Reads from before mapping
publication, stale/uncertain reads and disabled pull do not relabel these values.
Earlier receipts without retained values remain explicitly unavailable. These
are last reconciled Source values, not current Reporter inspection facts; their
accepted receipt time, expiry and original policy remain visible. Migration 0088
keeps the existing typed JSON record bounded at 65,536 bytes so supported escaped
managed values and receipt metadata fit without truncation or another journal.

## Source HTTP and Console contracts

Task Source endpoints live under `/api/v1/projects/{project_id}/issues/{issue_id}`.
Retained responses use `Cache-Control: no-store`. Source administration is
separate from ordinary Issue editing and workflow administration.

| Method and suffix | Contract |
| --- | --- |
| GET `source-link` | Nullable original primary association; no provider read |
| GET `synchronization` | Issue/workflow review, field policy, mapping/pin and retained observation conditions |
| GET `synchronization/projections` | Bounded live Reporter receipts and exact original desired effects/provenance; opaque `after` and history `limit` |
| GET `source-choices` | Authorized exact read profiles compatible with an existing native association; independent writer/effect choices |
| POST `source-link/candidates` | Explicit bounded native identity discovery with original scan ID/opaque continuation; no import or attachment |
| POST `source-link` | Existing-task attachment with Issue `If-Match`, reviewed field policy and `Idempotency-Key`; no field overwrite |
| POST `source-link/mappings` | Immutable ownership/status mapping publication with Issue `If-Match`, previous mapping pin (or null) and `Idempotency-Key` |
| GET `source-link/mappings/{version}` | Original immutable mapping under current task visibility |
| POST `synchronization/refresh` | Explicit installed read with `Idempotency-Key`; shared ownership/observation/transition publication |
| POST `synchronization/pull` | Pause/resume using `expected_generation`, `enabled` and `Idempotency-Key`; independent of Issue version and outbound enablement |

Attachment retains the selected preview page’s original `scan_id` and opaque
continuation; later pages cannot be attached under a new scan identity. The scan
ID is optional for first-page clients. Attachment returns 201 for a new link and
200 for an original replay. An exact retry recovers its original receipt before
provider access; the ordinary publisher rechecks replay atomically for concurrent
requests. A native
collision is a finite 200 outcome `{kind:"conflict",canonical:link_or_null}`;
only a currently visible canonical task may be returned. It never merges tasks
or discloses a hidden association. Stale review remains an ordinary conflict.
Mapping publication returns 201 or 200 with its original resource and exact
server-computed mapping reference; clients do not reconstruct the digest.

An explicit refresh fingerprints the original selection, task and read request.
It uses the existing durable Source read journal and shared bounded page driver.
The installed `IssueSourceRefreshPolicy` supplies finite read/retry budgets,
cadence and a crash-recoverable ownership lease; no Factory target is required.
A slice that has not yet reached the linked native item returns `scanning`. The
Console continues it through the common confirmed-command mechanism with the
same frozen request and `Idempotency-Key`. The server retains the original scan,
window, pending page and opaque continuation; clients do not supply refresh
continuations. A paused link rejects a new refresh.

Once an observation is published, exact retries return its original receipt
before another provider call, including after restart/outage or a later pull
pause. Changed reuse conflicts. The existing refresh transaction/replay/outbox
publisher uses a separate explicit-operation namespace; background observation
producers keep their observation-based replay contract. Failed publication leaves
no partial freshness or receipt, while committed page progress remains resumable.

The Console reads retained metadata independently from manual editing and
pipeline execution. Its configuration form discovers native identities only on
request, preserves reviewed ownership on attachment, and reviews field authority
separately from the pinned Project workflow. Status rules select stable workflow
IDs and opaque advertised native states/labels. Push reuses the common Reporter
effect editor and requires a separate enable switch. Generated OpenAPI bounds
constrain rows and UTF-8 mapping size. Current server authorization and transition
policy remain authoritative.

Source feedback shows field directions, last reconciled native values and their
provenance separately from original outgoing desired effects. Outgoing pages
explain pending delivery, conflict, exhausted bounds, blocked access, unsupported
writes and observed delivery without changing an execution outcome or claiming
acceptance. Exact originating execution links and receipt details are progressive.
Explicit refresh reads only retained server state; there is no automatic provider
poll from the card. Independent refresh failures retain stale visible data.

A failed receipt can open the same mapping editor for a reviewed resolution.
Current Issue/mapping/workflow, installed supported choices and the exact original
receipt must remain fresh before a new confirmation. The shared confirmed-command
mechanism freezes the policy, receipt and replay identity; ordinary mapping saves
and explicit resolution remain distinct commands. It offers no unconditional
overwrite or automatic retry of an obsolete intent. English/Russian and narrow
keyboard layouts retain these distinctions. Manual tasks without a source remain
editable and do not request outgoing synchronization history.

Workflow/mapping/head or advertised-choice changes preserve the draft and block
a new review until explicit reload. Confirmed commands use the shared in-memory
frozen intent and replay key; transport retries never silently rebase a request.
Canonical navigation appears only for a server-authorized link. Accepted time,
pending confirmation, original expiry and typed status conditions do not claim
an execution outcome. Refresh and incoming pause/resume are separately reviewed
commands. Forms contain no credentials, endpoints or execution-state setters,
and do not persist native previews or drafts in browser storage.

The standard native runtime installs retained task Source reads and local pull
control with an unavailable reader and no installed choices. Manual tasks need
no adapters. Deployments may use `ServerRuntime::start_with_issue_sources` with
`IssueSourceRuntimeInstallation` to supply the existing bounded reader and exact
safe mapping choices independently of Factory targets. Installation starts no
polling or provider I/O; `HostedWorkSourceObservations` is the shared verified
process reader. Writer authorization uses the separate managed connection and
TaskUpdate contract. This API adds no profile file or credential loader. Deferred
delivery uses the independently installed Reporter dispatcher described above;
Source read installation alone does not enable it.

## Implementation scope

`IssueHandlers` use the existing authorized management dispatch and audited
mutation context over one narrow `IssueStore` port. Memory and PostgreSQL adapters share pure edit/transition/adoption policy.
Memory conformance shares Project/workflow authority and a single revision publication function. Task
revision, status history, replay receipt, safe audit, outbox and any retained
push intent commit together. Test-only failure injection reuses the existing
mutation failure boundaries and publishes none of that state on failure.
Trusted source-policy fixtures model retained association state only; they are
not an operator-facing source configuration or permission grant.

PostgreSQL persists Issue heads, Project-local key allocation, immutable revisions
and status history through migration `0068_issues.sql`. Each mutation reuses the
existing security-scoped idempotency, audit and outbox transaction. Creation
locks Project policy/key allocation; edits, moves and adoption lock only their
Issue head before checking the reviewed preconditions. Retained push intents
are typed references in the existing outbox, with no requirements/reason bodies
and no provider dispatch. Failure rolls back the allocated key, revision,
history, replay and evidence together.

The released server composes these task services independently of Factory and
integration configuration. The Console card follows in task 2.8; existing
workflow settings, Artifact/manual Factory intake and Work/Run controls remain
independently usable.

## Reviewed workflow adoption and applicability

`AdoptIssueWorkflowCommand` requires `Administer` on the visible Issue, separately
from content `Update` and ordinary status `Publish`. It binds the current Issue
revision, workflow/version/status, exact target workflow/version, a bounded
explicit old-to-new status correspondence and the reviewed `IssueFieldPolicy`.
The correspondence must include the current status; all supplied endpoints must
exist in their respective policies. Several old statuses may deliberately map
to one new status. No spelling, vector order or inferred path defines adoption.

The transaction rechecks ownership and mapping. Source-owned status is rejected;
a managed status mapping must explicitly pin the compatible target workflow.
Changed reviewed policy, invalid correspondence, a foreign/unavailable workflow
or stale revision fails without partial publication. The supplied policy is only
a precondition: this command cannot install ownership or mapping authority.
Successful adoption preserves requirements, key and creation time, retains both
workflow versions and records the old workflow/status in the organizational
history. It emits no native push intent, changes no Work, and starts no execution.
Exact replay returns the original receipt even after later edits or policy changes.

`IssueRevision.applicability_version` identifies the last invalidating requirements,
manual status or workflow change. Adoption and manual movement always advance
it, including movement away and back. Changed title/description/acceptance also
advance it; a priority-only edit or unchanged requirements preserve it. Dedicated
reads do not change either version. Later accepted execution actions bind and
check this marker through the same task authority; the marker alone is not
permission to execute or proof of acceptance.

## Management REST v1

All task paths are below `/api/v1/projects/{project_id}/issues`. The explicit
Project scope retains the same Project/Issue preconditions as the application
and persistence ports; an Issue ID or display key never substitutes for it.

| Method | Suffix                            | Contract                                                           |
| ------ | --------------------------------- | ------------------------------------------------------------------ |
| GET    | empty                             | Visible current summaries, excluding requirements bodies           |
| POST   | empty                             | Create a manual draft using `IssueContent`                         |
| GET    | `/{issue_id}`                     | Dedicated current requirements and exact business workflow/status  |
| PATCH  | `/{issue_id}`                     | Replace editable content only                                      |
| GET    | `/{issue_id}/revisions`           | Bounded immutable revision headers, without document bodies        |
| GET    | `/{issue_id}/revisions/{version}` | Dedicated exact retained requirements                              |
| GET    | `/{issue_id}/transitions`         | Current workflow/status/ownership review and direct manual choices |
| POST   | `/{issue_id}/transitions`         | Apply one explicit reviewed direct edge                            |
| GET    | `/{issue_id}/status-history`      | Bounded organizational events, distinct from execution history     |
| POST   | `/{issue_id}/workflow-adoption`   | Explicitly adopt a reviewed retained policy/status correspondence  |

Creation accepts title and description, with optional acceptance and priority;
empty requirements remain a valid draft. No repository, integration, Factory
configuration or model is required. Creation returns `201` for an applied intent
and `200` for exact replay. Every successful Issue response uses
`Cache-Control: no-store`. Detail and mutation receipts return the strong Issue
revision ETag, including the original revision on a replay. Requirements and
status reasons are not returned by ordinary Issue/revision summary pages.

Every mutation requires `Idempotency-Key`; edit, transition and adoption also
require the exact strong `If-Match` Issue revision. Missing preconditions return
`428`; weak, wildcard and malformed versions return `400`; stale or incompatible
review returns `409`. Unknown fields and execution/status/workflow setters in
ordinary content documents are rejected. Client-supplied actor identity is never
accepted. Current visibility is checked before retained body/replay/history
access. A foreign Project/Issue pairing cannot disclose the task.

List discovery applies current Issue visibility before ordering, page size and
continuation. Pages default to 50 and accept 1–200 records. Current summaries use
an exclusive visible same-Project Issue UUID `after`; a cursor outside the
current visibility/Project scope is rejected. Revision and status-history pages
use an exclusive positive Issue revision `after`. No total of hidden records is
returned. Reads neither allocate a revision nor invoke execution or a provider.

Transition review reports the exact Issue/workflow/status versions and current
field policy, with direct edge definitions and safe reasons:
`permission_required`, `source_owned`, `origin_not_allowed`, `evidence_required`.
Read visibility is insufficient to offer a move: its separate current `Publish`
right is checked. Observation reserves nothing; commands reauthorize and apply
the same pure business eligibility and original version/status preconditions.
Adoption uses separate `Administer` authority. Its `expected_field_policy` echoes
the reviewed value from transition review and is compared, never installed.
Neither a terminal-looking name nor a workflow terminal selection grants
Factory acceptance, execution, approval or delivery rights.

OpenAPI describes closed requests, finite reasons, UUID/version preconditions,
text/collection limits, no-store and replay responses. The Console client is
generated from that document through `ui/scripts/generate-api.mjs`; it does not
implement another task-policy or provider model. Transport tests use the
application's opt-in `test-support` fixture, preserving the API → application →
store dependency direction. Production builds do not expose the test fixture.

## Console manual task workspace

Open a Project, choose **Tasks**, then **Create task**. Only a title is required.
Description, acceptance criteria and priority can be completed later; no
integration, repository, Factory setup or workflow-administration permission is
required to create a manual task. Creation pins the Project default on the server.

A task has a direct `/projects/{projectId}/issues/{issueId}` URL and a human key.
Overview shows the current requirements revision and a bounded Activity preview.
The preview reads status changes within the last API page-size task revisions;
it explains that coverage and links to the complete paginated history. Activity
separates organizational status history from requirements revisions. Selecting a
revision fetches its exact immutable body, marks it historical and keeps its
selection in the URL while navigating sections. The main header always describes
the current task, not the selected historical requirements.

Desktop cards place basic Properties beside the content. Narrow layouts expose
Properties through a keyboard-accessible disclosure. Technical IDs and timestamps
are progressively disclosed under Details. English and Russian are supported.
Expanded planning properties, execution requests/results and source association
are separate delivery steps; the base card states their availability instead of
inventing a worker, a Run or a verification outcome. The pipeline-start affordance
is disabled with an explanation until the execution-request journey is delivered.
Existing Factory views remain usable.

**Edit requirements** reviews the complete editable content and exact task version.
Source-owned fields are read-only with an ownership explanation. **Actions** offers
only the server's current direct transitions and their restriction reasons.
A move confirms its from/to status and strong Issue version; a terminal-looking
status never establishes acceptance or delivery authority. Changing the task
workflow is a separate explicit review: choose a visible workflow and exact
version, load it, select the current-to-target status correspondence, then confirm
the current Issue and source-field policy. It does not change the Project default,
start execution or dispatch provider effects.

Confirmed commands reuse a frozen body, version and idempotency key only for safe
transport retry. A conflict requires refreshing and a new review; it never
resubmits automatically. Unfinished drafts and command identities remain in
mounted component memory, are discarded on leave/reload and are absent from
preferences, URLs, telemetry and generic notifications. Dedicated task/revision
queries are discarded from the in-memory query cache after their last observer
leaves. Independent refresh failures retain visible stale data with a warning;
authority failures disable affected editing/move reviews without hiding the task.

### Refresh and concurrent editing

A mounted requirements draft retains its original Issue version and source
content. Refreshing the card cannot silently change its edit precondition. If the
head advances or newly source-owned fields conflict with the draft, the form shows the changed remote values and requires explicit
reconciliation before another save review. Reconciliation keeps local edits to
still-editable fields, takes current values for untouched fields, and always
refreshes source-owned fields. If both parties changed an editable field, the
operator is told that keeping the draft retains their local value. A subsequent
head change requires another reconciliation. Successful receipts advance the
editing baseline to that exact retained revision; an indeterminate confirmed
command keeps its original body/version/key until resolved or abandoned.

The explicit card refresh also revalidates mounted status-history and revision
header pages. Failures retain stale visible data with their existing warnings.
Selected immutable revision bodies and historical URL selection are independent
and are not retargeted or unnecessarily refetched by a head refresh.

Transition reasons are bounded to 128 UTF-8 bytes. Their OpenAPI request and
history schemas publish `x-max-utf8-bytes` in addition to the character bound.
Console form generation uses the byte extension, not `maxLength` as a substitute
for a byte count.

## Optional planning properties and relationships

The application supports an optional `IssuePlanning` snapshot on creation and a
separate `EditIssuePlanningCommand` under ordinary Issue `Update` authority.
Requirements edits preserve this snapshot. Planning changes publish an immutable
Issue revision and original replay receipt without changing business status,
status history, requirements applicability, frozen inputs or execution. They
produce no provider write or assignment-triggered agent work.

Project administrators publish `IssuePropertyDefinitionSet` through
`PublishIssuePropertiesCommand`, using a current definition version and scoped
idempotency key. `GetIssuePropertiesQuery` reads current or exact retained
publications. Version zero is an empty declarative catalog requiring no setup or
read-time writes. Ordinary manual creation can keep completely empty planning
regardless of later catalog publication. Nonempty creation and planning edits
bind the reviewed current catalog version; stale definitions conflict. All
publications and referenced versions remain retained.

Stable property and choice keys are bounded lowercase ASCII letters, digits and
underscores, distinct from their editable display names. Project-defined choice
fields can represent type, component and size without built-in domain enums or
provider-specific names. Retire a field, label or choice rather than removing its
identity or changing its type. New selections reject retired values; an unchanged
retained value can survive an unrelated edit. Exact old revisions and their
catalog versions preserve the original display meaning.

| Supported property kind | Accepted values |
| --- | --- |
| Text | Plain text under the declared UTF-8 byte bound, at most 4,096 bytes; NUL rejected |
| Choice | One active identity from the declared finite choices |
| Boolean | A boolean organizational value |
| Number | An integer within inclusive declared bounds and the safe client integer range |
| Date | A valid `YYYY-MM-DD` calendar date, without a time or timezone |

A catalog has at most 32 fields, 64 labels and 64 choices per choice field, and
at most 65,536 serialized UTF-8 bytes. Keys have at most 64 bytes and names at
most 128. A task selects at most 32 labels and 32 configured values. Unsupported
kinds, duplicate identities, invalid dates, out-of-range values, formulas, nested
value objects and reserved requirements/status/execution/authority setters are
rejected. These bounds are public store constants for subsequent generated
contracts, not provider configuration.

`GetIssuePropertyChoicesQuery` exposes the exact catalog and explicit assignee
availability. An optional `IssueAssigneeDirectory` resolves at most 200 currently
selectable organizational people under the accepted operator and Project context.
Without that directory, choices are unavailable and tasks remain usable as
Unassigned; gateway/service identities are never fabricated as people. A new
selection must be in the current authorized directory. Unchanged prior assignment
can remain when a person is no longer selectable or the directory is unavailable.
Trusted directory choices are excluded from caller intent and replay equivalence;
replay retains the originally accepted revision. Assignment grants no rights,
changes no worker/profile and does not replace actual audited actor attribution.

`IssueFieldPolicy` separately protects assignee, labels, due date, configured
values and parent, as well as the existing requirements/status fields. Changed
source-owned fields conflict while unrelated local fields remain editable. These
local planning fields do not advertise an installed native synchronization
capability or infer a remote write mapping.

A task may reference one visible parent in the same Project. Parent visibility
is checked before lookup or original replay. Self, foreign, missing and cyclic
links are rejected atomically against current relationships; competing memory
mutations cannot jointly introduce a cycle. `ListIssueChildrenQuery` checks parent
visibility and filters direct children before pagination and continuation. Shared
Issue page bounds apply. Parent links do not grant read/edit access, move a
parent's status, imply acceptance or create execution dependencies.

Tasks 2.9–2.10 compose these contracts with memory and PostgreSQL using the same
pure planning policy and existing Issue revision/evidence publisher. Migration
`0069_issue_properties.sql` adds immutable Project catalogs and generated
revision references to their exact versions and same-Project parents. These
columns derive from the retained JSON, rather than another mutable property
store. Foreign keys retain referenced versions and prevent foreign parents;
a parent index supports bounded child discovery. Existing revisions keep their
original JSON and reference the empty catalog. Reads never initialize settings.

Creation, catalog publication and planning edits take the existing Project lock
before an Issue head lock. This makes definition preconditions and ancestry
checks atomic, including concurrent links that would jointly create a cycle.
Issue head edits use `FOR NO KEY UPDATE` so stable parent references remain
compatible with independent metadata edits. All reads and validations inside a
mutation use its existing connection; a one-connection pool is supported.

Planning publication/edit reuses security-scoped replay, audit and outbox in one
transaction. Failure commits neither metadata nor evidence. Generic audit/outbox
facts contain only Project/Issue identities and versions, with no property
values, assignment bodies or push effect. Planning edits preserve content,
status, applicability and status history, and never create or change Work/Run.
Exact replay returns the original receipt after later edits, directory changes
or a pool restart.

Task 2.11 exposes planning through the existing task service and generated REST
client. All paths below use the `/api/v1` prefix:

| Method | Path | Contract |
| --- | --- | --- |
| GET | `/projects/{project_id}/issue-property-choices` | Current definitions and bounded selectable people; explicit directory availability |
| GET | `/projects/{project_id}/issue-property-definitions` | Current immutable catalog with a strong ETag |
| GET | `/projects/{project_id}/issue-property-definitions/{property_version}` | Exact retained catalog, including the empty version zero |
| POST | `/projects/{project_id}/issue-property-definitions` | Closed `{expected_version, definition}` publication with `Idempotency-Key` |
| PATCH | `/projects/{project_id}/issues/{issue_id}/planning` | Closed `IssuePlanning` replacement with positive Issue `If-Match` and `Idempotency-Key` |
| GET | `/projects/{project_id}/issues/{issue_id}/children` | Visible direct children with the existing bounded `after`/`limit` contract |

Catalog publication reviews its exact version in the required body, including
zero for the empty catalog. It does not use the positive-only Issue `If-Match`
parser or an unconditional write. The application atomically checks that version,
retains the immutable publication and returns original replay after later changes.
Definition publication requires Project `Administer`; planning edits require
Issue `Update`. Neither right implies the other. Current/exact catalog reads
require visible Project scope. Parent choices reuse ordinary visible Project
Issue discovery; there is no separate hierarchy, permission inheritance or
provider lookup. Parent visibility is rechecked before mutation/replay, and child
visibility is applied before pagination.

Existing flat REST creation bodies remain valid and may additionally contain an
optional closed `planning` snapshot. Dedicated current/revision responses include
nonempty planning; safe summary and revision-header lists omit property bodies.
Generic requirements editing remains content-only. Every property-bearing
response and child read uses `Cache-Control: no-store`. OpenAPI publishes the
closed scalar/choice grammar, supported counts and UTF-8 bounds. Incoming label
arrays reject duplicates and excessive length before normalization to a set.
Unknown fields, reserved setters, unsupported values and invalid dates fail
without publication; changed stale/directory references retain conflict semantics.

The server offers Unassigned when no person directory is installed and does not
invent a person from the management principal. Existing assignments survive a
directory outage. The requirements editor filters ownership to its own four
content fields. Old
retained revisions decode absent planning as empty. Empty planning stays omitted
from revision JSON and creation intent, preserving the previous stored shape
and creation fingerprint.


## Console Properties and related tasks

Open a Project task and use **Properties → Edit properties**. The panel shows
organizational responsibility, Project labels, due date and installed scalar or
choice definitions. Type/component/size are ordinary Project-defined fields,
not provider names, agent roles or execution settings. An unconfigured Project
shows an empty-definition explanation. Without a person directory, Unassigned
remains usable; a retained unavailable assignment is identified honestly and can
be cleared. Source-owned controls explain their ownership and remain disabled.

The editor keeps drafts in mounted memory only. Review shows the exact task and
catalog versions and readable selected values before a dedicated planning PATCH.
A changed task, catalog, directory or ownership requires explicit reload/discard
and a new review; refresh never silently rebases the draft. A command whose
response was lost retries only its frozen original body, ETag and idempotency key.
Planning saves preserve requirements, business status and active execution inputs,
and start no agent, Work, Run or provider operation.

Saved planning values resolve their names from the exact retained catalog when
it differs from current definitions. Retired values may remain unchanged or be
removed, but cannot be newly selected. Unchanged values survive tightened bounds;
changed values use current definition bounds. Failed catalog refresh retains the
readable prior data, marks it stale and blocks a new confirmation. An optional
planning/relationship read failure does not hide the task or its manual
requirements editor.

Parent and sub-issue links show visible human task keys and titles. Parent
selection reuses bounded authorized Project task pages, excludes the task itself
and preserves one selected label across pages. Only one direct-child page is
shown at a time. The server still checks visibility, Project scope and cycles;
links grant no permissions, status roll-up or execution dependency. No raw ID or
provider credential form is needed. Narrow layouts use the existing keyboard
Properties trigger and confirmation focus restoration, preserving the selected
historical requirements until explicit task navigation. English/Russian labels
cover these controls; technical identifiers stay in collapsed details.

## Reviewed execution requests

`RequestIssueExecutionCommand` reserves an explicit request for the exact reviewed
Issue version. It requires Issue `Execute`, visible Project/Issue/Factory target
and Repository references, and ordinary Project-owned Factory Run `Execute`.
It validates immutable target/repository versions and reuses the ordinary base
selection policy. Missing or whitespace-only acceptance returns the finite
`IssueAcceptanceRequired` error; the same text remains a valid manual draft.

The request retains its original revision, target, repository policy and finite
`TitleDescriptionAcceptanceV1` mapping. Task bytes are UTF-8 title, two LF bytes,
then description; acceptance bytes are unchanged. Content digests and byte sizes
are retained without copying task bodies into generic request diagnostics.
`IssueExecutionMapping::task_bytes` is the shared mapping for input publication.
Later Issue edits do not modify these references or an associated Work.

Scoped exact replay returns the original reservation receipt before target lookup
or current-content validation; changed key reuse conflicts. Current rights and
reference visibility still apply. `GetIssueExecutionQuery` returns the current
exact request record without publication or admission. An absent association is
pending preparation, never admitted execution. The request UUID determines stable
ordinary manual admission and replay identities, independently of its transport
key. A later intentional execution has a distinct request/Work identity.

`AssociateIssueWork` is a trusted store command accepting an original ordinary
`PublishedFactoryAdmission`, not caller-selected Work/Run IDs. It reuses admission
relationship validation and atomically retains the same-Project Issue link and,
when present, exact request association. Work/configuration/repository and mapped
task/acceptance digests and sizes must match the reviewed request. A Work belongs
to at most one Issue and its original Run; one request cannot be retargeted to
another Work. Exact link replay preserves the original association. Legacy
Artifact-based Work may remain unlinked or be linked with no invented requirements
revision. Infrastructure retry stays under the original Run and link.

These narrow application/store contracts have memory and PostgreSQL conformance
and compose with the shared publication service and native runtime below.
Execution HTTP/Console controls remain separate implementation slices.
Reservation and association retain their own boundaries; explicit publication
uses ordinary admission. Manual creation continues to use `IssueHandlers`
without these execution dependencies.

## Shared Project text publication

`ProjectTextIntake` is the common application service for manual Project-owned
input publication and ordinary Factory admission. `PublishIssueExecutionCommand`
adapts a previously reserved Issue execution to this service; quickstart can call
it with its reviewed submission without creating an Issue. The Issue adapter
requires the original admission security partition and current Issue/Project/
target rights. It uses the retained exact revision on first preparation, then the
shared retained submission on replay; later text edits never replace its inputs.
Explicit classification is frozen at the first shared reservation. The response
contains a Work/Run association only after that association commits. A failed
association retains the admitted Work and resumes the same link.

`ProjectTextSubmission` pairs a reviewed `ProjectTextIntent` with exact private
documents. Preparation reads retain only intent, object identities and the original
receipt. PostgreSQL stores document bodies separately; the scoped
`project_text_documents` read is used when recovery needs the original bytes.
Artifact metadata, protected transfer and cleanup select only their required fields
without loading the bodies or the admission Flow. Body retirement removes the
separate record rather than creating empty, invalid documents in retained metadata.

A shared submission binds Project, exact Configuration/Repository versions,
reviewed source policy, classification, application mapping, external Work identity
and replay key. Each document is nonblank UTF-8 within the common 1 MiB byte
ceiling and aggregate Context bound. Tabs/CR/LF are preserved; other control
characters are rejected. Deserialized commands are revalidated before reservation
or effects. Quickstart applies its stricter 16 KiB field ceiling at its ingress.
Private bodies are omitted from generic Debug diagnostics.

The operation reserves both logical/upload identities and original private intent
before byte I/O. A trusted VCS resolution is frozen under the current owner before
publication. Recovery verifies existing bytes before writing a missing object and
independently checks each published digest/size. The complete pair becomes visible
atomically, then the existing ordinary admission transaction links its original
Work/Run receipt. Current rights still apply to exact replay, which precedes
mutable target/source resolution. Changed reuse conflicts.

Memory and PostgreSQL conformance use a 30-second preparation lease, a five-minute original
preparation deadline, at most 16 ownership attempts and an original 30-day private
input lifetime. Recovery retains these bounds, the first committed base and the
same object identities. Owner fences are checked before byte effects and at
freeze/publication/admission. These preparation bounds are separate from Run
budgets. PostgreSQL commits the original input receipt inside ordinary Factory
admission; Issue association and its request/replay/audit commit separately as one
atomic link. Interrupted association resumes that original Work/Run.

`ProjectInputQueryStore` exposes only complete publication metadata within its
owning Project. `FactoryArtifactReader::with_project_inputs` reads exact selected
private inputs through the existing byte reader, with Project/digest/size/plain-text
media and required retention-window checks. Build-output reads preserve their
own authority and reject these inputs. Source and Project reads share byte and
retention checks while retaining distinct provenance. No Build, Job, Agent or
Source installation is fabricated to publish the original task.

Native runtime resources share one retained input reader and the same text intake.
Protected execution transfer resolves the exact Project publication without a
producing Build, and caps its grant at the original private expiration.

The existing supervised private-input cleanup worker includes abandoned Project
reservations. Ordinary Build holds serialize with cleanup eligibility; a new hold
conflicts once deletion has started. Physical deletion precedes its acknowledgement.
The pair's private bodies are scrubbed after deletion; immutable receipts and
cleanup identities remain. An indexed preparation due time selects initial cleanup
and hourly rechecks without sorting all historical receipts. Each acknowledgement
atomically advances that time to the earliest remaining object due time; a partially
acknowledged pair remains immediately retryable. The original Run has an indexed
reference for hold checks. Periodic rechecks remove late orphan writes. Completed
replay compares retained byte digests/metadata and returns its original receipt
without restoring bodies or minting new read authority.

PostgreSQL restart/process-termination, transaction rollback, migration and native
S3 qualification are recorded in [task 3.3 verification](../../openspec/changes/add-factory-work-card/verification-3.3.md).
Storage separation and indexed cleanup are covered by the
[review corrections](../../openspec/changes/add-factory-work-card/verification-review-fixes.md).
HTTP execution receipts and Console controls are specified below. See the
[quickstart handoff](../../openspec/changes/add-factory-bugfix-quickstart/shared-project-inputs.md)
for reuse and its remaining qualification.

## Reviewed execution HTTP contract

Execution is optional and separate from manual Issue create/edit/discussion.
`POST /api/v1/projects/{project_id}/issues/{issue_id}/executions` requires
`Idempotency-Key`, the reviewed Issue revision in `If-Match`, and a closed body
containing `target` (exact configuration/repository versions, source selection,
versioned mapping) and ordinary Work `classification`. Issue execution rights,
visible referenced resources and ordinary Project admission rights all apply.
Saving a draft or reading an Issue never performs admission.

A first durably linked request returns 201; exact completed replay returns 200.
A retained request whose publication cannot finish within the bounded request
window (five seconds in the native host) returns 202 with `state: pending`. All
successful POST responses include `Location` for the exact request. Repeating the same body, If-Match and key resumes
that request, including after a lost response or interrupted association; a
changed replay conflicts. Legacy retained requests without classification remain
readable with their original replay encoding. A new key is an intentional
additional execution and must review the current Issue revision. The common Project text intake owns byte
publication, frozen VCS resolution and ordinary Work/Run admission.

`GET .../executions/{request_id}` reads retained metadata only. `pending` means
there is no committed Issue-to-Work/Run association; `admitted` includes the exact
association and existing `/api/v1/factory-runs/{run_id}` URL. It does not imply a
running or successful Run. Requirements digests, sizes and exact target/revision
lineage remain immutable; task/acceptance bodies and transfer authority are absent.
Responses use `Cache-Control: no-store`; status has no Issue ETag because linking
execution does not edit the Issue revision.

`GET .../executions?limit=...&after=...` enumerates visible request metadata in
ascending `(requested_at, request_id)` order, at most 200 entries per page. Its
`request-watermark-v1` observation fixes membership at the first page’s maximum
committed reservation sequence and records the observed Issue revision. New
requests require a fresh traversal. Associations reflect each page’s current
read; this is not a transaction snapshot of Run progress. There are no inferred
totals. Opaque canonical cursors bind Project, Issue, security partition, current visibility and
observation version; malformed or mismatched cursors return 400. Visibility is
checked before reading, ordering or producing continuation metadata. GETs never
resume publication. Existing Artifact admission bodies and Run/control URLs keep
their contracts.

HTTP Red/Green and native restart qualification are recorded in
[task 3.4 verification](../../openspec/changes/add-factory-work-card/verification-3.4.md).


## Console execution review and history

The card's **Start pipeline** action is separate from saving an Issue. Missing
acceptance criteria lead back to requirements editing. Pipeline configuration and
repository setup links remain available when no visible enabled target exists.
Catalog reads begin only when the operator opens execution inputs; ordinary manual
create/edit requires neither a provider nor Factory setup.

The card reuses the Factory Work form's configuration/repository/source selector.
It offers authorized visible versions and repository-supported default reference,
allowed reference or exact revision. Priority bounds and risk choices come from
the generated ordinary Work schema. Review shows the exact target, source
choice, classification, Issue revision and requirements. New confirmation is
blocked during a refresh or after a failed refresh; retained choices remain
visibly stale. Execution authority is independent of Issue transition choices.

Confirmation calls the existing reviewed execution POST. The shared in-memory
confirmed-intent mechanism freezes the body, `If-Match` and key. A lost response
retries that same command. A 202 receipt retains the same continuation, releases
modal focus and exposes **Resume preparation**, allowing requirements edits and
section navigation without rewriting the original review. An admitted receipt
links the exact existing Run; it does not imply successful completion or move the
Issue's business status. After a definitive stale conflict, refresh and review
again; target edits stay in the form and the next confirmation has a new key.
A later intentional execution also gets a new key and its own retained selection.

**Runs** reads bounded chronological request history and exact selected metadata
through the existing GET contracts. `execution=<request_id>` in the card URL keeps
historical selection across card sections, requirements revision links and reload.
It contains no request body or admission key. Current Issue requirements remain
separate; execution against an earlier revision is explicitly labelled. Refresh
is read-only and retains last loaded metadata with a stale notice on failure.
Existing Run pages own progress, diagnostics and attempt controls.

Unsubmitted inputs and confirmed continuation keys exist only in the mounted
form. Leaving or reloading discards them; the server's retained request/history
remains readable. Reload restores historical selection, not a mutation key or an
automatic retry. A response arriving after the form leaves cannot select an
execution on a different page. No Console publication algorithm, provider adapter,
background admission worker or second Work journal is introduced.

Console Red/Green, browser and bundle qualification is recorded in
[task 3.5 verification](../../openspec/changes/add-factory-work-card/verification-3.5.md).

## Explicit accepted pipeline status actions

The internal `ApplyIssuePipelineStatusCommand` uses the existing authorized
`IssueStore::transition_issue` boundary. Its accepted input pins the Project Issue
workflow, immutable version, direct edge and declared outcome. Work, Run, original
reviewed requirements revision, Issue, Flow Run, cycle, node attempt and stable
operation identity are retained as provenance. This is an accepted-producer
contract, not a deserializable REST body: the trusted producer owns acceptance
and supplies the Flow/attempt facts. The Issue boundary independently checks the
Work association, original requirements, current applicability and pinned edge.
Names, model prose, Run completion and accepted Factory Decisions never select
an Issue move. Ordinary manual creation and transitions need no Factory or provider.

Both origins share workflow, source-ownership and evidence rules. Evidence is
qualified from explicitly selected immutable journal records for the exact
Work/Run/candidate, using the existing journal validator and typed discovery
facts matching their canonical payloads. Accepted execution requires an accepted
Decision; required checks require a nonempty manifest with all required gates
passed. Unknown, unrelated, unqualified, future or expired evidence cannot grant
a move. History keeps safe provenance and record digests, never evidence bodies.

A reviewed Work starts with the applicability marker of its original requirements
revision. Its own successful move atomically advances that Work's marker with
the Issue revision, status history and existing audit/outbox publication. This
allows successive configured edges in the same lineage. Manual status moves,
status moves from another execution, requirements edits and workflow adoption
invalidate old applicability even if values later return to their originals.
The shared contract requires the same invalidation for committed source status
moves; source-transition integration remains outside task 3.6. Current source
ownership is checked independently before a new move: source-owned Status blocks
manual/pipeline replacement. A source-policy change alone does not advance the
applicability marker. Reads, another execution request and unrelated priority
edits do not invalidate it. Legacy Artifact associations without reviewed
requirements cannot invent this authority.

Accepted commands reuse the transition replay record and an operation-derived
key. The first attempt retains its exact intent, including a rejected command;
recovery cannot retarget it to a newer head. A successful replay returns the
original revision after later edits, subject to current authorization. Local
status changes require no Reporter and preserve immutable Work/Run results.

PostgreSQL retains the Work applicability marker beside the existing association.
Migration 78 initializes existing reviewed associations from their original
requirements revision, never the latest head; legacy associations without a
reviewed binding remain ineligible. Only the winning Work advances its marker
in the same transaction as revision, history, audit, outbox and receipt. Migration
79 keeps history metadata bounded at 64 KiB, accommodating the supported 256
record references without storing evidence bodies.

The existing idempotency record seals the pipeline command before business
publication, retaining its original guard, selection, provenance and move time.
Transient failure leaves that command pending; definitive rejection is retained
without a business mutation or mutation evidence. Recovery cannot change the
command or refresh its timestamp. The sealed time also supplies the shared audit
and outbox publication metadata. Retained metadata contains no deserializable
evidence-authority flags: the shared codec requalifies exact journal references
for both initial publication and recovery. Candidate metadata without selected
evidence is rejected.

`FactoryRuntimeResources::issue_status_executor` explicitly installs the local
action in the existing trusted-action adapter/registry. The host supplies accepted
attribution, current authorization policy and execution clock. Binding parameters
are the closed workflow/version/edge/outcome selection; projected input contains
only optional candidate/evidence references (`{}` for an action without evidence).
Evidence must belong to the frozen input's explicitly selected source records.
Work/Run/Flow/cycle/attempt/operation identity comes from the common owner, never
model payloads. Current Issue visibility and transition rights, execution windows
and resource bounds are checked on recovery. Lost acknowledgement after a local
commit returns the original receipt after later Issue edits, then the common node
owner retains one result. Cancellation neither replays a move nor undoes one.
No Reporter, provider connection, additional transition handler, execution journal,
outbox or recovery worker is introduced. Manual task creation remains independent.

Flow and Factory Configuration validation/publication use the same local-action
checker. The existing generic `FactoryFlowAction` envelope remains extensible;
`transition_issue` parameters use the closed `FactoryIssueStatusActionParameters`
contract (`workflow_id`, `workflow_version`, `transition_id`, `outcome`). The
installed plugin reference must still be selectable. Publication resolves the
exact retained Project workflow under current read visibility and accepts only
an explicit edge permitting the pipeline origin and an outcome declared by the
node. Evidence requirements belong to that immutable edge; action parameters
cannot weaken them, select a target Issue, carry source effects or infer a move
from model prose. No public pipeline-status execution endpoint is introduced.

The existing Console Flow editor reuses installed execution profiles and shows
local Issue actions separately from `set_status` source/Reporter effects. Select
a visible workflow, explicitly inspect its current or retained version, review
pipeline edges and their evidence requirements, and choose a declared action
outcome. Switching execution profiles discards the previous local binding review;
applying reviewed edits preserves it. A retained workflow version remains visibly
selected even when the catalog lists a newer version. The existing typed
input/source mapper binds optional `candidate` and
`evidence` record digests; it does not grant evidence authority. Review preserves
an already configured outcome. The shared publication confirmation retains the
exact Flow version precondition and frozen command on retry; stale publication
requires refreshing and validating the draft again. Drafts remain in memory.

An Issue execution request checks every local action's workflow pin against the
reviewed card before reserving Work. Ordinary automatic Source input currently
cannot supply a reviewed Issue association, so activation with a local Issue
action is rejected; a disabled draft can be retained for repair. Legacy
unassociated Work still yields explicit unavailable at execution rather than
creating or guessing an Issue. Local actions and manual task creation require
no provider connection, credential or external mapping. These authoring checks
perform no status movement, provider calls or input publication.

Contract Red/Green and regression results are recorded in
[task 3.6 verification](../../openspec/changes/add-factory-work-card/verification-3.6.md).
PostgreSQL migrations, interruption and native owner recovery are recorded in
[task 3.7 verification](../../openspec/changes/add-factory-work-card/verification-3.7.md).

REST/Console authoring, target compatibility and publication review are recorded in
[task 3.8 verification](../../openspec/changes/add-factory-work-card/verification-3.8.md).
