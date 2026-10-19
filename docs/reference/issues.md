# Internal Issue application contracts

An Issue is a persistent Project-owned task. It can exist as a manual draft
without a provider, credentials, repository, Factory configuration or execution.
Its business status does not establish a Factory Decision or execution permission.

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
Delivery, source association/mapping publication and accepted automated command
provenance belong to the later source/action slices; this contract adds no
second Reporter, polling loop or execution journal.

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
