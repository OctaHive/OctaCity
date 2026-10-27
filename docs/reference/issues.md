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

Normal installed Source/Reporter startup delivers an explicitly reviewed manual
Issue-status push through that same ordered worker without a Factory Configuration
or Run. The verified proxy's identity/scope is preserved on the local transition,
so its original native link and writer grant remain in the same partition. Delivery
preserves unrelated labels and the accepted local Issue status. A missing push
mapping or read-only/source-owned pull produces no outbound effect; incoming status
echoes retain the current revision. Safe Reporter receipts remain visible while
retrying and after delivery/failure, independently of local transition success.

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

In **Project → Tasks → Task sources**, select an installed connection and exact
read profile before any task or Factory target exists. **Preview tasks** is a
bounded read; it neither imports tasks nor enables execution or status delivery.
Review publication separately and explicitly enable discovery when desired.
Published settings appear with their retained version. **Refresh tasks** reloads
the ordinary canonical task list after the supervised importer has made progress;
successful configuration publication alone does not claim a task was imported.

**Edit task import** opens the existing connection/profile and discovery setting.
Publication retains the import ID and filters and requires the exact current
version; a conflict refreshes that version while keeping the operator draft for
another review. Editing import settings does not retarget already imported tasks.
**Remove task import** requires separate confirmation and publishes a terminal
`retired: true`, `enabled: false` version through the same versioned API. Removed
imports leave the current list, including its pagination, while exact historical
versions remain readable. They cannot be reactivated by editing or replaying an
older publication. Removal requires no available reader or active read grant and
does not delete tasks, pause their independent pull or change outgoing mappings.

Enable/disable reviews preserve the original import identity, source pins and
filters. They change discovery only; existing tasks, linked-task pull, Factory
inputs and outgoing reporting keep their separate controls. Drafts and preview
bodies stay in the mounted form. Transport retry reuses the frozen request and
replay key; stale access/profile changes require refresh and another review.
After a stale preview, **Retry** refreshes connections and Source choices while
preserving the draft. Preview and publication wait for that refresh; if the
original Source pin disappears, select its replacement explicitly before another
preview or review. A failed refresh keeps the same recovery action available.
Each fresh enable review resolves current Project access for the original
connection version and exact Source/connection-configuration pins. It updates
only the mutable access revision, preserving the import ID and filters.
Unavailable or replaced pins block review with safe Retry guidance. Disabling
discovery does not require resolving a currently authorized reader.
Connection/profile absence leaves manual task creation available and points to
integration/access setup. These controls support English/Russian, keyboard
review with focus restoration and narrow screens.

The Project controls use `issue-source-choices`, `issue-sources/preview` and
versioned `issue-source-imports` under `/api/v1/projects/{project_id}`. Open a
retained task's **Properties → Source** for its existing mapping and separate
incoming/outgoing feedback. The linked workflow canvas retains its independent
publication/default review. A native mapping uses that task's exact workflow
version; publishing a later Project default does not change it.

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

The standard native runtime keeps retained task Source reads and local pull
control available when Source support is absent. Manual tasks need no adapters.
An installed `source_profiles_file` supplies the bounded reader, supervised
consumers and exact mapping choices through normal startup independently of
Factory targets; see [installation contracts](integration-management-contracts.md).
Project import and linked-task pull obey their separate reviewed controls.
`HostedWorkSourceObservations` remains the shared verified process reader.
Optional Issue writer choices use the same installed Reporter inventory with
explicit delivery bounds and native effect ceilings. They recheck the original
managed connection's separate TaskUpdate authority; a read grant is insufficient.
Replacing mapping/delivery settings changes the writer pin and requires another
review. Installation, source attachment and workflow/default publication do not
enable status push or adopt a newer workflow for an existing Issue. Alternate
`ServerRuntime::start_with_issue_sources` installation remains supported. Deferred
delivery uses the independently installed Reporter dispatcher described above.

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
| POST   | `/{issue_id}/comments`            | Append operator text with a metadata-only scoped replay receipt     |
| GET    | `/{issue_id}/comments/{comment_id}` | Independently authorized no-store read of one exact comment body |
| GET    | `/{issue_id}/timeline` | Bounded safe references with original local membership and independent Source coverage |
| GET    | `/{issue_id}/results` | Exact Work/collection read with a Run version fence and explicit coverage |

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
The Console's planning-priority list starts with Blocker (3), High (2), Medium (1)
and Low (0), plus Not set (`null`). One shared presentation mapping supplies the
create/edit selector, confirmation and card labels. The numeric REST range stays
0–255; an existing non-preset value appears as `Existing priority (N)` and is
preserved through unrelated edits until explicitly replaced or cleared. The four
presets are not a closed server enum, so the shared list can be extended without
migrating existing Issue values. Execution scheduling priority is separate.

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
Properties, reviewed execution requests, original Run details, source association,
PR/MR, Checks, Artifacts and shared actions are independently loaded sections.
**Start pipeline** reviews the current requirements and exact target; incomplete
acceptance leads to requirements editing before execution review. Saving a task
never starts execution. Unavailable optional capabilities do not prevent local
editing, and existing Factory views retain their original controls.

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

Inside **Properties → Edit properties**, **Create label** beside the label list
creates a reusable Project label. Enter its display name and review publication
before confirming. This adds a stable generated key
to the current catalog while retaining all existing labels (including retired
ones) and custom property definitions. A changed catalog requires a fresh review;
an uncertain transport outcome retries the original publication and idempotency
key. After creation, select the label in the same editor and save properties.
The editor retains its draft, including responsibility, selected labels, date,
parent and custom values. Only its own successful catalog addition advances the
review baseline; concurrent task, catalog, directory or ownership changes still
require an explicit reload. Publication itself does not change any task.
Source-owned labels remain read-only; catalog limits and publication permissions
are enforced by the server.

The editor keeps drafts in mounted memory only. Review shows the exact task and
catalog versions and readable selected values before a dedicated planning PATCH.
A concurrent task, catalog, directory or ownership change requires explicit reload/discard
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
S3 qualification are recorded in [task 3.3 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-3.3.md).
Storage separation and indexed cleanup are covered by the
[review corrections](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-review-fixes.md).
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
[task 3.4 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-3.4.md).


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
[task 3.5 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-3.5.md).

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
Committed source status moves use the same applicability invalidation. Current source
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
[task 3.6 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-3.6.md).
PostgreSQL migrations, interruption and native owner recovery are recorded in
[task 3.7 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-3.7.md).

REST/Console authoring, target compatibility and publication review are recorded in
[task 3.8 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-3.8.md).

## Safe card and selected execution projections

The application read contracts in task 6.1 compose existing authorities rather
than persist another execution projection. `GetIssueCardQuery` returns safe
current task metadata, optional planning values and the exact retained workflow.
Requirements bodies keep their dedicated reader. Business transition history
continues through `ListIssueStatusHistoryQuery`, including original workflow,
before/after statuses, actor and optional accepted pipeline/source provenance.
Neither a newer Project workflow nor a completed Run replaces those facts.

