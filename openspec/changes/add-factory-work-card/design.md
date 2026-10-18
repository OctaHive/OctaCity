## Context

See `proposal.md` for motivation and scope. The original plan projected one admitted Work as its card. The requested product now needs a task that exists before execution, survives requirement edits, and unifies assessment, implementation, and later executions. This deliberately replaces that earlier planning decision; it does not alter the immutable Factory execution contracts.

Current constraints:

- `WorkEnvelope` and admitted Factory Configuration/Flow versions are immutable. The admission store and migration `0047_dark_factory_authority.sql` enforce scoped Work replay and one Factory Run per Work.
- `add-github-work-source` already separates Source observation/admission from Reporter effects, uses stable native identities, and retains recoverable reporting. Its input-scoped Work identity intentionally differs between independently configured pipelines.
- Direct text publication is planned in `add-factory-bugfix-quickstart`; current Artifact admission remains supported. Project-owned input Artifacts have no fabricated Build provenance.
- Existing diagnostics, Build logs, Factory control eligibility, exact evidence, and Run preconditions are reusable. AI profiles and enrolled worker Agents are different concepts; role-profile management is still a separate change.

## Goals / Non-Goals

**Goals:**

- Give both manually created and imported tasks a stable internal home independent of a provider or execution target.
- Preserve exact requirements-to-execution lineage across multiple pipelines and changed requirements.
- Make synchronization ownership and conflicts visible without letting external changes control running execution.
- Let each Project configure business statuses and permitted transitions, with explicit pipeline actions and a separate provider mapping rather than lifecycle inference from Run completion or labels.
- Reuse current execution, reporting, input publication, guarded controls, and pagination rather than introduce parallel implementations.

**Non-Goals:**

- A replacement Factory scheduler, a mutable Work Envelope, or multiple Runs for one Work.
- Full forge feature parity, arbitrary two-way document merging, external ticket creation, cross-provider mirroring, synchronized comments/attachments, or new native GitLab/Jira adapters.
- Agent chat, mention-triggered execution, automatic delegation, authenticated personal assignment/inbox semantics, or a new pipeline designer.
- A second execution scheduler, arbitrary workflow scripts/guards, inferred multi-edge moves, or automatic/bulk migration of existing Issues to replacement workflows.

## Decisions

### 1. Issue owns editable intent; Work owns one frozen execution

Use the following change-local vocabulary until implementation updates the shared glossary:

| Term | Meaning |
| --- | --- |
| Issue | Persistent Project-owned task with stable internal ID and Project-local display key |
| Issue revision | Immutable version of task title, description, acceptance input and bounded business metadata |
| Issue workflow | Project-owned immutable business-status vocabulary and finite permitted-transition policy; independent of a Factory Flow |
| Issue status transition | One authorized organizational move with exact workflow, before/after status, concurrency and actor/action/observation provenance |
| Issue execution request | Explicit durable request binding an Issue revision to an exact Factory target and ordinary admission |
| Work Envelope | Existing immutable admitted execution input; still owns exactly one Factory Run |
| External source link | Optional association to one authorized native external task with a versioned synchronization policy |

An Issue has zero or many execution requests and associated Work Envelopes. Each Work belongs to at most one Issue in its owning Project; retries, nested Flows and requirements cycles remain children of its original Run. An existing Work without an Issue remains valid and discoverable through legacy views until backfilled.

Issue creation requires a visible Project and bounded nonblank title/description rules, not a repository, acceptance document, installed provider, model, or Factory Configuration. Acceptance criteria can be incomplete in a draft. The server validates configured finite business metadata values and declared bounds. Metadata status represents task organization; it cannot set an accepted Decision, complete a Run, or grant delivery authority. Profile changes and Issue priority edits affect only later reviewed requests, not scheduling intent already frozen in Work/Build records.

Use a small Issue aggregate with current revision pointer and append-only revisions, plus exact execution/source associations. Do not implement a generic event-sourcing platform or store a second copy of mutable Factory execution state. Concurrent edits require the observed Issue version; exact scoped command replay returns its original revision without re-reading the new head. Execution progression must not change the editable Issue version merely to refresh a progress badge.

Alternative considered: make Work editable or create one new card per pipeline. The former breaks immutable inputs and the latter loses the user's persistent task. A separate task identity above existing execution is justified by the revised scope.

