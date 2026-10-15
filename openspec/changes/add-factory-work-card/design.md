## Context

See `proposal.md` for motivation. The user confirmed that discussion is operator-only and that execution clarification uses existing human gates. This is a follow-up to the in-progress `add-dark-factory-mode` contracts, not a replacement for its remaining composition or release qualification. `add-factory-bugfix-quickstart` is an optional entry point, not a dependency: existing Artifact-admitted Work must receive the same card.

Observed boundaries:

- `WorkEnvelope` is immutable. Migration `0047_dark_factory_authority.sql` enforces unique scoped external Work identity and `factory_runs.work_envelope_id UNIQUE`. A Work already owns one Run whose nested Flow Runs, cycles, attempts, and Builds provide execution history.
- `factory_discovery/diagnostics.rs` publishes twenty bounded allowlisted collections, including Work, completions, technical stops, evidence, assessments, Decisions, and operator audit. It deliberately excludes private inputs and provider bodies.
- `factory_operator.rs`, `factory_control.rs`, and the atomic control store own visibility, declared human-response schemas, bound operations, eligibility, strong Run preconditions, scoped replay, and audit. A recorded response alone does not complete a node; `factory_flow_control_tests.rs` verifies later consumption by the owner across restart.
- `FactoryRunDiagnostics.tsx` and `FactoryRunCommands.tsx` already expose exact targets and commands. The Console currently assembles active human gates and server-eligible retries from diagnostic pages; a shared queue needs an authoritative server projection.
- The current attention feed is for favorite Build/Agent/Pool transitions and critical conditions. It has no personal authenticated inbox, and Console preferences must not persist server payloads or mutation identities.

## Goals / Non-Goals

**Goals:**

- Make one admitted Work discoverable as a coherent operator workspace while preserving original identity and the existing Run lifecycle.
- Add durable bounded discussion without granting model, context, or transition authority.
- Give operators an exact actionable queue and preserve the difference between recorded intent and completed execution.
- Keep headless clients, current Run routes, and ordinary CI/CD fully usable.

**Non-Goals:**

- Work editing/revisioning, linking independently resubmitted Work into a mutable Issue, or multiple Factory Runs per admitted Work.
- Automatic comment ingestion into model calls, mentions, agent delegation/chat, attachments, rich-text rendering, or new model providers.
- New user authentication, personal unread/assignment semantics, queue ownership locks, bulk approval, notifications, or a second execution/control API.
- A new raw task/provider-transcript preview. Original task/acceptance documents retain their separately authorized input/Artifact access path.

## Decisions

### 1. Use the Work Envelope as the stable card identity

Add Work-oriented application queries beside existing Factory discovery. Resolve Work -> existing Run under current management visibility and return a safe summary: external label, exact Configuration/Repository/base, available input metadata references, current Run/version, candidate/Decision, budgets, and links to bounded sections. Never reconstruct an identity by matching task text, a comment, or an external label across scopes.

History contains the root and nested Flow Runs, workflow cycles, Node Attempts/retries, and linked ordinary Builds/Attempts/Jobs. Each entry carries its immutable origin and subject. An independently submitted Work remains a different card. Existing `/factory/{run_id}` routes continue to operate and can link to the Work card; new admitted Work can open the card using already-returned Work identity without changing admission behavior.

No card table stores an editable execution state. Add query projections in the application/store boundary using existing records. Optional fields from older retained history remain unavailable. Input metadata is included only after independent Artifact/input visibility checks; an input reference or card does not mint a transfer capability. Underlying Work/Run remains the authorization and retention owner.

Alternative considered: add an Issue entity above Work. It would introduce mutable lifecycle and grouping semantics not needed by this request and risk weakening the established admission identity.

### 2. Keep discussion outside the Factory journal and Context

Use a narrow `FactoryWorkDiscussionStore` port in the existing store crate with memory/PostgreSQL parity. Add Work-owned discussion metadata and append-only comments with identity, Work-local sequence, actor kind/digest, server time, body digest, optional root parent, and sensitive plain-text body. A parent must be a retained root comment in the same Work. No editing/deletion API is added; corrections append another comment or reply.

