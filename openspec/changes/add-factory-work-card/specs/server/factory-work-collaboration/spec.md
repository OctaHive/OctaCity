## Purpose

Defines persistent provider-neutral tasks with editable revisions, exact Factory execution history, optional external synchronization, operator discussion, and evidence-bound actions without replacing existing execution authority.

## ADDED Requirements

### Requirement: Internal Issues exist independently of execution and providers
The server SHALL provide a Project-owned Issue with a stable internal identity, unique Project-local display key, bounded title and description, optional acceptance input, configured business metadata, and immutable content revisions. Authorized operators SHALL be able to create and edit manual Issues without an external connection, repository, model, or Factory Configuration. Editing SHALL require the observed Issue version and scoped replay; exact replay SHALL return the original outcome and changed reuse SHALL conflict. Saving an Issue SHALL NOT start execution or require invented acceptance criteria. Task business metadata SHALL NOT constitute a Factory Decision or execution permission.

#### Scenario: Operator creates a manual draft without integrations
- **WHEN** an authorized operator supplies valid task text in a Project without a provider or Factory Configuration
- **THEN** one Issue is created with a stable key and initial revision, no Work or Run is admitted, and incomplete execution prerequisites remain explicit

#### Scenario: Two operators edit the same revision
- **WHEN** both submit different edits against the same observed Issue version
- **THEN** at most one new revision commits and the other receives a conflict without overwriting content or creating execution

### Requirement: One Issue retains multiple exact executions
An Issue SHALL retain zero or many explicit execution requests and associated immutable Work Envelopes in its owning Project. Each Work SHALL remain associated with at most one Issue and exactly its existing Factory Run; legacy unassociated Work SHALL remain valid. Each execution SHALL identify its exact requirements revision or legacy input provenance, configuration, repository/base, mapping, and existing Run history. A node retry or requirements cycle SHALL remain within its original execution, while a later intentional processing request SHALL use a distinct admission identity. Multiple active executions SHALL be represented individually rather than collapsed into one agent or outcome.

#### Scenario: Assessment and implementation use different pipelines
- **WHEN** independently authorized inputs admit the same linked external task to assessment and implementation
- **THEN** one Issue lists both Work/Run associations without changing either input's deduplication identity or introducing a second Run for a Work

#### Scenario: An earlier node is retried
- **WHEN** an eligible infrastructure retry is accepted through existing controls
- **THEN** the Issue shows that retry in its original Run history and creates neither another Issue nor another execution request

### Requirement: Starting execution freezes a reviewed Issue revision
An authorized execution request SHALL bind the reviewed Issue version and content digests to an exact visible Factory target, repository/base policy and versioned task/acceptance mapping. It SHALL validate ordinary admission authority and required inputs, publish immutable Project-owned inputs through the shared recoverable intake boundary, and admit Work through existing scoped replay. Publication, admission and Issue association SHALL recover to the same logical request after restart or lost acknowledgement. A request SHALL NOT be reported admitted before its exact Work/Run association is durable. A later Issue edit SHALL NOT alter queued or running Work, Context, profile, priority or retries.

#### Scenario: Requirements change during execution
- **WHEN** an operator edits an Issue after its first execution is admitted
- **THEN** the Issue exposes the new revision while the first execution retains its original input and is labeled as using that earlier revision

#### Scenario: Admission committed before association acknowledgement
- **WHEN** a request is retried after its admission committed but its association or response was interrupted
- **THEN** recovery attaches and returns the original Work/Run without another publication identity, base resolution or admission

#### Scenario: Acceptance input or reviewed version is invalid
- **WHEN** the selected pipeline requires missing acceptance input or the reviewed Issue version is stale
- **THEN** the request is rejected with an actionable validation/conflict result and no silently updated or fabricated execution input

### Requirement: Native source association is stable and scoped
An Issue SHALL have zero or one primary external source link in the initial release. Links SHALL use trusted provider installation, native resource and native item identities within the owning Project and accepted security partition, independently of display URLs, names, numbers, credentials and pipeline input identities. Concurrent imports SHALL resolve one Issue for that key. Explicit attachment of an existing native task to a manual Issue SHALL require current visibility, authorized source access and ownership review; an existing association to another Issue SHALL conflict without automatic merge or hidden-target disclosure. Provider-specific wire objects SHALL remain outside the Issue domain contract.