### 1a. Project workflows define statuses; one command owns transitions

Publish bounded immutable Project workflow versions with stable workflow/status/transition IDs, display names, display order, one initial status, optional organizational terminal markers, and explicit from/to edges with permitted command origins/rights. Validate references, uniqueness, bounds and initial-state consistency. Reopening can be an explicitly permitted edge; do not infer a path or execute intermediate moves. An optional finite guard can require exact current accepted execution evidence through existing eligibility contracts; no script, phase-name switch or second Flow interpreter is introduced. An organizational terminal status never creates a Decision, completes a Run or permits delivery.

Each Issue-enabled Project has a valid editable default from ordinary declarative template data, without requiring its first task creator to administer a workflow. Manual/imported creation starts at that version's initial status. Template names such as New, Triage, Ready, In progress, Review, Done and Declined are examples, not domain enums. Each Issue pins its workflow version; a new default applies to new Issues only. Keep referenced versions/status identities available. Adopting another version for an existing Issue requires separate reviewed authority, its current version/status, an explicit valid status correspondence, compatible source mapping and current status-field ownership; it changes no frozen execution, implicitly dispatches no provider effect and invalidates old automated transition applicability. Bulk migration is out of scope.

Manual moves, accepted pipeline actions and source-owned mapped observations call one narrow transition command/store boundary. It checks current Issue/status concurrency, exact workflow edge, origin/rights, applicable evidence and field ownership, and atomically publishes the resulting revision, status history, replay receipt, audit and any permitted retained outbound intent. Generic editable-field updates cannot set status/workflow directly. Denied, stale or forbidden moves publish no partial status or provider intent. Exact replay returns its original outcome after later moves rather than applying the edge again. History records before/after IDs, workflow version, safe reason and actor or exact execution/action/source observation; ordinary progress refreshes create no status revisions.

Example configuration, independently editable per Project:

| Explicit trigger/action | Business transition |
| --- | --- |
| Manual/import creation | Initial New status; no execution |
| Accepted configured Triage entry action | New -> Triage |
| Accepted eligible/classified Triage outcome | Triage -> Ready |
| Accepted duplicate/out-of-scope outcome | Triage -> Declined, with reason |
| Accepted implementation entry/review actions | Ready -> In progress -> Review, as separate declared moves |
| Accepted configured completion action with required evidence | Review -> Done |

If no action is configured, completing Triage or any Run changes no Issue status. Manual status movement starts no pipeline and cancels no existing execution. Existing independently enabled Source inputs keep their own intake semantics; a business status is not an execution request.

### 1b. Pipeline status actions are explicit and applicability-bound

Use the existing generic trusted-action publication/execution boundary for an installed Issue-status command. An operator selects a declared stage outcome, exact Project workflow/version and transition, with permitted evidence requirements. Its input is typed accepted data; raw model prose or an unhandled failure cannot choose an edge. No external source, Reporter, token or special node kind is required for a local Issue action. With no associated Issue, do not guess/create one; reject an incompatible configuration before execution where known or expose an explicit safe unavailable action result. Keep existing unlinked Work and ordinary Flow behavior intact.

An execution's authorized Issue association retains its requirements content/revision lineage and an expected status-applicability marker. Only successful status moves from that same execution's retained command lineage can advance that marker for its next configured action; a status-only move does not falsely invalidate unchanged requirements. A committed manual/source status move, a status move from another execution, workflow adoption or changed requirements invalidates the earlier applicability even if the displayed status or requirements text later returns to the same value. Merely starting another execution, observing unchanged data or changing unrelated metadata does not invalidate it. At action acceptance, recheck this binding plus the current Issue/status precondition, exact edge, current rights and evidence. Competing Runs cannot use last-finisher-wins. A stale/conflicting move keeps its original result and accepted execution history, starts no new execution and is not silently re-evaluated against a newer head; reconciliation requires a new reviewed command under current authority. Replay of an already committed status command remains valid after later edits.

Persist/resume the exact command and original action identity through existing retained-intent/replay machinery, so interruption between action acceptance and status application cannot duplicate history or lose a committed move. This marker and receipts are bounded applicability metadata, not a second Run state or ownership engine. Status-action success never substitutes for a required execution gate. Existing frozen Work and its Run controls remain authoritative for execution.