`GetIssueExecutionProjectionQuery` reads one exact execution request. Preparation
without a durable association, an available original Run and unavailable Run
discovery are distinct. The existing bounded request history enumerates requests;
each selected request has its own technical observation, including parallel or
queued Runs. Frozen title/description/acceptance digests determine whether its
requirements still match the card. That comparison is not status-action
applicability, a current-candidate verdict or execution acceptance. Later card
edits never replace the selected Work/Run. `GetIssueWorkExecutionQuery` also reads
imported or legacy Artifact associations directly; missing request/requirements
identities remain absent rather than fabricated.

`ListIssueExecutionRecordsQuery` selects one existing Factory diagnostic
collection for that exact associated Work. It reuses common bounds, Run/collection
cursors, subject-bound safe records and current Factory/Project authorization.
The declared `live-factory-collection-v1` contract observes collections
independently; it is not an atomic card snapshot or complete verification verdict.
Candidate, evidence, plan, assessment and Decision subjects retain their original
Flow/cycle/node/input identities. Empty evidence or Decision pages do not mean
success, and indeterminate outcomes stay indeterminate. Original Work references
and separate completion/stop collections remain available without aggregating
inputs, protected outputs or transcripts. The card pagination contracts below
add a Run version fence over this existing reader. Subject-bound detailed
[Checks/Artifacts inspection](#subject-bound-checks-and-artifacts-http-inspection)
composes the same retained authorities.

Optional `IssueExecutionJobHandlers` expands one Job only after verifying its
exact retained Build-execution record under the selected Work and independently
checking current Build/Job visibility. It reuses the ordinary authorized Job
reader. Worker assignment comes from the actual enrolled Agent/Pool record;
optional frozen model/tool profile facts come from the original Build intent.
Neither a queued Job nor an absent profile invents an AI role or assigned worker.
Safe Job lifecycle/times and terminal classification exclude JobSpec, placement,
output references and ownership material. Log navigation retains the original
Build/Attempt/Job and event cursor, never a later retry or latest Attempt. Following
that reference still requires the existing authorized log/event reader; no log
body, transfer URL or content privilege is returned here.

All these queries are read-only and require no provider call, input publication,
admission, status change or new execution owner. Manual card reads install no
Factory or ordinary Job dependency. Synchronization remains an independent read.
Application, REST and [Console journal](#console-execution-journal) qualification
is described below. Qualified tool-event production is described in the
execution-observation section.

## Operator-only discussion

Tasks 6.2–6.3 add optional `IssueDiscussionHandlers` over the narrow
`IssueDiscussionStore` contract with memory and PostgreSQL adapters. It requires the existing Project/Issue
authority, management policy and trusted clock, with no execution, Context,
provider or input-publication port. Saving or reading discussion never changes
the Issue revision/status, starts a Run, records a gate response or supplies
requirements to a later execution. Mentions and approval-like prose remain text.

`AppendIssueCommentCommand` freezes plain text, the owning Project/Issue and an
optional root parent. Comment identity and original time are server-generated;
actor provenance comes only from the accepted management context. Trusted-network
requests retain unauthenticated-management attribution rather than a fabricated
person. The closed intent accepts no actor, sequence, timestamp, execution or
attachment fields. UTF-8 bodies must be nonblank and at most 16,384 bytes, permit
ordinary multiline text and retain original whitespace. Recognized credential
and private-capability markers use the same conservative check as Factory safe
text and execution traces. This filter is not a complete secret detector; body
access remains a separate protected content boundary. Debug formatting redacts
the body and validation errors contain no supplied text.

Validated deployment limits may narrow the body ceiling and the lifetime limit
of 1,000 comments per Issue. Roots and replies share that quota. Sequence/lifetime
count is separate from retained body records; content cleanup does not reset it. Only a retained root in the same Issue can be a parent; nested,
absent and foreign parents reject without publishing state. Failed appends and
exact replay consume no sequence or quota. A winning scoped replay retains its
original identity, actor, time and metadata even after limits are narrowed;
changed body/parent/target reuse conflicts.

Both adapters reuse the existing Issue transaction, scoped replay and safe
audit/outbox evidence. It commits body, sequence and
original receipt together; replay fingerprints/receipts contain a digest and
metadata rather than another body copy. No second execution journal or reporting
outbox is introduced.

Append uses `Create` on the Issue-scoped `IssueDiscussion` resource, independently
of Issue editing, execution or status rights. Dedicated `GetIssueCommentBodyQuery`
uses `View` on that discussion resource. Both also require current owning-Issue
visibility before parent/replay/body lookup. `ListIssueCommentsQuery` needs Issue
visibility and returns only bounded sequence-ordered metadata, without granting
body access. Its sequence continuation is an application contract, not an opaque
HTTP cursor or an atomic cross-section observation.

Migration `0089_issue_discussion.sql` adds `issue_comments` with separate body and
metadata columns and an Issue lifetime counter independent of the revision.
Concurrent appends serialize on the existing Issue lock. Counter allocation,
body, replay outcome, audit and outbox publication form one transaction; failure
at any publication stage leaves no consumed sequence or partial comment. The
common mutation registry retains only a request digest and metadata receipt,
including across server restart. Discussion publishes no native-sync intent.

`POST .../comments` accepts only `body` and optional `parent_id`, with a required
`Idempotency-Key` and no Issue `If-Match`: append does not revise the card.
Applied appends return `201`; exact replay returns `200`, both with metadata-only
`IssueCommentOutcome`, `Location` for the dedicated read and `Cache-Control:
no-store`. Changed reuse returns `409`; invalid bodies, parents and exhausted
quotas return generic `400` errors that contain no supplied text.

`GET .../comments/{comment_id}` returns `IssueCommentContent` only after separate
discussion and owning-Issue visibility checks. Successful responses use
`Cache-Control: no-store`. Forged actor/time/sequence/control/attachment/target
fields reject. OpenAPI publishes closed request/response schemas, UTF-8 byte and
lifetime hard caps, replay and independent permission mappings; the generated
client includes these operations. A stricter trusted application composition can
lower the hard caps; request documents cannot configure them.

Timeline metadata pagination uses the independent observation contract below.
Console discussion uses the card contracts below; explicit retention and holds are documented under [collaboration content lifetime](#collaboration-content-lifetime). Evidence is recorded in
[task 6.2 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-6.2.md)
and [task 6.3 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-6.3.md).


## Bounded activity and result observations

`GET .../issues/{issue_id}/timeline` uses `ListIssueTimelineQuery` and the narrow
`IssueTimelineStore`. It merges retained revision headers, explicit status facts,
original execution-request references, operator comment metadata and scoped
Source read-receipt references. Rows exclude requirements/comment bodies, status
reasons, provider payloads, native credentials, logs and technical Run detail.
Dedicated content and execution readers retain their own permissions. Manual
Issues need no Factory, input publication or installed provider to use this read.

Consecutive Source reads with identical snapshot and installed provenance produce
one Activity entry, using the first receipt's original time and identity. Read
time and expiry alone do not create another entry. Content changes, changes in
installed provenance and a return to an earlier snapshot remain separate events.
Folding happens before pagination and applies to already retained history. Exact
technical read receipts and the current freshness state remain independently
available to synchronization readers.

The default limit is 50, with a maximum of 200. Optional `kinds` is a comma-separated
set of `revision,status,execution,discussion,source`; omission selects all kinds.
Ordering is original timestamp, source kind in that order, then canonical stable
identity lexically within the source. Continuation is exclusive. The initial
observation binds the Issue revision, committed execution-request membership,
lifetime discussion sequence and a scoped Source fingerprint. New edits, requests
and comments cannot enter an existing traversal, even if their timestamps sort
before its current position. `current_version` reports the independently current
Issue head; it does not rewrite original requirements or facts.

The Source fingerprint binds native-link correlation, ordered Activity receipt
identities, pull generation and mapping version. Quiet rereads keep the fingerprint
stable; a delayed receipt that changes Activity membership invalidates it even if
the number of entries stays the same. A changed fingerprint
omits only the Source section and marks `source_stale`; other frozen local
sections remain readable. PostgreSQL uses a read-only repeatable-read transaction
and an optional-section savepoint. Source storage failure marks
`source_unavailable` without discarding local metadata. If Source was unavailable
at the first page (`boundary.source = null`), it stays unavailable for that
traversal; a fresh traversal can capture recovered Source data. Receipt
`expires_at` is the original provenance proof deadline, not a promise that body
content remains retained or that provider data is current.

`GET .../issues/{issue_id}/results?work_id=...&collection=...` uses
`ListIssueResultsQuery` over the existing authorized Factory diagnostic reader.
`work_id` is an exact retained Issue association, and `collection` is one existing
Factory diagnostic collection. Its default limit is 50, maximum 100. This read
requires current Issue visibility and independent Factory/Project authorization;
no provider is contacted. Nullable legacy requirements provenance stays absent.
The original local boundary and selected Work/Run/collection remain fixed, while
current Issue version and Source freshness are reported separately.

The `issue-result-run-version-v1` continuation carries the original Run version.
The reader checks that version before and after the common record read. A changed
Run returns `coverage: stale`, no records and no continuation, requiring a fresh
traversal. A retained association whose technical content is missing, hidden or
unavailable returns `coverage: unavailable`, no records and no continuation.
Readable bounded collections use `coverage: partial`, including an empty or final
page. None of these observations establishes complete verification, successful
checks or acceptance; original candidate/evidence/Decision subjects keep their
own authority. The journal reader below and the typed Checks/Artifacts inspection
reuse the same authorization, retained owners and Run-version fencing.

Both routes use closed query grammar, no-store responses and the shared canonical
opaque cursor codec (maximum 2,048 bytes). Cursors bind Project, Issue, current
security partition, normalized visibility, filters/selected execution and original
observation. Current visibility is checked before lookup, ordering or pagination;
a hidden Issue returns not-found. Invalid, noncanonical, foreign-scope or future
membership cursors are rejected. Each section is independently observed; these
contracts claim no atomic snapshot across external systems and perform no writes.
OpenAPI supplies the bounded schemas and generated Console reads. See
[task 6.4 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-6.4.md).


## Console Activity, Runs and Results

Overview includes a bounded Activity preview. Activity renders server-ordered safe
references with original timestamps, explicit Source/revision observation coverage,
and independently expanded plain-text comments. Root replies are identified and
only retained root comments offer Reply. Comment bodies are loaded through their
dedicated authorized endpoint with browser `cache: no-store`; their query data is
discarded when expansion unmounts. Neither comment text nor mentions can start
execution, approve results or answer a gate.

The operator discussion form uses the generated UTF-8 byte ceiling and the shared
`ConfirmedCommand` lifecycle. Review freezes the original body, optional parent and
idempotency key. Transport retry repeats that exact command; definitive rejection
requires another explicit review. Confirmation feedback contains no comment body.
Drafts, confirmed values and keys stay in component memory, leave no browser storage
or URL entry, and are discarded when leaving/reloading. Only successful append
clears the submitted draft. Discussion does not require a configured provider or
Factory target.

Runs and Results share the exact URL-selected `execution` request. Pending input
preparation remains distinct from admission and technical Run state. Selected Runs
reuse the existing authorized summary and expose only reported model/tool profile
references; missing profiles/Jobs remain Not reported. Declared clarification
responses open the exact advanced Run controls and retain their ordinary typed
contracts, Run preconditions and replay. There is no comment-to-gate translation.

Original Build/Attempt/Job links preserve `attempt_id`, `job_id` and Job log scope.
The existing Build view reads that exact retained Attempt, verifies its identity and
owning Build, and cannot substitute the latest retry or another Job when the target
is unavailable. Historical Attempt labels explain that Build controls still target
the current Build. Existing unselected Build URLs retain their current-Attempt
behavior.

Results uses the bounded Issue result contract and common safe Factory record
renderer for changesets, evidence, review plans, assessments, Decisions, delivery
and promotion. `result_collection`, `result_id` and optional `result_after` preserve
metadata-only historical navigation across sections and reload.
Selecting another execution clears the previous execution's `result_after` and
`result_id`; reselecting the same execution preserves both. Empty/final pages
retain Partial coverage; Stale/Unavailable remain explicit. Failed refresh retains
loaded data with stale feedback; failed continuations offer a fresh first page.
No partial collection or successful Build becomes overall Issue acceptance.

The shared read-only renderer, Run summary and styles live outside feature internals;
Issues and Factory reuse them without an import-policy exception or another
execution owner. Sections use explicit bounded reads and no new background poller.
English/Russian component/browser checks cover confirmation focus, plain text,
frozen lost-response replay, discarded drafts, historical selection and narrow
layouts. The [Console execution journal](#console-execution-journal) adds detailed
qualified event reads; PR/MR/CI and artifact expansion remain their separately
scheduled tasks. See [task 6.5 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-6.5.md).

## Qualified execution observations

OctaCity pins [Octa v0.6.0](https://github.com/OctaHive/octa/releases/tag/v0.6.0)
at source revision `3996594d5ff724d7b43888ac7db6df0b647e7d65`, including the
`public-command-file-observations.v1` capability alongside blocking tool authorization.
Task 6.6 qualifies the Codex process adapter's public command/file observations
through the existing runner event channel. The optional `progress.observation`
contract is owned by Octa's shared runner protocol and is provider-neutral.
OctaCity validates it before accepting Agent events and exposes only the typed
allowlisted metadata through `RunnerEventPayload::public_operation_observation`.
Other payload fields, prose, token claims, private invocation content and output
cannot enter that projection. Unsupported categories are absent/Not reported;
a reported start cannot claim completion or an exit status.

The existing fenced Job/attempt append preserves the original event sequence,
runner sequence/timestamp, Agent observation time and adapter operation time,
including replay after a lost acknowledgement. Existing run/step/command
references and exact Build/Attempt/Job log targets remain the navigation boundary;
metadata alone creates no log chunk or new content authority. No provider link,
new journal, migration, executor or Agent protocol version is required.

Reported input/cached-input/output tokens remain in the existing result/provenance
usage contract. Empty counters stay unknown; model token caching is separate from
Octa task-result cache events, and budget reservation is not measured usage.
Operation outcomes and model summaries grant no verification or acceptance
power. Command arguments, environment, prompts, transcripts, file bodies/diffs
and protected outputs are excluded from public operation metadata. The qualified
fixture runs the real plugin and runner around a controlled Codex CLI process;
it is not a live-provider qualification. The HTTP journal reader and
[Console rendering](#console-execution-journal) are described below.

See [task 6.6 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-6.6.md)
for the process fixtures, Red/Green outcomes and regression commands.


## Exact execution journal reads

Three read-only, no-store routes select an original execution request beneath
`/api/v1/projects/{project_id}/issues/{issue_id}/executions/{request_id}/journal`:

| Suffix | Selection | Response |
| --- | --- | --- |
| none | `collection` (default `flow-runs`), `after`, `limit` | Original request/Run observation and one common Factory diagnostic page |
| `/events` | Required `build_execution` digest and `job_id`; optional `after`, `limit` | Exact Job progress, safe typed events, sequence gaps and continuation |
| `/events/{sequence}` | Required original `build_execution` and `job_id` | One independently authorized original event body |

Collection and event pages default to 50 records and accept 1–100. Both use the
shared canonical opaque cursor codec, bounded to 2,048 bytes. Query parameters
are closed; detail accepts neither a bulk limit nor a continuation. OpenAPI
publishes these schemas and operation contracts for generated Console clients.

`ListIssueExecutionJournalQuery` composes the selected request projection with
`ListIssueResultsQuery`. Preparing requests have no associated Work and return
`records: null`; reads never publish or admit them. Associated requests retain
the original Work/Run and frozen requirement revision. Existing Flow, cycle,
Node Attempt, Build-execution, completion, stop and budget collections provide
the hierarchy, retries, original timing, settled usage and outcome references.
Each selected collection keeps the existing Run version fence and
partial/stale/unavailable coverage. Independent Run and collection observations
are not an atomic snapshot or an acceptance verdict.

Event reads validate that the selected Build-execution and Job belong to that
original Work, then reuse `IssueExecutionJobHandlers` and `JobEventLongPoll` with
zero wait. Current Issue, Factory, Project, Build, Job and Job-event visibility
are independently checked before event lookup or pagination. Progress includes
actual Job state/version/times, terminal classification, worker assignment and
optional frozen model/tool profile. Missing profiles and unassigned queued Jobs
remain absent. Log navigation retains the original Build/Attempt/Job, including
after retries, and grants no content authority.

`issue-job-event-sequence-v1` cursors bind the association, exact Build-execution,
Job, current security partition and normalized typed Job-event visibility.
Changing selection or visibility requires a new traversal. Each event retains
its original durable sequence and observation time. Returning the continuation
on an empty tail allows reconnect without duplicate rows. Missing retained
sequences are inclusive `gaps` ranges; expired content is never reconstructed.
An unavailable event store preserves Job progress and the incoming cursor, with
`coverage: unavailable` and no invented loss. If events advance beyond the Job
snapshot, `job_stale: true` marks that observation; it does not rewrite the Job
state. Future cursors beyond both the current Job acknowledgement and the retained
event-stream cursor are rejected. An already issued live cursor remains usable
while the earlier Job snapshot is stale.

Safe event facts include only qualified typed command/file observations,
Agent lifecycle changes and actual resource-usage samples. Operation rows keep
original run/step/command references and adapter observation time; command exit
status is not a verification result. Unsupported records remain sequence-bearing
`not_reported` rows with authorized log navigation. Samples retain milliseconds
and byte counters; nullable network counters distinguish missing accounting from
measured zero. They are not model token usage. Existing completion/stop/budget
records expose settled accounting separately from reserved limits; unreported
model input/cache/output breakdowns and costs cannot be inferred from progress,
prose, log text or an empty page.

Metadata excludes raw event payloads, arguments, environment, prompts,
transcripts, file bodies/diffs and protected outputs. Each event's `detail_url`
selects exactly its original sequence. Expansion independently repeats current
authorization and uses the existing event reader with limit one; a missing row
returns not-found rather than a later retained event. The linked common
`GET /api/v1/jobs/{job_id}/events` reader also returns no-store. Dedicated body
reads retain their existing raw-content permission, bounds and redaction policy;
they do not convert private content into generic journal metadata.

These projections add no persistence, provider calls, executor, publication or
status transitions. Manual Issue creation remains independent. See
[task 6.7 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-6.7.md)
for Red/Green evidence and restart qualification.

### Console execution journal

Runs retains the selected execution request across sections and reloads. Its staged collections use the exact execution journal reader, including nested Flow/cycle, Node Attempt, completion/stop and settled budget records. Original Jobs are selected by the retained Build-execution digest and Job identity; log navigation keeps the original Build and Attempt. Declared responses, retry and cancel continue through the existing exact Run controls and their current eligibility/preconditions.

The selected Job view follows bounded safe event pages using the opaque server cursor. It deduplicates reconnects, retains at most 512 visible rows, labels unavailable sequence ranges and stops at the consumed terminal cursor. Empty/error reads back off; hidden browser tabs stop interval reads. Pause/resume and restart affect this observation only. Expanding an event pauses live reads; deep-linked content is read independently even outside the visible event window. Updates retain DOM row identity and do not force scrolling. Dedicated event content is fetched only on explicit expansion, with independent authorization, `no-store` and zero query-cache retention after leaving the view.

Command/file facts, original event times, measured resource samples and terminal classifications are separate from model/tool profiles and worker assignment. Missing facts remain **Not reported**; resource samples do not manufacture model input/cache/output counters. The common Run budget view remains settled accounting, not a resource reservation or per-model measurement. Discussion still grants no execution or gate authority.

### Optional code-review application and storage contracts

Code reviews are separate from the optional primary ticket. An Issue may retain several PR/MR associations, identified by Project, Issue, installed namespace and stable native code-repository/review identities. Reader configuration pins remain original provenance; changing a display number, URL, title or task key cannot create authority or retarget the canonical association. Saving a manual Issue installs no review reader and performs no provider or Factory operation.

`IssueCodeReviewHandlers` composes the existing management authorization, opaque observation cursors and a narrow installed read-observation port. Repository metadata visibility is independent of the current protected read grant. Hidden repositories are filtered before membership watermarks, ordering and continuation; changing that visibility invalidates an old cursor. Revoking protected reads can leave already authorized retained metadata visible with **Blocked** availability. **Available** describes current read authorization, not a fresh provider response. GET-style queries serve retained observations and never call the observation reader; refresh is an explicit bounded read. Provider errors do not erase the last known facts or affect manual tasks.

Explicit attachment records the accepted management actor and exact reviewed Issue revision. Optional execution lineage requires the existing exact Issue/Work/Run association, retained producing record, independently visible code repository and matching candidate/head. Delivery provenance additionally requires the declared trusted `publish_candidate` action and its original retained publication receipt, native review identity and candidate facts. The installed reader must independently verify, without provider or credential I/O, that the producing action's frozen publication policy selects the same installation and native code repository. A candidate mapping or coincident review ID/head on another mirror is insufficient; missing policy mapping makes Delivery provenance unavailable. Receipt-shaped output from another action cannot establish Delivery provenance. These reads do not republish, merge, accept a candidate, transition Issue status or change execution state.

Normalized observations contain only optional safe title/number/navigation/visible-author, external state/draft, immutable head/base, available change counts and external review/check summaries. Unsupported facts stay absent. A navigation URL is bounded credential-free HTTPS without query or fragment; it is never a fetch target or native identity. Observations retain original timestamps and sequence. Latest facts are selected by observation time and retained sequence; replaying an older observation cannot rewind the head. Later heads never rewrite an association's original candidate, producing record or check lineage.

Migration `0090_issue_code_reviews.sql` adds independent scoped associations and append-only observations. PostgreSQL and memory use the ordinary Issue lock, scoped original receipts and shared atomic mutation publication. Association and observation continuations reuse the Issue history page ceiling and are bound to original membership, security partition and current metadata visibility; future positions/watermarks are invalid. Generic audit/outbox facts contain reference digests, without provider title/author/link bodies. Existing tasks, requirements, workflows, primary links and execution records are unaffected by migration or reads.

The 6.11 installed reader now implements this observation port through the existing verified Work adapter host and managed `RepositoryRead` grants. GitHub supplies bounded PR metadata and original-head check pages; a second fake MR vocabulary qualifies the same normalized contract. Current grants are rechecked after reads, without replacing original observation time. Native approval aggregation and complete check summaries remain unavailable. The [review reader protocol](protocols/work-source-reporter-v1.md#independently-installed-review-reader-v1) documents exact bounds, configuration and qualification limits. Review HTTP contracts are described below. Console review cards use the same retained contracts described below; no automatic review provider poller is installed.

### Review attachment and retained HTTP observations

The optional review surface uses the existing `IssueCodeReviewHandlers` and
installed reader. It is separate from primary ticket Source/Reporter management
and ordinary manual task creation.

| Method | Path below `/api/v1/projects/{project_id}/issues/{issue_id}` | Behavior |
| --- | --- | --- |
| `POST` | `/code-reviews` | Explicit reviewed attachment; requires `Idempotency-Key`, strong Issue `If-Match` and a closed `{selection, binding?}` body. |
| `GET` | `/code-reviews` | Bounded retained associations, last known facts and current protected-read availability. |
| `POST` | `/code-reviews/{review_id}/observations` | Explicit refresh of that original association; requires `Idempotency-Key` and an empty `{}` body. |
| `GET` | `/code-reviews/{review_id}/observations` | Bounded original per-head observation history. |

`review_id` is the canonical association digest, not a provider URL or display
number. `selection` contains the existing installed binding pins and exact native
code repository/review identities. They must resolve under current independent
read grants and metadata visibility. The existing integration/connection resource
contracts supply installed references; callers supply no credential, transport
endpoint, provider DTO, actor or provenance setter. Optional execution binding is
verified through the existing exact execution/candidate reader and local repository
mapping. Public attachment always records explicit operator provenance; it cannot
submit or manufacture trusted Delivery provenance.

Attachment requires Issue update permission. New attachment checks the current
reviewed Issue version before spending a provider read, with the final atomic
store precondition still protecting against an edit during that read. Stale new
requests conflict (`409`); missing `If-Match` returns `428`. The first successful
attachment returns `201`, and exact scoped replay returns `200` with the original
association even after later Issue edits or protected-read revocation, provided
current Issue/repository metadata remains visible. The existing accepted security
partition scopes replay keys; reusing a key for another Issue or selection conflicts
instead of retargeting the original receipt. Attachment does not increment the
Issue revision, move status, create Work/Run or grant merge/forge write access.

Refresh has no new mutable selection or Issue-version setter: its URI fixes the
original association, while scoped idempotency returns the original observation
receipt before another provider call. A new successful refresh appends facts for
the observed head without replacing original execution lineage. Provider failure
uses the existing safe dependency error (`503`), and a capability-unavailable outcome
uses the existing response (`422`); metadata-hidden references remain `404`.
Neither clears retained observations. Viewing metadata needs Issue view permission;
a read does not reserve
an execution, perform a refresh or call a provider.

Successful responses use `Cache-Control: no-store`. Both GETs accept only `after`
and `limit`, with the shared Issue history ceiling of 200 and common bounded
opaque observation cursors. Metadata visibility is applied before membership,
watermarks and paging. Continuation cannot cross Issues, associations, security
partitions or changed metadata visibility. Hidden references do not become counts
or protected metadata in a page.

The generated schemas distinguish association provenance, optional exact lineage,
retained observations and `available`/`blocked` protected-read availability.
`available` states current read authority, not provider connectivity or fresh data.
Use original `observed_at`, head and explicit refresh/error feedback when presenting
staleness; GET makes no fresh-provider-result claim. Title, number, safe HTTPS link,
visible author, external state/draft, head/base, counts and external review/check
summaries preserve unsupported facts as `null`. A merged state or passing external
check is informational and confers no Factory acceptance or Issue transition.

Server composition accepts these independent ports through
`ServerRuntime::start_with_issue_integrations` and `IssueRuntimeIntegrations`.
Existing `start_with_issue_sources` delegates to the same composition. An absent
review reader exposes no installed repository metadata and starts no provider
operation; manual tasks and an empty review collection remain usable. An installed
reader separately controls retained metadata visibility and current read grants,
so revocation can report `blocked` without hiding already authorized history.
Console review controls use these routes. Typed server-side Checks/Artifacts
inspection and its Console panes are described below. These routes
add no new credentials, schema migration or provider poller.


### Console review cards and explicit attachment

The card's compact **PR/MR summary** links to the same retained review collection
in **Results**, preserving the selected execution. Review identity and bounded
collection continuation are URL selections; opening a summary focuses that review
card. Attachment is available in Results even when the task has no ticket Source,
Factory target or Run.

Cards render only supported retained title/number, provider installation/native
repository, navigation link, visible author, state/draft, head/base, change counts,
external review/check summary and original observation time. Unsupported values
are **Not reported**, including unavailable counts rather than zero. Retained facts
never claim fresh provider connectivity: current `available` read authority still
shows the original observation time and an unconfirmed-freshness explanation;
`blocked` disables a new refresh and preserves independently visible metadata.

**Observation history** reads one bounded original-association page on demand.
An observation for another head is historical. An association's original execution
candidate remains visible and is explicitly historical when the current review
head differs; newer external checks cannot relabel that candidate. Provenance
Details distinguish trusted Delivery from explicit operator attachment and
organizational links without verified execution lineage. External merged/check
states imply neither a Factory Decision nor an Issue status move.

**Attach PR/MR** uses the existing bounded connection list, Project grant and
resource contracts. The form resolves the grant's exact retained connection
version before composing installation/capability/configuration/resource pins;
latest connection metadata cannot silently retarget an old grant. Only granted
`repository_read` choices are selectable. A generic installed connection is not
proof of an installed review reader; final capability/authority verification
remains on the existing server. The operator enters a bounded native PR/MR ID,
not an endpoint or credentials, then reviews the original task version and exact
native repository/review selection. The shared confirmation freezes body,
`If-Match` and idempotency key for transport replay. A changed Issue requires a
new review. Connection setup links to Integrations and adds no token form.

**Refresh PR/MR** is a separately confirmed observation command. Collection
navigation and history expansion perform retained GETs only. Refresh failures
keep last known facts with explicit stale feedback; failed retained GET refreshes
use the common stale-data notice. Loading, empty, blocked, unavailable and failed
sections remain independent of local task creation/editing and ticket Source
synchronization. Drafts and keys remain in component memory; no review-specific
browser persistence or automatic provider polling is added. English/Russian and
390 px keyboard journeys qualify selection focus, confirmation cancellation and
restoration, request preconditions and absence of horizontal page overflow.


### Subject-bound Checks and Artifacts HTTP inspection

All inspection is read-only. It reuses the original Issue/Work association,
independent Factory visibility, retained Flow journal and `FactoryArtifactReader`;
manual Issues require no Source, Reporter or installed execution capability.

| GET below `/api/v1/projects/{project_id}/issues/{issue_id}` | Selection and result |
| --- | --- |
| `/results` | With `collection=candidates`, required `work_id` and optional `after`/`limit`, returns original Candidate and ChangeSet publications through the existing version-fenced result reader. `collection=changesets` retains its original ChangeSet-only semantics. |
| `/results/checks` | Required `work_id` and `record`, where `record` is an exact retained candidate publication digest. Optional `after` and `limit` (1–100). Returns the original candidate, association, Run version and declared Factory checks. |
| `/results/artifacts` | Required `work_id`; optional `after` and `limit` (1–100). Returns independently visible frozen inputs and retained produced outputs. Metadata expands only for the requested page; lists never fetch bytes. |
| `/results/artifacts/{artifact_id}/content` | Required original `work_id` and explicit `mode=preview` or `mode=download`. Requires original inventory membership plus independent Artifact View and Download authority. |
| `/code-reviews/{review_id}/checks` | Optional `after`. Returns at most one original-head observation per page from the existing review history; each contains at most 100 normalized CI items, narrowed by the installed profile. GET never contacts a provider. |

All selectors are closed; no URL, provider endpoint, credential or status setter
is accepted. Cursors are opaque and bound to the owning task/selection, security
visibility and retained membership. Factory continuations additionally bind the
Run version. A concurrent Run change discards the page as `stale`; execution
state lost during a read is `unavailable`. Initial inaccessible selections return
the ordinary authorization/not-found/dependency failure before protected snapshot
reads. Coverage remains `partial` even at the last
page: none of these projections establishes overall Factory acceptance.

Factory Checks enumerate the frozen verification policy, including required
checks with no observation (`missing`). A retained active Node Attempt is
`running`; prepared input alone is `pending`. Confirmed technical stops retain
failure/cancellation. Only a verified typed report matching the exact frozen
input/plan can show pass/fail/indeterminate. An ordinary completed Build is
insufficient. Producer and record references remain original, with independent
Build/Job/Repository visibility; protected provenance becomes unavailable rather than a
successful summary. Observation time is historical, not a freshness promise.

External CI is explicitly `external_ci`, separate from Factory checks. An
explicit attach/refresh may retain one bounded original-head page through the
existing installed reader and its original total deadline. `has_more` means
further provider facts were not retained; absent details mean unsupported or
unavailable, and `required: null` means required-check policy was not established.
Skipped, neutral, unknown or stale outcomes never become pass. Provider content
URLs/cursors are omitted. New heads append observations without relabeling old
heads or associated candidates. Review summaries and metadata history do not
eagerly expand CI details; explicit refresh receipts may include them. Revoked
protected reads retain independently visible history with `blocked` availability.

Artifacts group frozen task/acceptance/specification references as
`original_input`, and original verified outputs as `output`, `report` or raster
`screenshot`. Project and Source inputs keep their actual publication origin,
without fabricated Build provenance. Produced references retain their original
record, candidate when known and Build/Attempt/Job producer. Safe metadata includes
logical name, media type, publication and exclusive retention deadline; immutable
references carry size/digest. Expired or unretained bytes remain expired/unavailable
references. Opening an item cannot recreate them.

Content is read only on explicit expansion through the existing bounded,
Project/content/retention-verified reader. Downloads are attachments with the
existing 64 MiB ceiling. Preview accepts valid UTF-8 plaintext/JSON/Markdown
rendered as plaintext, or PNG/JPEG/WebP that is fully decoded and normalized to PNG.
Preview has a 64 KiB encoded/output ceiling, 4096-pixel dimension ceiling and
64 MiB decoder allocation budget. HTML, SVG, scripts, other media and disguised
rasters are unsupported. Responses use `no-store`, `nosniff` and
`Content-Security-Policy: default-src 'none'; sandbox`; preview/download does not
return provider hotlinks or storage capabilities. Unsupported media types and
references exceeding the selected read ceiling return the documented
`422 capability_unavailable`; invalid or unavailable content returns the ordinary
read failure without bytes. Retention is checked again
before returning bytes. No upload or comment-attachment system is added.

These REST contracts are generated into the existing Console client. The Results
panes below use them without a provider client or a second result reader.

### Console Checks and Artifacts

Results keeps PR/MR cards and the existing Evidence and decisions view, with
URL-addressed Checks and Artifacts panes. Properties links open those same panes;
no compact summary fetches content. `result_pane` selects the pane. Factory reads
require the explicitly selected `execution`; no latest Run is chosen implicitly.
An admitted receipt preserves the executed requirements revision and warns when
the Issue has changed. A manual task without execution can still inspect its
independently linked external CI, edit requirements and create another task.

Factory Checks selects a visible retained publication from the bounded common
`candidates` collection, which includes both trusted `Candidate` capture/publication
facts and `ChangeSet` publications in the existing ordered, visibility-filtered
journal stream. The legacy `changesets` collection still returns only ChangeSets.
`check_record` retains its original producing record;
`candidate_after` and `check_after` page publication choices and checks separately.
The page displays exact candidate, required/missing and all reported check states,
original observation time and optional producer details. Build navigation retains
both `attempt_id` and `job_id`, so a later retry cannot substitute the producing
Attempt or Job.
A partial or exhausted page cannot establish overall verification or acceptance.
No provider title, model output or completed Build supplies a passing check.

External CI uses the same retained review membership read as PR/MR cards and one
independently pageable original-head observation per review. `ci_review_after`
pages associations; `ci_review` and `ci_after` retain the selected review history.
Older observed heads are explicitly historical relative to the review's latest
retained head; even that latest observation has unconfirmed freshness. Unknown
required policy, unsupported detail, truncated check lists and blocked read access
remain explicit. Reading CI performs no provider refresh, merge or Issue move.

Artifacts uses `artifact_after` for its own bounded metadata page. Original inputs,
outputs, reports and screenshots show safe names, media type, size, origin and
publication/retention times. A known `expires_at: null` means No expiry, distinct
from unavailable metadata; it is not shown as an unknown deadline.
Unavailable metadata receives a generic name; expired
or unavailable references offer no content controls. Details expose only the
original Artifact digest, candidate, producing record and actual Build/Attempt/Job
when known. Project inputs do not acquire fabricated Build provenance.

Preview and Download are explicit independent expansions through the existing
same-origin content endpoint. Text remains plain text, including markup-like
content; only server-normalized `image/png` becomes an image. HTML, SVG and other
preview representations have no renderer. The server owns preview/download bounds,
independent content rights, retention checks and raster validation. Downloads fetch
an authorized binary Blob and expose an explicit Save file link. Object URLs and
in-memory content are released on close/leave, and pending reads are cancelled;
content does not refetch on focus/reconnect or enter browser persistence or URLs.
No upload, comment attachment or provider hotlink was introduced.

Each page owns loading, empty, partial, stale/unavailable and retry feedback.
A failed refresh keeps the prior metadata visibly stale while disabling onward
paging and closing Artifact content expansions. Another pane remains usable.
Selecting another execution clears only its candidate/Artifact continuations,
while navigating sections or reselecting the same execution preserves the original
selection. English/Russian browser qualification covers keyboard activation,
390/1280 px layouts, safe text/raster preview, explicit download and reload.
See [task 6.15 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-6.15.md).

### Pure descriptions of exact operator actions

`FactoryNodeActionDescription::from_snapshot` describes an already authorized
retained execution snapshot at a trusted observation time. It uses the same pure
HumanGate eligibility contract and technical-retry validator as atomic Run
controls. A description reserves nothing and supplies no mutation authority.
Current visibility, time and Run version must be revalidated by consumers and
again by the existing command boundary.

Each description retains original Work/Run, observed Run version, root or nested
Flow Run, cycle, Node Attempt, pinned definition, node key and frozen input digest.
An optional candidate comes only from that input's typed frozen Context; a missing
candidate stays absent rather than selecting a newer head. Human responses retain
declared outcome/schema references and their inclusive deadline. Purpose-bound
delivery/promotion/escalation gates expose their exact operation/disposition
binding and only its mapped outcomes; they require the existing typed operation
control, not a generic response bypass. Private input, Context and response bodies
are not included.

Retry descriptions address the original failed attempt and use its retained stop
time. They exist only while the admitted retry allowance, remaining budget and
current cycle permit the existing retry command. They have no HumanGate response
deadline; the failed attempt's execution deadline does not invent a new retry
policy. Recorded intent is no longer unanswered before owner consumption.
Expired responses, cancellation, superseded cycles/attempts and terminal Runs do
not produce actionable descriptions.

`describe_issue_factory_actions` binds these descriptions to a supplied retained
Issue/Work association, rejecting a different Project, Work or Run. It does not
read Issue status, infer an action from a node name, select a latest execution or
replace the Run precondition with an Issue version. Parallel executions and nested
targets remain distinct; unlinked Work can still use the common description
directly, and a manual Issue requires none of these execution capabilities.

The descriptor lives in the backend-neutral store contract so direct application
inspection and persisted projections share one eligibility implementation; the
application re-exports the same public description types. See
[task 7.1 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-7.1.md).

### Durable action projection

`FactoryActionProjectionStore` materializes exact Run actions independently of
Issue creation, provider integration and execution ownership. Migration 0091 adds
one per-Run source-version checkpoint and safe action rows. Existing executions
start uncovered and remain available for ordinary execution and controls.

The authoritative Run version is the transactional invalidation fence: every
source-version change makes an older projection ineligible in the same source
commit. Execution/admission/control transactions do not write or depend on the
projection tables. Reads check current visibility and source version under the
Run lock; missing, invalidated or newer-than-observation coverage is explicitly
`Incomplete`, with no actions or continuation. An empty `Complete` page means
that this Run version was materialized and has no matching unexpired actions.
Expiry is inclusive and filtered before ordering, limits and continuation.

`FactoryActionProjectionWorker` selects at most the common diagnostic page limit
of missing or stale source versions, ordered by source update time and Run ID.
Publication locks and rechecks the original source, derives actions through the
common descriptor, and replaces the checkpoint and action rows in one transaction.
A stale source or older observation cannot overwrite newer work. An interrupted
transaction acknowledges nothing; a lost acknowledgement safely repeats. Committed
checkpoints survive worker/server restart, so backfill resumes without a second
outbox or an execution claim. Recorded responses/retries retire unanswered actions
at intent acceptance, independently of later owner consumption.

The server schedules this maintenance through its existing worker loop and
retention-worker polling/batch policy, including when the Factory owner is disabled.
Individual publication failures remain pending and do not stop other items in the
batch. The read-model task is supervised separately from execution failure policy:
a stopped/crashed index worker leaves uncovered versions readable as `Incomplete`
and cannot cancel Factory execution or fail its readiness. Source controls still
revalidate their own authority independently of this index.

See [task 7.2 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-7.2.md).


### Shared action REST and Console

The read-only resources use the same persisted action projection and eligibility
contract:

- `GET /api/v1/factory-actions` spans visible Projects and Runs.
- `GET /api/v1/projects/{project_id}/factory-actions` restricts the collection to
  one Project.
- `GET /api/v1/projects/{project_id}/issues/{issue_id}/actions` spans the original
  Work associations of one visible Issue, including parallel executions.
- `GET /api/v1/factory-runs/{factory_run_id}/nodes/{node_attempt_id}/action` reads
  the exact authoritative attempt. Optional `project_id` and `issue_id` selection
  checks ownership and the original Issue/Work association independently; an Issue
  selection requires its Project.

Collection pages accept `limit` (default 25, maximum 100) and a bounded opaque
`after` cursor. Current Run, Project and linked Issue visibility are applied before
coverage, count, ordering and pagination. Hidden linked Issues never become
unlinked Work. Count and rows share one repeatable-read database observation.
Ordering is by Run ID and node-attempt ID. Cursors bind the selected Project/Issue,
security partition and all current read scopes; a changed scope requires a new
first page. These are current collections, not frozen historical snapshots.

`total` counts matching indexed actions; `coverage: incomplete` explicitly warns
that uncovered source versions may add actions. Complete empty coverage means no
matching unanswered action at that observation. Expiry uses trusted server time,
not a query-supplied timestamp. All four resources return `Cache-Control: no-store`
and never repair an index, reserve execution or contact a provider.

Response descriptions include only the frozen data schemas referenced by their
permitted declared outcomes, plus the existing optional typed operation binding.
They omit gate inputs, Context, response values and operator reasons. Migration
0092 clears the older derived projection format so the existing worker rebuilds
these shapes from authoritative admitted definitions; it changes no Run or Issue.

The exact observation distinguishes `waiting`, `recorded`, `consumed` and
`historical`. `recorded_version` and `recorded_at` retain original acceptance
provenance without the command body. A recorded response becomes consumed when its
owner records completion; a retry becomes consumed when its corresponding new
attempt exists. Neither state means that an Issue was accepted. An independent
`accepted_candidate` fact comes from the existing exact-current-candidate Decision
summary and supplies the existing bound-operation form; it grants no merge or
publication authority.

Console exposes `/factory-actions`, an optional `project_id` selection, links from
Factory/Project navigation, and the same workspace in the card's Actions section.
Actions remain separate by exact Run and attempt. The observation cache also
binds the selected Project/Issue; target-wide invalidation refreshes every scoped
observation of that Run/attempt. Selection uses `action_run` and `action_attempt`;
paging uses `action_after`. Reload can recover recorded or
historical targets even after they leave the unanswered collection. Private form
values and replay identities stay in memory and never enter these URLs or browser
storage. Navigating between card/Project scopes discards drafts.

The workspace shares schema-driven response/operation forms and the original
response, request and retry clients with the Run page. It reads the selected
attempt again before review and confirms its strong Run version using the existing
`If-Match` and `Idempotency-Key` controls. Issue edits do not replace that version or
retarget another execution. Transport retry preserves the confirmed intent;
conflict/precondition failure requires refresh and a new review. Background reads
retain visible data with an explicit notice and disable new commands while stale.
Recorded targets refresh through the existing operator polling interval while the
view is open. Coverage alone never establishes command authority: existing guarded
Run commands always revalidate eligibility and concurrency.

Manual creation/editing and existing Run controls remain usable without a provider
or the optional action service. There is no separate close-action or approve-Issue
setter. See [task 7.3 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-7.3.md).


## Legacy Work migration and navigation

Migration `0093_issue_legacy_backfill.sql` freezes a pending set of visible,
unassociated Work at upgrade. It excludes already reserved Issue execution
publication. Ordinary manual and reviewed-request admissions keep their own
association path. The independent `issue-legacy-backfill` maintenance worker
processes the frozen set in bounded passes through ordinary Issue creation and
association transactions. Backfilled cards start at the selected Project
workflow's exact initial status, with `content_available: false` and no invented
requirements revision or execution request. Verified native provenance in the
same Project and security partition groups related retained Work; otherwise
frozen upgrade Work receives a separate Issue. Installations and security
partitions remain separate.

New Source intake keeps task import independent of execution. A newly settled
Source Work enters association recovery only when its verified native task has
an explicitly imported or attached primary Issue link. Execution-only Work
creates no card. A later explicit native import or attachment queues associations
for the original admitted Work/Run, including separate inputs, without another
admission or a history rewrite. The same Project fence covers either ordering;
the frozen upgrade membership remains unchanged.

Association maintenance does not derive business status from a Run outcome,
contact a provider, publish inputs, start a Run, overwrite task text, install a
mapping or synchronization, or send Reporter effects.

For ordinary file deployments, explicitly install the local action as described
in [Factory operations](../operations/factory.md#explicit-local-issue-status-actions).
It still requires a reviewed Issue execution association, a pinned workflow edge
permitting pipeline origin and current command authorization. Source-only
association deliberately has no reviewed requirements revision. The
[packaged qualification guide](../testing/source-runtime.md) records the separate
import, optional admission, explicit execution and reporting journeys.

`GET /api/v1/projects/{project_id}/issues/{issue_id}/legacy-work` returns bounded
original association metadata. Its opaque continuation binds the security scope,
Project/Issue, membership and current Issue revision; changed membership/revision
requires a fresh page. `GET .../issues/{issue_id}/work/{work_id}` reuses the exact
Work reader and independently authorized Run observation. Both are `no-store`
reads with no migration or admission side effects. Pending backfill remains
explicitly unavailable instead of causing a GET to create state.

The Console exposes these Artifact-based executions separately from reviewed
execution requests. URL selection `work` retains the original Work while changing
card sections. Original Factory Run navigation preserves existing controls/logs.
Selected Work reuses the card's stage/Job, event, result, Checks and Artifacts
components without inventing an execution request. Hierarchy/results use the
existing Work result contracts. Requestless event metadata and expansion use
`GET .../issues/{issue_id}/work/{work_id}/journal/events` and
`GET .../journal/events/{sequence}` with the same exact Build/Job selector,
independent rights, bounded cursor, redaction and `no-store` behavior as reviewed
requests. An expired history cursor retains an explicit first-page recovery action.
A Factory Run's optional `issue_association` is independently checked against
current Issue visibility; null means unavailable or pending, without proving that
no hidden association exists. The **Open task** link selects the original reviewed
`execution`, or legacy `work` when no request exists.

## Collaboration content lifetime

Requirements and discussion have separate explicit plans; no plan means no
scheduled body deletion. These administrative contracts work without Factory or
providers:

| Method | Issue-relative route | Contract |
| --- | --- | --- |
| GET | `/content-retention/{retention_kind}` | Read one plan, or version `0` and `plan: null` |
| POST | `/content-retention/{retention_kind}` | Review an exact current plan version, cutoff, deadline and hold |

`retention_kind` is `requirements` or `discussion`. Both routes require current
Issue **Administer** authority and use `Cache-Control: no-store`. POST requires
`Idempotency-Key` and `{expected_version, plan}`; the plan version is independent
of the requirements revision. A plan contains
`scope: {kind, through}`, `expires_at` (Unix milliseconds), and nullable `hold`.
The path and scope kind must match. Requirements cutoffs refer to existing Issue
revisions; discussion cutoffs refer to positive allocated sequences up to the
1,000 lifetime limit. Later edits/comments are excluded. Each kind has independent
CAS, deadline and hold, so deleting discussion cannot release a requirements hold.

A hold preserves bodies still present. Its bounded private operational `reason`
uses the common 128 UTF-8 byte text ceiling; `expires_at: null` means indefinite.
Replacing/releasing it is another explicit reviewed administrative command.
Reasons are redacted in diagnostics and omitted from generic audit/outbox facts.
The deployed management mode remains trusted-network; no authenticated person is
invented by this policy.

The independent `issue-content-retention` worker uses the existing bounded
maintenance polling/batch policy, including with Factory disabled. It removes at
most the configured revision/comment body batch from one due lifetime per pass.
Issue locking and plan rechecking fence concurrent edits/hold changes. Cleanup
makes the revision's `content_available` false, replaces its text with an
unavailable representation and redacts matching original Issue mutation receipts
in the same transaction. Comment body reads become unavailable while sequence,
actor/time/reply metadata, original replay and allocated quota remain. Migration
`0095_issue_receipt_retention_index.sql` gives each revision batch an indexed
Project/Issue/version lookup instead of scanning unrelated replay history.
A repeated
comment cannot restore bytes or replenish its lifetime quota. Cleanup does not
advance Issue/applicability versions or status history. Later explicit requirements
edits create available new revisions; they do not refill expired historical ones.

Issue/workflow/status/mapping versions, source-link/observation digests, execution
requests and associations remain referenceable. Original source snapshots and
published execution/Artifact bytes retain their existing cleanup/hold owners and
deadlines. This plan neither extends nor deletes those bytes and cannot recreate
an expired execution input. Derived shared-action rows are purged through the
existing Factory retention pass, with child rows before their checkpoint.

The Console renders unavailable current and historical text explicitly in English
or Russian. It retains the human task key and original execution navigation;
unavailable acceptance is never presented as a newly empty draft. These plans are
administered through REST; no new retention editor is added to the task workspace.

Capture/restore the complete database and original object-store consistency unit;
see [backup and restore](../operations/backup-restore.md#task-and-collaboration-recovery).
The [section 8 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-8.md)
records Red/Green, PostgreSQL/browser pilots and remaining live-adapter limits.