#### Scenario: Two inputs discover one native issue concurrently
- **WHEN** two authorized inputs in one Project observe the same stable native task
- **THEN** they resolve one internal Issue while retaining independent admission and observation provenance

#### Scenario: Connection name or credential changes
- **WHEN** an authorized connection is renamed, rotated or adopted without changing its native installation/resource/item identity
- **THEN** the existing link remains attached and no duplicate Issue or Work is created

#### Scenario: Manual Issue is attached to an already linked source
- **WHEN** an operator tries to attach a native task already associated with another visible Issue in the Project
- **THEN** the command reports a safe conflict and the canonical visible association without merging histories

### Requirement: Source import and refresh do not imply execution
Installed authorized Source capabilities SHALL be able to create and refresh internal Issues without a configured pipeline target. Synchronization SHALL refresh retained linked tasks independently of their current match against execution-input selectors. Import/refresh SHALL validate native identity, observation provenance, content bounds and ordered or reconciled freshness; repeated unchanged observations SHALL create no new content revision, and stale or uncertain observations SHALL NOT overwrite a newer accepted revision. Source text and native labels/state SHALL remain data rather than lifecycle commands. Existing enabled execution inputs SHALL retain their separate configured admission semantics.

#### Scenario: Linked issue no longer matches the intake selector
- **WHEN** its labels change so that it no longer matches its original execution input while authorized link refresh remains enabled
- **THEN** its internal card can refresh without a new execution or removal of retained history

#### Scenario: An older observation arrives late
- **WHEN** a source returns an observation older than the accepted version or cannot establish its freshness
- **THEN** the system retains current content and classifies or reconciles the observation instead of applying arrival-order overwrite

#### Scenario: External text changes while a Run is active
- **WHEN** a fresh authorized observation changes source-owned requirements
- **THEN** a new Issue revision is retained while the active execution snapshot stays unchanged and no edit-triggered Run is created

### Requirement: Synchronization uses explicit capability-supported field ownership
Every source link SHALL retain an exact versioned mapping and explicit pull/push settings. A managed field SHALL have one declared authority and direction: source-owned pull, OctaCity-owned push, or unsynchronized. The mapping SHALL validate supported fields, finite values, managed native effects, scope and independent read/write grants before activation. Source-owned fields SHALL reject local replacement while linked; local-only fields SHALL remain editable. Unsupported provider operations SHALL be reported unavailable rather than silently ignored or advertised as full bidirectional synchronization. GitHub, GitLab, Jira and other provider names SHALL have no intrinsic task lifecycle meaning, and absent adapters SHALL not block manual Issues.

#### Scenario: Operator selects an unsupported remote field write
- **WHEN** a mapping requests a field or direction the installed Reporter does not support
- **THEN** validation rejects that mapping without starting synchronization or inventing a provider operation

#### Scenario: Operator edits source-owned description locally
- **WHEN** a linked Issue's mapping declares its description source-owned
- **THEN** local replacement is rejected with the ownership reason while unrelated local metadata remains editable

#### Scenario: Deployment has no native GitLab or Jira adapter
- **WHEN** it uses manual Issues or another installed provider
- **THEN** those tasks and executions remain usable without claiming that the absent adapter is available

### Requirement: Accepted updates use durable reporting without changing execution authority
Outbound synchronization SHALL originate only from a separately authorized Issue metadata command or an accepted configured Factory action. It SHALL retain exact Issue/source/mapping and originating command or Work/Run/action provenance, preserve unmanaged external fields, and use existing durable Reporter ordering, fencing, unknown-response observation and bounded reconciliation. Observing a pull echo or replaying a receipt SHALL NOT create a new logical action or execution. Unreconcilable concurrent changes SHALL become explicit conflicts; resolution SHALL require a reviewed preconditioned desired state or policy change within current grants. No comment, raw model answer or native label SHALL directly select an execution transition, claim acceptance or widen authority.

#### Scenario: Triage completes before the provider recovers
- **WHEN** its accepted configured action records a business result while the external system is unavailable
- **THEN** the result remains retained, reporting is pending or exhausted under its bounds, and the Issue does not mark the successful execution failed

#### Scenario: Remote write acknowledgement is lost
- **WHEN** the Reporter may have applied a managed update but its response is lost
- **THEN** recovery observes and reconciles the same desired effects without another logical action or Run