### 2. Explicit execution requests freeze inputs through existing admission

Starting work is distinct from saving a task. Review the exact Issue revision, visible Factory Configuration and Repository versions, bounded task/acceptance mapping, base-reference policy, and required capabilities. Missing acceptance input is an actionable validation error, never generated from an empty field. A stale reviewed Issue version is a conflict; the server must not silently substitute the latest description.

Persist the execution request, revision/content digests, selected target and replay identity before resumable input publication. Reuse one Project-input publication service and existing admission probe. Recover through intent, immutable input publication, base resolution, admission and association stages with exact retained identities. Admission replay still resolves before mutable source lookup. A retry after a lost acknowledgement returns the same Work/Run; the request is not reported admitted until that association is durable. Use an atomic store operation when possible and a retained reconciliation step for already committed admission; never compensate by deleting Work or admitting a replacement.

An intentional later execution has a new execution-request identity and a new ordinary scoped manual Work identity. It does not reuse a transport idempotency key or pretend to be a retry of the earlier Run. Existing automatic Source admission keeps its current input-scoped deduplication identity; attach its accepted Work to the canonical Issue idempotently without changing replay keys. An external edit alone is not an execution request. Existing independently configured downstream input predicates can still authorize a distinct ordinary admission.

The execution snapshot includes exact revision, task/acceptance input identities, source observation when applicable, configuration, repository/base, and mapping versions. Source effects additionally bind the authorized source link/native target and reporting mapping at the original execution/action boundary; a card association is not permission to resolve a different current source at dispatch. Attaching a source later cannot retroactively enable automatic reporting from an unlinked execution. The UI labels a running execution as using an earlier revision when requirements change. Retries use that same snapshot; starting against changed requirements is a separate reviewed request. Multiple active executions are possible only under their existing WIP/permission/budget rules; the card lists each rather than choosing an imaginary single current agent.

Alternative considered: have an editable Issue body read on every model call. Rejected because retries and results would no longer describe the originally accepted task.

### 3. Canonical external identity is independent of pipeline intake identity

Initially each Issue has zero or one primary source link. A link is unique within its Project and accepted security partition by the trusted provider installation namespace, native resource identity and native item identity. A display number, title, URL, credential, connection display name or pipeline input ID is not that key. Compatible connection rotation/adoption preserves installed native identity. Never group issues across Projects or infer sameness from text.

Separate import/synchronization configuration from automatic pipeline admission. An authorized Source can create or refresh a linked Issue without a configured execution target. If a configured input also admits Work, it attaches to that Issue using the same native key. Two inputs observing one external ticket resolve one Issue and two independently deduplicated Work associations. A changed description, labels, token or input version does not create another Issue.

Support explicit association of a visible unlinked manual Issue to an existing external task after scoped read validation and review of field ownership. If the native task already belongs to another Issue in that Project, return a safe conflict and canonical visible link; do not silently merge task histories. Initial scope does not support source replacement, multi-source mirroring, or automatic creation of a remote ticket. Cross-Project or hidden associations are not disclosed.

Reuse installed Source operations and bounded observation/progress machinery. Synchronization must refresh retained links even when a ticket no longer matches an execution selector. A permitted link can have its own pull policy while an execution input is disabled; disabling a connection, revoking its grants or disabling link synchronization stops the corresponding provider access. Share observations with intake where practical; do not build an unrelated provider poller, credential store or label-specific routing engine.

### 4. Field ownership bounds synchronization

Each link pins a validated immutable mapping version and exposes independently managed pull/push settings. Every managed field has one authority and direction: source-owned pull, OctaCity-owned push, or not synchronized. Initially import title/description from the source and project configured accepted business outcomes outward; other fields require an explicitly advertised readable/writable capability and bounded mapping. Source-owned fields are visibly read-only in the card while linked; other fields remain locally editable. A local-only task has no provider dependency.

The external status mapping is separate from the Project workflow and a Flow's action selection. It references stable internal status IDs and the applicable workflow version, then maps either an OctaCity-owned accepted status to exact supported native values/effects or fresh source-owned native observations to one configured permitted internal transition. Native open/closed states and label names have no built-in equivalence to Done, Ready or acceptance. Pull rules must resolve exactly one target or remain visibly unmapped/ambiguous; do not choose by rule order, invent intermediate transitions or bypass an edge/evidence guard. Imported tasks begin at the workflow initial status; an incompatible subsequent source status stays an explicit synchronization condition rather than forcing the card through a forbidden move.

