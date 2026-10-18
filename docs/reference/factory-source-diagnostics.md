# Factory source preview and diagnostics

These protected management routes use ordinary application authorization and
`Cache-Control: no-store`. Deployments explicitly install read services with
`IntegrationManagementApplication::with_source_reads`. Preview additionally needs
`FactorySourceConfigurationHandlers::with_preview` and verified installed Source
profiles through `HostedFactoryWorkSourcePreview`. Managed inputs resolve their
own current connection/resource/read grant; they cannot borrow a legacy profile.
No service is enabled just because a provider is named GitHub.

## Preview a reviewed input or exclusive group

`POST /api/v1/projects/{project_id}/factory-sources/preview` accepts a closed
`{"intent": <WorkSourceConfigurationIntent>, "limit": 1..100}` document. It needs
no publication precondition or idempotency key and does not publish the draft.
Group members pin exact input versions; members omitted from the draft are loaded
from those immutable definitions. Disabled inputs can be inspected. Publication,
current target validation and actual admission remain separate operations.

The response preserves the submitted publication intent, including references to
unchanged group members; expanding those members for provider reads does not turn
them into input replacements. The returned configuration can be previewed again
and passed to validation under the same version preconditions. The response also
includes exact selector bindings and safe installed predicate descriptions. An absent predicate description means
**unknown**, not unrestricted. Each native candidate carries safe summary facts
and per-input predicate reasons. `filtered`, `selected` and `ambiguous` describe
selection only. A match does not promise a new Run or prove admission eligibility.
Visible `admissions` carry original Work/Run and input/version references; an
existing association remains an existing execution even when it matches again.
Automatic replay continues to use the original input-scoped admission authority.

`failure: null` with no candidates means a successful empty read. `rate_limited`,
`unavailable`, timeout and other closed dependency failures remain distinct and
may include bounded `retry_after_millis`. Preview does not schedule that retry.
`has_more` means the provider listing exceeded this single bounded read. No
provider continuation or production cursor is returned. Visible admission lookup
is independently bounded to 64 rows across the page; `admissions_truncated`
means the lookup did not exhaust visible history. An empty/truncated lookup is
never proof that the task has no prior or hidden execution.

## Inspect retained progress and reporting

`GET /api/v1/projects/{project_id}/factory-source-inputs/{input_id}/versions/{version}/diagnostics`
and the corresponding `factory-source-groups/{group_id}` route accept `limit`
(1..64) and an optional `after` cursor. Cursors bind the Project and exact target
version. Dependency and Run visibility are applied before ordering, limits and
cursor calculation; hidden rows produce no count, cursor or truncation hint.

Diagnostics include immutable definitions, installed predicate descriptions,
last scan acquisition time, covered window/reconciliation, latest selection and
retained retry/failure/exhaustion facts. Owner/fence proofs, private cached pages,
continuations, raw task bodies and credential contents are excluded. A selected
observation is not evidence that admission completed: only a settled authoritative
Work/Run association proves that. A later definition edit does not rewrite the
original input version or execution association.

Status projections are read from accepted actions in the existing Run/outbox
journal and only for the original source Reporter's exact capability. `pending`
and `claimed` are not applied effects; `applied` means delivery was confirmed;
`exhausted` means further automatic dispatch is blocked. Reporting health is
independent of task business status and Run execution. Each summary includes at
most 64 operations, with `reporting_truncated` if more visible operations exist.
The read uses operations registered atomically with accepted actions and fetches
only their latest bounded outbox metadata. It does not load full Run snapshots or
take execution row locks. Reading diagnostics neither starts nor retries a projection. Disabled/revoked
inputs retain inspectable history when its dependencies and Runs remain visible.

## Native identity and optional internal tasks

`native_correlation` is a Project/security-partition/installation/native-container/
item digest. It excludes input identity, definition version, title, labels and
observation time. It groups visible source-stage executions; it is **not** another
Work replay or deduplication authority and is not an internal Issue id. The native
task container is independent of the code Repository used for execution.

`FactoryWorkSourceIssueLinks` is an optional read-only capability of the future
authoritative task owner. It must apply current task visibility. Factory accepts
only a Project-matching reference and omits absent, hidden or unavailable links.
Missing cards do not block native/Work/Run diagnostics. No text matching, guessed
card, import, revision, association write or synchronization intent is performed.
The preview provider capability has no observation-consumer or task-write port;
an enabled task observation owner receives only an explicit read lookup here.

Persistent task creation, refresh, manual Issue execution and field ownership
remain in `add-factory-work-card`. Existing manual Artifact-based Factory intake
requires no connection, Source capability or internal Issue. Source/reporting
services hold no authority to change that path. UI editors and guided presentation
are separate tasks; these routes provide their authorized data contracts.

The separate [optional observation port](work-source-observations.md) supplies
private current snapshots to explicitly enabled consumers. Preview/diagnostic
routes do not invoke it or advance a consumer's refresh progress.

## Stage and action provenance

A reporting operation can include optional `origin` metadata. `origin.action`
identifies its accepted trusted-action record; `origin.stages` identifies only the
exact records explicitly consumed by that action. Each record reference includes
its immutable definition, Flow invocation, cycle, node, declared selected outcome
and node-attempt identity. This distinguishes identically named nested stages and
links to the original Run attempt without exposing payloads or Context.

The same metadata accompanies pending, claimed, applied and exhausted reporting;
readers need not wait for the enclosing Run to finish. Empty stage references mean
no stage data was explicitly bound, not an inferred business outcome. The UI keeps
these reporting facts distinct from task business status and execution results.
Existing consumers can ignore the optional metadata, and Work/Run navigation stays
available without internal task cards.

PostgreSQL projects origin from existing immutable action/source records alongside
outbox state. This remains a read-only query with no Run-row write lock, provider
request, task refresh or new progress journal.