Bounds are 16 KiB of nonblank UTF-8 per body, 1,000 lifetime comments per Work or a stricter deployment limit, a page ceiling of 100, and the existing request/rate limits. The count includes replies and expired-body metadata, so cleanup/replay cannot replenish quota. These are storage/interaction quotas; comments do not consume or reset Run execution budgets and do not increment Run version or count against the execution snapshot record bound.

Append is an authorized idempotent mutation. Atomically check Work visibility/retention/quota, validate the same-Work parent, allocate sequence, append the row and safe audit metadata, and store the scoped replay result. Concurrent distinct appends can both succeed because they overwrite nothing; they do not need the execution Run ETag. Exact replay resolves before a new allocation or quota charge. Changed body/parent reuse conflicts. Actor data comes from the management context; trusted-network release displays a neutral operator attribution rather than a fabricated user name or authenticated account.

Treat body types as sensitive in Debug/error/log serialization. Reject malformed input, recognized credential/private-capability patterns, and unsupported content before publication using existing sensitive-content primitives where applicable and a bounded comment-input policy. Store accepted text verbatim; render it as plain text, never HTML, executable Markdown, a fetched URL, or a mention. Comment reads are the only response boundary returning bodies; card summaries, queue items, audit, metrics, and system entries contain metadata only. This is not a claim that a detector can recognize every secret: privacy also depends on restricted dedicated reads and never automatically copying execution data into comments.

Do not add comments to Work artifacts, Context construction, Flow results, or prompt builders. No comment callback invokes a model or control operation. Selecting a clarification opens a separate declared typed response form; the operator supplies the value and confirms it through the existing command. Only the admitted Flow's explicit projection of that resulting immutable human response can affect a later model Context. The whole thread is never attached.

Alternative considered: share the control journal or an issue-comment trigger system. Discussion append would then contend with execution versions or create authority from prose; separate Work-owned storage avoids both.

### 3. Read execution and checks as typed evidence projections

Reuse the allowlisted Factory/Build diagnostics and retain exact producing identities. A card's checks list links candidate -> evidence/report -> review plan/assessment -> Decision, with safe reasons and completeness/freshness. Its schema distinguishes execution state from verification state and acceptance. Missing proof is unknown/missing, failed/indeterminate proof remains failed/indeterminate, and superseded proof is historical.

System timeline entries reference immutable source records and safe event codes. Do not generate AI narrative, copy raw provider output, infer successful tests from a completion string, or expose private findings/transcripts. Typed node outcomes follow the admitted schema; a custom key called `success` or an operator comment cannot override a required gate.

Return card Run version and per-section observation metadata. History/check pages bind a fixed authoritative history watermark and deterministic ordering `(recorded_at, source_kind, record_id)`; discussion pages bind their Work-local sequence watermark. Parent/nested/Build links are explicit identities, not browser-created joins. Refresh starts a new observation and invalidates prior current-result labels. The timeline merges references to execution records and comment metadata within declared run/discussion watermarks; it does not unify those streams into execution authority.

Historical diagnostics remain reachable when current facts change, but unavailable/expired bodies or proof are not reconstructed from a cache. A bounded page cannot claim completeness for unseen branches; required current summary comes from the authoritative read projection. Independent component errors retain the last visible data as stale rather than declaring empty or accepted state.

### 4. Derive the queue from the same operator eligibility contract

Introduce a pure action-description boundary shared with control eligibility, behind application queries. It produces generic human responses, bound escalation/delivery/promotion requests, or eligible retries from the pinned definition and authoritative current snapshot. Extract common current-cycle, deadline, cancellation, prior-control, retry-bound, and subject checks only after existing behavioral baselines/characterization. Keep bound operations separate from generic responses. No node-name or business-phase switch is introduced.

An item identity is derived from Work/Run, Flow/cycle/Node Attempt, command kind, admitted contract/schema digest, and required subject binding; it excludes the transient Run version. The item also reports current Run ETag/version, deadline, safe reason code, allowed outcomes/dispositions, and exact candidate/Decision when required. Unrelated Run progress can update the ETag without inventing another question. Labels come from safe operator-owned configuration/typed codes, with node key as fallback; question text is not taken from an unrestricted input or model transcript.