Source-owned status blocks manual and pipeline replacement; OctaCity-owned status treats native divergence as reporting/conflict evidence rather than an incoming status command. Unsynchronized status remains locally managed. Ownership changes are reviewed mapping commands, not fallback after a denied move. Workflow adoption/mapping replacement validates all referenced statuses and supported effects and never retargets an accepted operation. GitHub can project managed labels and optional open/close through its existing capability; another provider's native transition is offered only if actually supported. Multiple native matches, unsupported effects and missing grants are explicit failures/conditions. A pull echo creates no new transition, revision, action or Run.

Do not double-produce the same native managed field from a card-status push and a direct Factory source-update action for one outcome. At Flow/mapping publication and action acceptance, reject overlapping producer configuration or select one explicitly reviewed producer using the existing effect authority; later association alone never adds a second write. Both producers reuse existing Reporter ordering/recovery rather than another outbox.

Pull observations are data, not commands. Validate source scope, native identity, content bounds and provenance before creating an Issue revision. A repeated or unchanged observation creates no new content revision. Retain ordering evidence and observation receipts so an older page cannot overwrite a newer accepted revision. Where the provider lacks reliable comparable versions, re-observe and reconcile under the existing bounded provider contract; uncertainty is explicit rather than ordered by arrival time. Pull never rewrites an execution snapshot, automatically cancels/restarts work, or grants lifecycle authority from native labels/state.

Push intent comes only from an authorized Issue metadata command or accepted configured Factory status action, with exact Issue/Work/Run/action provenance. Merely having a link or editing a comment does not publish a status. Accepted business status updates are reflected in the Issue through their authoritative records; a manually chosen task status remains organizational metadata, not a Factory Decision. Initial GitHub effects use the existing managed-label and optional open/close Reporter mapping. Arbitrary remote title, assignee, priority or component writes are not advertised unless the installed adapter supports their exact contract. Human-facing values such as `approved` are configuration, not privileged domain enums.

Reuse the existing reporting outbox, fencing, managed-field ordering, unknown-response observation and bounded reconciliation. Retain desired value, observed value and mapping/action identity; preserve unmanaged fields. Mapping replacement affects new reviewed intents, not the target or effects of an already committed operation; current revocation still prevents dispatch. A remote divergence either reconciles under the previously authorized managed-field policy or becomes an explicit conflict. An operator resolution is a new preconditioned, authorized desired state or mapping update; there is no unconditional retry-overwrite button. A pull echo or repeated projection receipt creates no fresh execution, comment or logical status action.

Issue business status, execution state and sync health are independent. Report direction, last successful observation, pending operations and safe blocked/conflict/exhaustion reasons. External deletion, inaccessible resource, rate limits and missing read/write grants leave the local Issue and accepted execution history intact. Do not claim a provider-wide atomic update or full two-way synchronization. A stalled push cannot block unrelated manual tasks or roll back accepted results.

Alternative considered: generic last-writer-wins mirroring. Rejected because external APIs differ, concurrent edits are normal, and a mirror would obscure which actor controls execution and results.

### 5. Card projections retain exact evidence and safe execution progress

Query Issue -> execution requests -> Work -> existing Runs, nested Flow Runs, cycles, Node Attempts, Builds and Jobs. Show all active executions, the selected historical execution, and pending admission separately. Expose the actual AI execution profile/model only when retained and authorized, plus the enrolled Agent/runtime when assigned. Queued, parallel, unavailable or historical execution is explicit; no fabricated assignee or live model identity. Role-profile management remains optional and worker Agent identity keeps its existing meaning.

The card has Overview, Activity, Runs, Results and Actions sections, with discussion in Activity. The task list offers bounded Project/status/priority/source/execution/sync filters and a manual-create entry point. Human-readable keys and safe title summaries replace opaque Work strings in primary navigation. This is a task-oriented entry within the existing Console shell; its implementation should reuse current list/detail navigation rather than redesign unrelated activities or the Factory canvas.