#### Scenario: Source association or mapping changes after execution intent
- **WHEN** a source is attached after an unlinked execution started or a reporting mapping is replaced while an operation is pending
- **THEN** the card does not retroactively authorize that execution to report to the new link, and committed operations retain their original native target/effects subject to current grant checks

#### Scenario: Remote managed field diverges concurrently
- **WHEN** an observed remote edit cannot safely converge under the retained authorized mapping and ordering policy
- **THEN** the link reports a conflict and requires a new reviewed resolution rather than silently broadening or replaying an obsolete write

### Requirement: Task, execution and synchronization states remain independent
Issue reads SHALL separately expose configured business metadata, every current execution's technical and verification state, and link synchronization health/freshness. Current AI profile/model and enrolled worker identity SHALL be shown only when available and independently visible; queued, parallel and historical work SHALL remain explicit. Provider throttling, inaccessible/deleted remote tasks, disabled connections and revoked grants SHALL preserve local task/history and perform no unauthorized provider fallback. No latest-completed-Run shortcut SHALL establish overall task acceptance.

#### Scenario: Two executions are active and one awaits a worker
- **WHEN** an Issue has parallel authorized processing and only one execution owns an assigned Job
- **THEN** its card identifies each execution and its actual available executor/profile facts without inventing a single active assignee

#### Scenario: Provider access is revoked
- **WHEN** the next synchronization operation would require a revoked read or write grant
- **THEN** no corresponding provider call occurs, the link reports blocked synchronization, and local task reads/edits and unrelated manual execution remain usable

### Requirement: Execution history and checks remain evidence-bound
The card SHALL expose bounded execution and result views identifying exact Issue revision/request, Work, Run, Flow Run, cycle, Node Attempt, Build/Attempt/Job, candidate, evidence, assessment and Decision references as applicable. It SHALL distinguish technical completion, verification pass/failure, pending or missing branches, indeterminate evidence, superseded candidates and accepted/rejected Decisions. Model summaries, task metadata, comments, stale results and one successful Build SHALL NOT establish acceptance. Authorized log access SHALL reuse existing Build/log contracts and SHALL NOT expose raw provider transcripts or hidden reasoning through the task timeline.

#### Scenario: Old execution passed a check
- **WHEN** another execution or candidate requires new verification
- **THEN** the old pass remains historical with its exact subject and cannot satisfy the newer gate

#### Scenario: Required evidence is missing or indeterminate
- **WHEN** a required report is absent, expired, failed or indeterminate
- **THEN** the card reports that condition and cannot label the required check successful

### Requirement: Discussion is bounded append-only operator data
Authorized management clients SHALL be able to append plain-text comments and one-level same-Issue root replies with stable identity, monotonic Issue-local sequence, server timestamp, actual management-actor provenance and content digest. Limits SHALL be at most 16,384 UTF-8 bytes per nonblank body and 1,000 lifetime comments per Issue or stricter deployment bounds. Scoped replay, sequence allocation, safe audit and publication SHALL commit atomically. Exact replay SHALL not append another comment; changed reuse SHALL conflict. Recognized credentials/private-capability patterns SHALL be rejected before publication. Generic operational/card/queue payloads SHALL contain metadata only, and no external comment synchronization or attachments SHALL be implied.

#### Scenario: Operators comment concurrently
- **WHEN** two permitted clients append different comments
- **THEN** both can receive distinct ordered identities without changing an execution version/budget or overwriting another body

#### Scenario: Reply targets another Issue or nested reply
- **WHEN** its parent is not a retained root comment in the same visible Issue
- **THEN** the command rejects before publication without revealing a hidden parent

#### Scenario: Lost comment response is replayed
- **WHEN** the exact authorized append is retried after restart
- **THEN** the original logical comment result returns without another sequence allocation or audit mutation

### Requirement: Comments cannot become Factory authority
Discussion SHALL remain outside immutable execution inputs, Context Manifests, provider sessions, evidence and control intent. Reading or appending comments SHALL NOT invoke a model, start/retry execution, reserve WIP, grant approval, or choose a transition. Clarification SHALL enter execution only through an independently confirmed schema-valid response to an existing declared human gate, consumed through the existing owner contract. No discussion thread SHALL automatically enter a later model's Context.