For bounded global reads, maintain an indexed **derived operator-action projection** from this pure descriptor. Owner/control transactions record a bounded durable projection invalidation with their existing authoritative mutation/outbox facts; a supervised optional read-model worker rebuilds descriptors from retained state. Projection maintenance has no model or external-service dependency and cannot become a required Factory execution dependency. It records source Run version and action identity, with no independent resolve setter or execution authority. A stale worker cannot overwrite a newer projection generation. Existing Runs receive a bounded resumable backfill; ordinary Work/Run rows are not rewritten. A lagged/unbuilt projection reports incomplete coverage and cannot produce an authoritative zero-pending claim. Expiry, cancellation, visibility, and equality with the current source Run version are checked before eligible ordering/counts/cursors; stale rows are not actionable. Rebuild never replaces Run/control authority.

Order actionable items oldest first by their authoritative gate/stop time and stable identity. Project/global scopes, Work filters, action kind, page bounds, and observation generation bind cursors; new refresh obtains a new live view. Visibility is applied before classification, order, counts, or cursor generation. Counts, if supplied, cover the same visible current snapshot and coverage; they are not inferred from the number of items loaded in the browser. Reads acquire no decision ownership or execution reservation.

Alternative considered: let each Console fetch all Runs and infer pending actions, or widen the notification feed. Neither yields an accurate shared bounded action collection while preserving current feed semantics.

### 5. Resolve queue items through existing controls only

The queue is a read view. The item points to the ordinary Run/Node response, bound-operation request, or retry command; there is no `close queue item`, `approve Work`, or bulk action endpoint. Submit the declared typed body using the same immutable confirmed intent, `Idempotency-Key`, and observed Run `If-Match`. Existing authorization, schema validation, exact target/subject binding, and atomic eligibility remain authoritative even if the item was forged, stale, or read from an old projection.

The accepted control transaction records its ordinary intent/audit outcome and invalidates older projection generations atomically. Until the read worker refreshes descriptors, the current source-version/control check prevents the old row being offered as unanswered; direct card observation exposes the safe receipt and recorded-intent state. The command does not complete the node. The owner later appends consumption/completion, retry, or a technical stop under its current fence. Card history distinguishes those steps and never reads queue disappearance as success. Two fresh competing intents against one version yield at most one accepted intent; exact replay of the winner returns its original receipt after later progress.

No discussion-link field is required on existing control DTOs in this slice. Operators can see the discussion while filling a response, but command authority and audit correlation remain the existing typed value/reason and immutable target. A later optional provenance link would need its own bounded contract; this proposal does not use a comment ID as proof or permission.

### 6. Keep REST and Console surfaces additive