Project settings expose status names/order, initial/terminal organization, allowed edges/origins and supported evidence requirements, with reviewed immutable replacement and explicit version adoption. The card renders only server-authorized current moves, their expected version and ownership/unavailable reasons; it shows status history separately from Run and sync progress. Existing Flow authoring adds explicit Issue-status actions on declared outcome routes without introducing a second canvas. Source-link mapping review is a separate surface showing internal status, native observation/effects and field authority, never a replacement for Project workflow editing. All three surfaces use bounded generated choices, current-version review, keyboard/focus, English/Russian labels and in-memory-only drafts. A status choice or terminal marker cannot be presented as approve/deploy authority.

Results bind each candidate, evidence report, assessment and Decision to its originating execution. A later completed triage does not erase an earlier implementation failure, and an earlier candidate's pass cannot satisfy a newer candidate. Separate technical completion, verification completeness and acceptance. Model text and comments cannot assert checks passed. Log links use existing authorized Build/log readers; never aggregate private transcripts or hidden reasoning into the task timeline.

Bound card sections independently. Execution/check pages bind their authoritative history watermark, Issue revision and selected execution; discussion has its own Issue-local sequence. Deterministic timeline ordering includes timestamp, source kind and stable record identity. The card reports per-section observation and incomplete/stale coverage, not an atomic snapshot of every external system. Never infer a global success badge from a paginated subset or the last completed Run.

### 6. Keep discussion separate from execution and retain exact operator actions

Retain the original operator-only discussion boundary, now owned by Issue. Comments are append-only plain text, at most 16,384 UTF-8 bytes and 1,000 lifetime comments per Issue or stricter configured limits, with one-level same-Issue replies. Atomically allocate sequence, persist scoped replay and safe audit, and preserve retention/hold metadata. Known sensitive patterns are rejected; dedicated body reads and restricted access remain necessary because detection is not complete. Actor attribution reflects the trusted-network release rather than invented authenticated accounts. No raw body in generic audit, cursor, logs, analytics, URLs or queue payloads.

Comments never trigger an agent, change Work, become evidence/approval, or automatically enter model Context. Clarification uses an explicitly selected declared human-gate response and its existing control endpoint. External comment synchronization and conversational agent commands remain separate future work.

Reuse the pure current control-eligibility descriptor for the shared Project/global action queue. Each item retains exact Issue association when present plus Work/Run/Flow/cycle/attempt/contract/candidate identities, deadlines, allowed responses and observed Run version. Existing versioned control authority owns resolution, with `Idempotency-Key` and Run `If-Match`; Issue versions are not substitutes. No generic approve-Issue, close-queue-item or bulk lifecycle override endpoint is introduced.

Retain the indexed derived action projection with transactional invalidation, resumable bounded backfill and source-version fencing. A stopped read-model worker reports incomplete coverage without blocking execution. Reads revalidate visibility, expiry and source versions before counts/order/cursors; recorded intent is no longer unanswered. Owner consumption remains distinct from command acceptance. Competing responses allow at most one intent; winning exact replay survives later progress. Issue metadata edits cannot retarget a pending response to another execution.

### 7. Additive REST, security and recovery boundaries

Proposed resources below `/api/v1`:

| Method | Path | Purpose |
| --- | --- | --- |
| GET/POST | `/issues` | Bounded visible list / manual creation |
| GET/PATCH | `/issues/{issue_id}` | Task card / preconditioned editable-field revision |
| GET/POST | `/projects/{project_id}/issue-workflows` | Visible workflow versions / reviewed immutable publication and Project default selection |
| GET | `/issues/{issue_id}/transitions` | Current permitted business moves and applicability/ownership reasons |
| POST | `/issues/{issue_id}/transitions` | Explicit preconditioned organizational transition, not a Run control |
| GET | `/issues/{issue_id}/status-history` | Bounded status transition receipts and safe provenance |
| POST | `/issues/{issue_id}/workflow-adoption` | Explicit reviewed workflow/version/status correspondence |
| GET | `/issues/{issue_id}/revisions` | Authorized bounded requirements history |
| GET/POST | `/issues/{issue_id}/executions` | Execution history / explicit reviewed execution request |
| GET | `/issues/{issue_id}/executions/{request_id}` | Recoverable publication/admission receipt |
| GET | `/issues/{issue_id}/results` | Exact selected-execution checks and outcomes |
| GET | `/issues/{issue_id}/timeline` | Safe system and discussion references |
| GET/POST | `/issues/{issue_id}/comments` | Dedicated body reads / idempotent append |
| GET/POST | `/issues/{issue_id}/source-link` | Source association / explicit attach with ownership review |
| GET | `/issues/{issue_id}/synchronization` | Mapping, freshness, pending effects and conflicts |
| POST | `/issues/{issue_id}/synchronization/resolve` | Reviewed versioned resolution within current grants |
| GET | `/issues/{issue_id}/operator-actions` | Exact actions across associated executions |
| GET | `/projects/{project_id}/factory-operator-actions` | Existing planned shared Project queue |
| GET | `/factory-operator-actions` | Existing planned visible global queue |

