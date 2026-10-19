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