Proposed routes below `/api/v1`:

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/factory-work/{work_id}` | Safe card summary and existing Run linkage |
| GET | `/factory-work/{work_id}/history` | Bounded exact execution history |
| GET | `/factory-work/{work_id}/checks` | Safe candidate/evidence/assessment/Decision projection |
| GET | `/factory-work/{work_id}/timeline` | Bounded system/comment metadata references |
| GET/POST | `/factory-work/{work_id}/comments` | Dedicated body reads / idempotent plain-text append |
| GET | `/factory-work/{work_id}/operator-actions` | Current Work-scoped action view |
| GET | `/projects/{project_id}/factory-operator-actions` | Shared Project queue |
| GET | `/factory-operator-actions` | Shared visible global queue |

Expose safe `work_envelope_id` links on existing summaries where needed as additive metadata. Publish bounds, closed action contracts, observation/coverage, error codes, and actor attribution in OpenAPI and generated clients. Work and parent visibility precede reads/writes, including reply-parent lookup and input metadata. Comment append needs its own stable discussion action/resource mapping; permission to discuss is not permission to execute a gate. All body reads use no-store response policy, no private provider/object URL in JSON, and no cross-origin or login changes.

The Console card route is `/factory/work/{work_id}` with Overview, Discussion, History, Checks, and Actions sections. A shared decision view lives within Factory and offers Project/action filters. Existing explorer Run selection and quickstart Run navigation can link to the card while retaining all current Run URLs and advanced diagnostics. Do not add a new primary product activity or personal inbox.

Reuse existing query-state notices, paged explorers, typed value editor, and immutable confirmed-intent helper. Queue opening supplies the exact target and typed form; stale data disables confirmation until explicit refresh/review. Comments have a separate append control and feedback. Pending forms/query bodies/keys remain in memory only, with no browser persistence or sensitive URL values. Bounded polling/backoff runs only while the view is active and needed; cancellation, retry, and operator responses retain current duplicate/conflict handling. Preserve keyboard/focus, non-color labels, English/Russian localization, narrow layouts, and the header notification contract.

## Risks / Trade-offs

- **Work is not a mutable issue** -> State this distinction in the UI: execution history is the original Run's exact nested/retry/cycle history; separately admitted Work has its own card.
- **Action projection can be stale or incomplete during backfill** -> Report source versions and coverage, disable unsafe actions, and keep existing direct controls and independent validation authoritative.
- **Run ETags change during sibling progress** -> Preserve reviewed target/context and require explicit refresh/review after conflict; never silently retarget or upgrade a precondition.
- **Comments may contain sensitive operator input** -> Bound and validate bodies, restrict body reads, use sensitive wrappers, reject recognized secret/capability patterns, and never mirror execution data automatically. Generic logs/audit/queue contain metadata only.
- **Retained older results may lack current proof details** -> Render unavailable/missing explicitly; do not synthesize current acceptance or reconstruct content from provider sessions.
- **Underlying Factory qualification is unfinished** -> Qualify this card against its real control/recovery boundary and retain upstream release blockers rather than treating planning completion as product readiness.

## Migration Plan

1. Add reentrant discussion/counter/replay metadata and rebuildable operator-action projection migrations and narrow store ports; retain all Work/Run identities and existing execution tables. Run old behavior first and add characterization before shared eligibility refactoring.
2. Wire optional projection invalidation/rebuild and backfill derived actions from authoritative retained Runs in bounded batches; publish coverage before exposing a complete queue. Do not create discussion rows or new Run versions for untouched existing Work. A failed read-model worker degrades queue freshness only; direct Factory execution/controls continue.
3. Register additive REST and generated client contracts and deploy the Console card behind optional collaboration wiring. Verify Artifact-based Work without quickstart and existing direct controls.
4. Integrate discussion with current Work/Run retention/holds, safe body expiry, backup/restore, and audited cleanup. Replay metadata retains identity/digest and explicitly unavailable expired content without resurrecting bodies. Cleanup changes no execution evidence or budgets.
5. Roll back by disabling new comment appends/card/queue exposure while retaining persisted discussion metadata and compatible cleanup/backup support. Rebuild the derived queue when re-enabled; do not downgrade to a binary that silently ignores retained discussion ownership requirements.
6. Sync/archive only this change after implementation and prerequisite acceptance. Do not rewrite or implicitly sync/archive the Factory or quickstart proposals.

## Verification Strategy

Use public application/HTTP and memory/PostgreSQL contracts, followed by Console component/browser journeys. Required tests cover original Work/Run preservation, old versus current candidate checks, append/replay/quota/parent scope, comment-to-Context absence, every current queue eligibility predicate, projection backfill/expiry/restart, recorded intent versus owner consumption, concurrent responses and stale candidates, cross-scope visibility and cursor misuse, redaction/no-store/no-persistence, and ordinary CI/CD/direct-control compatibility.

The released pilot admits ordinary Work, produces a nested human gate and an eligible infrastructure retry, appends operator discussion, resolves one declared clarification, and follows owner consumption into exact verification results across restart. Fault injection and deliberately failing/indeterminate evidence prove that neither comments, queue membership, nor model success can bypass required gates. Every behavior, executable fixture, configuration, and migration uses repository-mandated Red/Green before implementation; documentation-only changes use appropriate validation.