Connection/grant/mapping creation uses existing integration and definition-management boundaries; typed pipeline Issue-status actions use existing Flow publication. Do not accept raw tokens or arbitrary provider payloads on Issue routes. Issue writes require their own authorized action/resource registration, scoped replay and strong current revision/link/status preconditions. Workflow administration, manual status movement, pipeline status actions and source-status mapping require their appropriate separate authority; Issue edit/discussion rights do not grant them. Execution commands additionally require ordinary admission rights and exact target validation. Business transition routes cannot resolve an execution human gate or substitute Issue versions for Run preconditions. GETs never create/backfill Issues, dispatch providers or reserve execution.

Dedicated description/revision/comment body reads apply no-store and independent visibility. List/card summaries and operational records return only safe metadata/references, not task bodies, prompts, hidden tests, provider transcripts, credentials or private transfer URLs. Generated bounded schemas drive Console validation and choices. Sensitive drafts and confirmed request keys remain in component memory only. Preserve current no-persistence, query-state, keyboard/focus, narrow-layout and English/Russian contracts.

Publish Issue identity as additive metadata on new admission/Run views after association. Existing Run routes, controls and Artifact intake remain usable, including before optional Issue features are enabled. The canonical card route is `/issues/{issue_id}`; URLs can select tabs and exact execution IDs, never bodies or command values. Issue-only creation works while Factory is unconfigured.

### 8. Share prerequisites without expanding neighboring changes

Keep the three existing capability paths in this change. Implement Source observations, Issue association and managed-effect consumption through generic seams next to existing admission/reporting; do not change the dark-factory admission requirement or its one-Run invariant. Current GitHub behavior remains the compatibility baseline. Native GitLab/Jira support is proven as an extension boundary with a second fake adapter, not claimed as shipped integration.

The archived `add-github-work-source` supplies connection grants, provider checks, input/source-effect authoring and optional observation contracts. Characterize the concrete installed prerequisites rather than reopen completed tasks or assume live GitHub qualification; its live scope remains unqualified. This change owns internal status workflows, explicit Issue-status actions and separate field/status synchronization. Manual Issue/workflow journeys need none of those provider prerequisites. `add-factory-bugfix-quickstart` still owns its preset and readiness UX. Its recoverable Project text-input publication is a single shared seam: characterize or extract existing behavior first, implement here if absent, and record follow-up reconciliation in that change before its implementation. `add-factory-role-profiles` can later supply profile labels without making every Issue dependent on it. Neighboring planning files are not edited in this update.

## Risks / Trade-offs

- **Issue revisions could obscure executed requirements** -> Retain immutable revision/input references per request; label newer descriptions and historical results explicitly.
- **Concurrent imports could split one task** -> Enforce scoped native-link uniqueness and idempotent atomic association; never deduplicate by title or pipeline key.
- **A mutable task status could look like acceptance** -> Separate task metadata from authoritative Run Decisions and exact checks in API and UI.
- **Old or parallel executions could move a newer card** -> Guard explicit actions by frozen requirements/workflow, status applicability and retained command lineage; stale commands conflict without undoing accepted execution.
- **Project policy could drift beneath active tasks** -> Pin workflow versions and require explicit reviewed adoption; keep referenced identities/history and mappings.
- **Native status mapping could become another workflow** -> Keep it field-owned translation through the one transition command; reject ambiguous, forbidden and overlapping write producers.
- **Admission and Issue association may commit separately** -> Retain the original request/admission identity and reconcile linkage across restart without new Work.
- **External state changes during writes** -> Use existing Reporter fences, observed revisions and bounded reconciliation; expose conflicts where atomic native preconditions are unavailable.
- **Legacy history has incomplete identity or payload retention** -> Backfill only verified native associations, otherwise use one Issue per Work and mark unavailable fields; never guess or recreate expired bodies.
- **A larger task model could become another orchestrator** -> Keep Issue limited to intent/revisions/associations; all execution and eligibility stay with current owners.

