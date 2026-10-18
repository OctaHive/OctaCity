## ADDED Requirements

### Requirement: Task navigation and manual creation precede execution setup
The Console SHALL provide a task-oriented list and manual-create entry point within the existing product shell. The list SHALL show human-readable keys and safe titles with bounded Project/business-status/priority/source/execution/synchronization filters. Manual creation SHALL ask for title, description and optional acceptance/metadata using generated bounds and SHALL work without an integration, repository or Factory Configuration. Saving SHALL open the canonical Issue card without starting execution. Missing execution prerequisites SHALL be explained at the explicit start action rather than blocking task creation.

#### Scenario: First task is created in an unconfigured Project
- **WHEN** an operator creates a valid manual task before configuring Factory or providers
- **THEN** the task appears under its human-readable key and opens its card with no Run and a clear execution-setup entry point

#### Scenario: Provider is unavailable
- **WHEN** an external source cannot be reached
- **THEN** local task navigation and manual creation remain available and affected linked tasks show their scoped synchronization condition

### Requirement: Project status settings and card moves are guided and independent
The Console SHALL expose bounded Project workflow settings for editable status names/order, initial/organizational terminal state and permitted transitions/origins with supported evidence requirements, using generated authoritative choices and reviewed immutable publication/default selection. Manual creation SHALL work with the provided editable default without setup. Existing cards SHALL show their pinned version and require explicit reviewed compatible status correspondence for workflow adoption. Card controls SHALL enumerate only server-authorized organizational moves, explain ownership/evidence/applicability restrictions and confirm the current Issue/status precondition. History SHALL identify before/after status, reason and actor or pipeline/source provenance. A status move SHALL NOT be presented as execution start, gate approval or verification success. Stale intents SHALL require fresh review and preserve in-memory context without automatic resubmission.

#### Scenario: Project uses its own status vocabulary
- **WHEN** an operator publishes custom statuses and edges without a provider or configured pipeline
- **THEN** tasks use those settings through the same manual-create/move/card UI and no label spelling or phase name is required

#### Scenario: Old execution cannot move the current card
- **WHEN** the server classifies its configured action as stale or conflicting after a newer revision or status intent
- **THEN** the card retains its exact historical result and shows the unapplied transition rather than moving the card or relabeling the Run result

### Requirement: Pipeline Issue-status actions and native mapping have separate editors
Existing Flow authoring SHALL allow an authorized operator to select an installed Issue-status action on a declared outcome and review the exact workflow/version/edge and supported evidence, independently of source-update actions. Source-link mapping review SHALL separately show internal status IDs/names, native supported values/effects and field authority/direction; it SHALL not edit Project workflow structure. The UI SHALL explain unmapped/ambiguous observations, forbidden edges and overlapping producers, and offer only installed supported operations. A local action/manual task SHALL require no connection, token or external mapping. Completion without a configured status action SHALL display only execution progress. Both surfaces SHALL preserve generated bounds, stale-version review, keyboard/focus, English/Russian strings and in-memory-only drafts.

#### Scenario: Operator configures Triage business outcomes
- **WHEN** separate accepted eligible and declined branches select different explicit Issue transitions
- **THEN** review shows Ready versus Declined as configurable examples and no automatic Done move is attached to Run completion

#### Scenario: Business status is saved while native update is pending
- **WHEN** a configured local move succeeds and its external push is retrying
- **THEN** the UI separately shows the current task status, exact execution/action outcome and pending synchronization with no false rollback or verification claim

### Requirement: Canonical Issue card presents task execution and source state separately
The Console SHALL provide one URL-addressable Issue card with Overview, Activity, Runs, Results and Actions sections, exact historical selection, and links to original inputs and existing advanced Run/Build/log views. It SHALL separately display task business metadata, all active executions/current steps, available AI execution profile/model and assigned enrolled worker, and source link/synchronization health. Queued/unassigned/parallel/unavailable facts SHALL be explicit. Editable requirements SHALL show their revision and source ownership; no task-status edit SHALL be presented as verification acceptance or a Factory state setter.

#### Scenario: Two pipelines have processed the same Issue
- **WHEN** the operator opens the task after assessment and implementation executions
- **THEN** both Runs remain visible in the same card with exact input revisions, individual outcomes and separately accessible logs

#### Scenario: Operator selects a historical result
- **WHEN** its URL identifies an earlier execution/candidate
- **THEN** refresh preserves that historical selection and does not label it current proof or an actionable current request

### Requirement: Issue edits and execution start are distinct reviewed commands
The Console SHALL support preconditioned revision editing and a separate pipeline execution action. The start form SHALL select visible authorized targets, show the exact reviewed requirements revision, input mapping and resolved configuration/repository choices, and explain missing acceptance or execution prerequisites. Submission SHALL use a frozen confirmed intent and show pending input preparation/admission until the exact Work/Run link is returned. Transport retries SHALL preserve its original request. A later deliberate execution SHALL create a new request while node retry uses existing Run controls. Unsubmitted sensitive values and request keys SHALL remain in component memory only.