#### Scenario: Comment mentions an agent or asks for approval
- **WHEN** its text asks an agent to retry, approve or deploy
- **THEN** it remains operator discussion and creates no execution/control intent

#### Scenario: Operator answers a waiting clarification
- **WHEN** the operator separately confirms the exact current gate's declared response
- **THEN** the ordinary control command records immutable intent and the Flow owner later consumes it without importing the whole discussion

### Requirement: Decision queue items derive only from current eligible Factory intent
The server SHALL expose shared visible Project/global queues and Issue-scoped actions across associated executions. Each item SHALL retain exact Work/Run/Flow/cycle/attempt/contract and candidate bindings, allowed responses, actionable time, deadline and observed Run version, plus Issue identity when linked. Eligibility SHALL derive from current authoritative human gates, bound operation requests and permitted infrastructure retries, never names, comments or Issue status. Reads SHALL reserve no authority; resolution SHALL use existing idempotent preconditioned controls, not an Issue-state or queue-close setter.

#### Scenario: Two Runs need different human responses
- **WHEN** two executions under one Issue each expose a declared current gate
- **THEN** the queue shows separate exact targets and cannot apply one answer to the other

#### Scenario: Generic response attempts a bound operation
- **WHEN** a client tries to bypass a delivery/promotion/escalation contract through a generic response
- **THEN** the existing control boundary rejects the intent without a queue-specific bypass

### Requirement: Queue freshness and command outcomes remain distinct
Action queries SHALL identify observation/coverage, filter expiry/cancellation/supersession and current source versions, and distinguish waiting, recorded intent pending owner, consumed and historical results. Delayed projections SHALL not present stale actions or authoritative empty coverage. Mutation eligibility SHALL be revalidated independently. Competing fresh responses SHALL admit at most one intent and winning exact replay SHALL survive later progress. Issue edits SHALL NOT substitute for a Run precondition or retarget a pending action.

#### Scenario: Response is recorded before owner completion
- **WHEN** a guarded response commits while owner consumption is pending
- **THEN** it is no longer unanswered and the card shows recorded intent rather than a completed node

#### Scenario: Queue maintenance is stopped
- **WHEN** its derived projection cannot catch up
- **THEN** coverage is incomplete, stale rows are not actionable, and existing Factory execution/direct controls continue

### Requirement: Collaboration reads and retention preserve bounded visibility
Issue, revision, execution, result, discussion, synchronization and action reads SHALL apply current visibility before projection, counts, ordering and cursor generation, use deterministic bounded pagination, and identify observation freshness. System timeline entries SHALL contain safe facts/references rather than private inputs, prompts, provider traces, findings, secrets or transfer URLs. Dedicated sensitive-body/log reads SHALL retain independent authorization. Issue content, discussion, associations and pending intents SHALL participate in backup/restore and explicit retention/holds without changing Work evidence retention or resurrecting expired bodies through replay or backfill.

#### Scenario: Hidden Issues sort around a page boundary
- **WHEN** matching hidden Issues or executions precede visible results
- **THEN** page metadata, ordering and cursors expose only authorized matching records

#### Scenario: Retained discussion or requirements expire
- **WHEN** unheld content reaches its retention boundary
- **THEN** authorized reads/replay report unavailable bodies while required identity/provenance metadata remains and no logs or provider session recreate the content

### Requirement: Legacy Work remains compatible through resumable association
Backfill SHALL preserve existing Work/Run IDs, admission replay, input/evidence provenance and direct controls. It SHALL group legacy Work only by verified equal native identity within one Project/security scope, otherwise retain a separate Issue per Work. It SHALL not fabricate requirements revisions for legacy execution, make provider calls, enable synchronization, admit Work or emit external effects. Interrupted/repeated backfill SHALL converge to the same associations and expose honest coverage. GET requests SHALL not perform backfill or create task state.

#### Scenario: Legacy Work has no reliable external association
- **WHEN** it is backfilled without trusted native identity or retained text
- **THEN** it receives its own Issue with explicit unavailable content, retains exact original Artifact inputs, and is not merged by title or external display label

#### Scenario: Collaboration is disabled
- **WHEN** Issue/card/sync facilities are unavailable or disabled
- **THEN** existing Artifact intake, Factory/Build discovery, direct controls and ordinary CI/CD remain usable