## Migration Plan

1. Run current admission, Source/Reporter, input-publication, control and Console regressions. Add characterization where coverage is missing before shared-seam refactoring.
2. Add reentrant Project workflow/default, Issue/revision/key/status-history, source-link, execution-request/association/applicability, discussion/replay and derived-action metadata through additive migrations and narrow store ports with memory/PostgreSQL parity. Make the declarative default available without provider/Factory setup or extra rights for the first creator. Preserve all Work/Run IDs, unique constraints and journals.
3. Backfill retained Work in bounded resumable batches: group only verified equal native identities within one Project/security scope, otherwise create one Issue per Work. Use safe retained title metadata or an explicit unavailable placeholder, not the external identity as fabricated requirements. Backfill creates no admission, provider calls, credentials, status effects or discussion; first backfilled revision does not assert that every associated Work used its content. Exact legacy Work Artifacts remain its execution input authority.
4. Enable manual Issue creation/editing and recoverable execution first. Reuse the same text publication/admission seam as quickstart and verify old Artifact/Run clients. Then wire optional source import, ongoing link refresh and existing reporting, disabled until explicit authorized configuration.
5. Expose generated REST/Console Project workflow settings, exact status actions/history, separate source mapping review, task navigation, old-Run-to-Issue links and shared actions with honest backfill/coverage. Initialize legacy cards at a reviewed/default workflow initial status rather than guess status from the latest Run. Configure no automatic execution, transition or synchronization merely because a migration created a card.
6. Apply separate Issue-content/discussion retention with exact execution input retention/holds. Keep association/replay metadata when required without resurrecting expired descriptions, comments, provider snapshots or logs. Include pending requests, native links and reporting state in backup/restore.
7. Roll back by disabling new Issue writes/import/sync dispatch and UI while retaining compatible cleanup and pending-intent state. Existing direct Factory/Build operations continue. Never downgrade to a binary that can dispatch providers while ignoring retained ownership/fences; do not delete new history as rollback.

## Verification Strategy

Use public application/HTTP contracts with in-memory and PostgreSQL parity, fake source/reporter adapters and Console component/browser journeys. Verify manual tasks without Factory or providers; concurrent edits/imports; multiple executions of exact revisions; unchanged old admission replay; interrupted publication/admission/linkage; stale external observations; managed-field conflicts and unknown writes; revocation and rate limits; no echo-triggered execution; evidence binding; comment isolation; guarded queue recovery; visibility, retention and backup.

Qualify custom renamed statuses/edges, initial defaults, immutable workflow replacement/adoption, forbidden/manual/source-owned moves, configured evidence guards and no-action Run completion. Exercise sequential actions from one execution, competing/late Runs, changed requirements and status returning to an earlier value; replay must retain one status receipt without rewriting accepted execution. Verify separate push/pull/unsynchronized status authority, unmapped/ambiguous observations, forbidden external edges, unsupported provider transitions and overlap with existing direct source effects. Triage may reach Ready or Declined only through its configured action, never Done merely because its Run completed.

The end-to-end pilot creates a manual task, edits it, starts one configured pipeline, edits the task while the original Run retains its snapshot, and starts a later explicit execution under the same card. A separate imported task crosses assessment and implementation inputs, receives accepted configured source effects, and retains successful execution while an injected provider outage shows pending synchronization. Include a declared human response and infrastructure retry, replay/restart, failed/indeterminate evidence, and a second provider-neutral fake implementation. No live paid model or native GitLab/Jira account is required for these contracts; real supported-adapter qualification remains explicit.

Every observable implementation, fixture, configuration and migration uses meaningful public-boundary Red/Green before implementation. Run current behavior first for refactors and record exact commands/outcomes. This planning-only revision has no executable behavior; validate OpenSpec, formatting and cross-artifact consistency instead.