#### Scenario: Requirements change while execution runs
- **WHEN** an operator saves a new task revision
- **THEN** the card shows that revision and labels active execution as using the previous snapshot without restarting or modifying it

#### Scenario: Execution start loses its response
- **WHEN** a confirmed request's transport acknowledgement is lost
- **THEN** the Console resumes the same request/status receipt and opens its original Run without a fresh command identity

#### Scenario: Reviewed version is stale
- **WHEN** the server rejects an edit or start request after another revision committed
- **THEN** the Console preserves the in-memory review context, shows the conflict and requires fresh review instead of silently substituting content

### Requirement: Source ownership and synchronization conflicts are understandable
Linked task views SHALL show provider/resource display context, source link, supported managed fields/directions, last successful observation, pending updates and safe blocked/conflict/exhaustion reasons. Source-owned fields SHALL be visibly read-only with an explanation. Explicit attachment of a manual task SHALL use authorized existing connection/resource choices and ownership review; conflicts SHALL not merge task histories. Mapping and connection setup SHALL link to their existing management surfaces without asking for tokens in the task. Resolution SHALL show exact observed/desired values and submit only currently authorized preconditioned choices.

#### Scenario: Triage succeeded but source update is pending
- **WHEN** the external provider is unavailable after an accepted Triage result
- **THEN** the card separately shows completed execution and pending synchronization without presenting the check as failed

#### Scenario: Source has changed a managed field concurrently
- **WHEN** the server exposes a synchronization conflict
- **THEN** the UI shows its managed-field scope and supported resolution rather than an unconditional overwrite/retry action

#### Scenario: Installed adapter lacks a requested operation
- **WHEN** the operator configures an unsupported field direction or absent provider
- **THEN** the UI explains the unavailable capability and does not imply that generic integration metadata implements it

### Requirement: Activity discussion does not imply agent chat
Activity SHALL combine bounded safe execution/synchronization event references and operator-only plain-text discussion with actual server attribution and timestamps. Comments/replies SHALL use generated limits, separate append feedback and scoped replay; agent mentions or approval-like text SHALL remain non-executable discussion. Human clarification SHALL open a separate typed form for an exact declared gate. Bodies/response values SHALL not enter browser persistence, URLs, analytics or generic notifications, and unfinished drafts SHALL be discarded on leave/reload.

#### Scenario: Operator writes an agent mention
- **WHEN** a comment asks an agent to start work or approve a result
- **THEN** it appears as operator discussion with no execution/control success indicator

#### Scenario: Comment acknowledgement is lost
- **WHEN** the active append is retried
- **THEN** it uses the same frozen body/parent/key, preserves pending feedback and creates no duplicate comment

### Requirement: Results and operator actions retain exact execution authority
The card SHALL render exact server-published execution, evidence and Decision facts with non-color labels, distinguishing pending/failed/missing/indeterminate/superseded/accepted outcomes. Issue metadata, model text and previous-execution success SHALL not establish current acceptance. Shared Project/global and Issue-scoped action views SHALL use server-classified exact targets/contracts and existing guarded commands with Run preconditions. Recorded intent, owner processing and completed action SHALL remain distinct; the view SHALL not collapse multiple executions into one approve-task action or change the notification-center contract.

#### Scenario: Required verification has not passed
- **WHEN** an implementation Build succeeds but a required current check is missing or failed
- **THEN** the card shows technical completion and the unresolved check without declaring accepted work

#### Scenario: Another operator answered a gate
- **WHEN** a confirmed response receives a stale/precondition conflict
- **THEN** the UI refreshes its exact target and requires new review without upgrading or retargeting the intent

#### Scenario: Response is recorded before owner consumption
- **WHEN** the existing control returns an accepted intent receipt
- **THEN** the item is no longer unanswered and the card shows awaiting owner processing rather than node completion

### Requirement: Task views preserve bounded access and legacy journeys
Every task/card section SHALL use bounded REST queries with explicit loading/empty/stale/forbidden/not-found/conflict/unavailable states and independent observation coverage. Refresh failure SHALL retain visible stale data rather than invent success or an empty queue. Polling SHALL stop or back off when inactive/terminal as appropriate. Existing Run URLs, Artifact-based intake, Factory authoring and ordinary CI/CD SHALL remain usable. Supported keyboard/focus, English/Russian localization and narrow layouts SHALL cover create/edit/start/source-review/card/action journeys.

#### Scenario: Card refresh partially fails
- **WHEN** history, synchronization or results cannot refresh
- **THEN** the affected section is visibly stale/incomplete and unsafe confirmation is disabled until fresh review

#### Scenario: Operator follows an old Run URL
- **WHEN** its Issue association is available or still awaiting backfill
- **THEN** the original diagnostics remain reachable and an available Issue link is additive rather than required
