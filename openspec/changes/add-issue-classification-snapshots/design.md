## Context

See [proposal.md](proposal.md) for motivation and scope. The existing model already separates `IssueRevision`, `Work Envelope` and `Factory Run` in `CONTEXT.md`; this design preserves those meanings.

Observed implementation:

- `server/core/octacity-server-store/src/issues/issue_planning.rs` provides Project-defined scalar/choice fields and immutable definition versions. Planning edits advance the Issue revision without changing its existing applicability fence merely because assignment or dates changed.
- Task 1.1 now adds optional semantic bindings for seven distinct Project `choice` properties with legacy-empty decoding. Built-in priority is unchanged. Ordinary custom properties still support text, choice, boolean, bounded integer and date values. The Console edits published values and creates labels, but has no general field-definition editor.
- Operator discussion is already implemented by `IssueDiscussionHandlers`, `IssueCommentForm` and the Issue Activity section, including immutable comment IDs/digests, root replies, separate body authorization and retention. It currently supplies no execution input; extend explicit snapshot selection without changing append/read behavior or adding another comment system.
- Project settings already publish and view named immutable input documents through `ProjectDocumentPublisher`. Exact references use Artifact identity, content digest and encoded size; a new publication is a new exact reference, not an implicit latest version. Issue-owned document publication/attachment and comment-to-input selection do not yet exist. Reusing the current Project catalog for ticket documents would mix task-specific attachments with shared guidance.
- `issues/issue_classification.rs` and its PostgreSQL owner already apply configured priority and size-label effects with exact Issue/Run provenance, source ownership, field-conflict detection and transactional replay. They do not yet apply a complete typed classification.
- `server/application/src/issues/issue_execution.rs` resolves `predecessor_run` against a selected producing Flow/version/terminal/outcome schema, checks Issue applicability and builds a handoff. `issue_execution_publication.rs` retains recoverable request/publication/admission and association through the shared Project text intake.
- Automatic intake uses the same predecessor contract through `server/app/src/runtime/sources/`; stable input identity already prevents edits and label churn from creating new automatic cycles.
- `factory_presets/delivery.rs` projects specific triage payload paths into reproduction and requirements input. `IssueExecutionForm.tsx` asks for an earlier Issue Run. The Issue property editor already consumes Project-owned definitions, while local classification authoring currently exposes priority and size-label mapping.
- The main UI spec already forbids duplicate operator-maintained document component/priority lists. The new path must reuse Project field contracts, not add another classification form vocabulary or an arbitrary prose-to-schema compiler.

`server/factory-ticket-delivery-preset` is still owned by the implemented, unarchived `add-factory-ticket-delivery-pipeline`. Its exact-predecessor requirement is deliberately superseded here. Synchronize/archive that capability before synchronizing this change's renamed/modified requirements; leave its historical planning and immutable publications untouched.

## Goals / Non-Goals

**Goals:** One canonical current classification on the Issue; usable Project field administration for ordinary and classification properties; Issue-owned documents with separate Project catalogs and optional links to shared documents; explicit comment/document selections; atomic mutation with provenance; delivery independent of producer topology alongside explicit producer-result inputs; shared manual/automatic snapshot validation; immutable selected inputs and existing execution isolation/recovery.

**Non-Goals:** Another scheduler, execution journal, comment system or duplicate content-byte store; a second mutable classification document; unbounded per-Issue schemas or generic nested JSON planning properties; a general binary-upload subsystem; automatic promotion of Issue attachments into shared Project documents; a new dependency-graph engine; free-text approval authority; automatic ingestion of an entire discussion; implicit source/provider writes; importing every old result; automatic migration of published Runs; AI generation of arbitrary schemas; merge or deployment automation.

## Decisions

### 1. Keep effective values with the Issue and store provenance alongside their revisions

The existing Issue owner remains the sole writer of effective values. Keep priority in its existing content field and component/size/kind and other supported scalar choices in existing Project-defined planning properties. Associate the relevant property IDs with classification semantic slots in the versioned Project definition publication; reuse that publication for edits, configured outcome bindings and consumer inputs. Stable IDs carry semantics; display names and document prose do not dispatch behavior.

Classification is an optional subset of Issue data. Project-specific fields need no classification slot and retain all existing supported simple property types. Project settings administer the same catalog through current-version publication: add definitions/choices, rename display labels, retire identities and edit compatible bounds with review. Keep Project `Administer` separate from Issue `Update`, preserve referenced keys/types and old publications, and explain incompatible changes rather than rewriting stored values. The new UI builds on task 1.1 without reopening its completed contract.

Extend the Issue's bounded typed classification metadata for field attribution, assessment applicability and reproduction/dependency evidence references. Do not duplicate effective scalar values in an independently mutable assessment store. History can retain the full original proposed result and application receipt; consumers resolve the applied Issue revision, never pick an arbitrary historical proposal as current data.

The classification contract covers kind, component, size, priority, severity, risk, route recommendation, preliminary reproducibility and the currently supported bounded dependency/evidence references. Do not widen generic planning values to arbitrary objects or require operators to type Work/Run IDs. Retain existing exact dependency references through an Issue-owned typed attachment with authorized selection; introducing new cross-Issue scheduling/relationship semantics is outside this change.

Project definitions are the structural source for choices/bounds. Documents explain assessment rules. A step selects target fields and the compiler reuses their published types; it does not ask the operator to enter the same enum again. Text such as P1 or "large" is interpreted only under an explicit supported field/semantic mapping. Do not silently rescale priority, equate a display label with a routing key, or let a model redefine the contract.

**Alternative rejected:** Keep triage output and Issue properties as competing current classification copies. They diverge after manual edits and require a producer selection to decide which is authoritative.

### 2. Apply classifications through one guarded Issue command

Extend `ApplyIssueClassification` and the existing trusted action instead of granting the AI a general Issue-write tool. The model produces an assessment; deterministic validation accepts it; the configured action proposes only explicitly selected field updates. `Apply classification to Issue` remains separate from `Show on Issue` and external Reporter effects and defaults off for an unconfigured outcome.

Application checks current principal/Project/Issue authority, source-owned fields, exact definitions, requirements applicability, code-bound evidence and the baseline of selected fields. Commit valid selected values, field origins, immutable Issue history and the original operation receipt in the existing transaction. Unknown values are omitted/preserved rather than fabricated. A field conflict rejects the selected application atomically, keeping the accepted assessment inspectable without presenting it as applied. Existing priority and size-label effects retain their original contract; a new typed size binding does not implicitly add or remove labels.

Retain existing ordering fences so an older, slower assessment cannot overwrite a newer applicable assessment. Intervening conflicting manual changes take precedence. A manual edit uses the existing authorized Issue edit/planning command with the same types, ownership, version and audit rules. It supplies human attribution, not a made-up producer Run. Neither origin confers execution readiness by itself.

**Alternative rejected:** Put `accepted: true` or a ready-looking label in editable metadata and use it as permission to execute. Those values do not establish current authority or required evidence.

### 3. Track applicability by the inputs an assessment actually depends on

Retain the Issue's reviewed requirements/applicability version with each assessment and the exact code subject, environment, criterion and observation identity for evidence-dependent claims. Retain any explicitly declared assessment dependencies on selected material identities/digests. Material requirements or relevant workflow/subject/material changes invalidate dependent claims. Assignee, due date, an unselected new comment, a new publication of an unselected document and unrelated planning changes do not automatically mark classification stale.

Expose current, missing, stale and conflicting assessment/evidence conditions as derived read facts with bounded reasons. A human override changes the selected effective field and its attribution; it does not renew other stale fields or turn unverified reproduction into proof. A snapshot consumer declares which current fields and evidence it needs. A complete unresolved defect can take the declared reproduction route; missing required input or an unsupported claim blocks admission. The system must not silently downgrade evidence and claim a verified bypass.

Distinguish classification applicability from request concurrency: a nonmaterial edit need not stale the assessment, but a manual execution request still checks its exact reviewed Issue version and requires refreshed review after a version conflict.

**Alternative rejected:** Invalidate everything on any Issue revision, or treat the newest result by completion time as current. The former blocks harmless edits; the latter can revive an assessment of superseded requirements.

### 4. Declare consumer data requirements, not its producer

Add a versioned generic Issue snapshot input binding to the existing Flow/Configuration model. It selects built-in requirements/acceptance fields, ordinary Project properties by stable ID, classification semantic fields, explicit Issue materials and permitted evidence projections, with declared required/optional values and supported readiness rules. Ordinary property selections reuse their published scalar types and bounds without requiring a classification semantic slot. Publication resolves Project field/definition identities and validates mappings. Missing binding or incompatible types are authoring/readiness failures.

The normal ticket-delivery example selects this mode. It no longer requires `accepted_predecessor`, `/assessment` paths from another Flow, or a user-selected `predecessor_run`. A human-prepared Issue and an Issue prepared by a differently named/versioned triage Flow satisfy the same consumer contract when their effective fields and evidence are valid.

The generic snapshot binding must not change intra-Flow accepted-result/data connections or explicit pipeline-to-pipeline accepted-result inputs. Keep exact-producer mode selectable in newly authored publications as well as old ones. Reject competing sources for the same classification input in a new publication rather than guess precedence; independently declared non-overlapping inputs remain governed by the existing input grammar. A source that cannot resolve an authorized canonical Issue cannot satisfy an Issue-snapshot target; source-only Flows remain usable under their declared contract.

Material selection is opt-in. A consumer declares exact material entries or a bounded approved Issue-material set, required/optional handling and per-step projections. The shared ceiling is 32 distinct references and 1 MiB of combined selected UTF-8 text, with stricter consumer bounds and existing per-source limits retained. Manual review confirms that selection; automatic intake requires a separately published selection policy resolving the Issue's explicit selections. Neither mode selects all comments, all Issue documents, the newest comment or every Project document by default. Text is reference/task-context data under the declared mapping; it does not gain controlling instruction, evidence or approval authority from its source. Material requirements corrections use the ordinary reviewed Issue edit and gate contracts.

**Alternative rejected:** Automatically choose the newest matching triage Run behind the old selector. It hides producer coupling and still prevents a human or alternate process from preparing the canonical task.

### 5. Freeze the selected Issue data through existing admission

Extend the existing Issue execution intent and Project text input preparation with an immutable snapshot reference/digest. The retained snapshot includes:

| Input | Retained identity |
| --- | --- |
| Issue requirements and implementation acceptance | Project/Issue, reviewed revision/applicability and content digests |
| Other selected built-in/Project properties | Stable field IDs, scalar values and exact catalog/type/bounds versions |
| Effective selected classification | Field values, exact definition/semantic-binding versions, attribution and applicability |
| Selected comment/document text | Exact Issue material selection, source kind and owner (Project/Issue), comment identity/digest or exact Issue-/Project-owned document publication and Artifact/digest/size, frozen bytes and original safe attribution |
| Selected dependency/reproduction material | Authorized bounded references, subject/criterion/environment and observation identities |
| Execution target and code | Input-contract version/digest, Configuration and Repository policies, resolved immutable base revision |
| Admission | Selected launch/access and prerequisite policies, checked result and trusted admission classification |

Do not read the live Issue anew at every step. Compile only each step's explicit projections from this retained snapshot into its Context. Issue visibility is not a grant to read every attachment, private investigation or protected test. Provenance can refer to a triage Run, but delivery needs access to the Issue and selected evidence, not unrelated producer logs or transcripts.

Preparation resolves selected bodies through independently authorized readers for their actual owners and can publish immutable bytes before the final database commit using existing recoverable intake receipts. Final admission rechecks the reviewed Issue/classification/definition/material selection and selected-content availability, and pins the resolved subject in the transaction that publishes the Work/Run association. A concurrent selection change fails without a mixed snapshot or implementation dispatch; an unrelated appended comment does not alter the selected input. Recovery of an already admitted identity returns its original snapshot even if the Issue or either document catalog later changes. Current runtime authorization and content-retention checks continue to apply; expired bodies are not restored by replay or replaced with newer content.

Keep effective Issue priority/risk distinct from the immutable scheduling/admission `WorkClassification`: retain the exact policy mapping once at admission. The consumer adapter can construct its internal acceptance/routing envelope after these checks; the agent and user do not fill technical `accepted` or `admission_risk` envelope fields to manufacture authority.

**Alternative rejected:** Let delivery repeatedly fetch the latest Issue. A size edit could switch requirements branches midway through implementation and a retry could run against different acceptance criteria.

### 6. Share manual and automatic validation without changing replay identity

Manual review selects the exact Issue version and target; the server resolves and verifies snapshot values. A distinct intentional request creates a distinct execution. A transport retry or node retry retains its original request/Run.

Automatic Source intake resolves its scoped native link to the canonical Issue, applies the same snapshot builder and prerequisites, and stores preparation under the existing stable input/item identity. An unadmitted item can wait for applicable classification and later reconcile. Other items can progress. Once admitted, edits to fields/labels or input/Configuration versions do not create a new cycle. A new deliberate manual request or explicitly authorized new automatic input identity uses the existing lifecycle rules. Do not put classification digests or polling timestamps in the automatic Work identity.

Triage completion/application, comment append and document publication/attachment never implicitly enable intake. Selection labels/statuses can find candidates but do not bypass launch, requirements, material availability or evidence checks. Automatic preparation uses the same bounded explicit material selection; later comments, material edits and document publications do not create another processing identity after admission.

### 7. Reuse Issue and Factory UI surfaces

Show effective classification in Issue Properties with definition-backed controls, origin/freshness and authorized history. Keep proposed/unapplied assessment cards distinct from committed fields. Review local classification effects in the existing Flow inspector using selected Issue fields; select the new snapshot input in the delivery editor without an upstream Flow/Run picker.

Add Project settings field administration with supported type/bounds/choice controls, stable-ID review, optional semantic bindings, compatible rename/retirement and conflict handling. An ordinary Issue edits its own values from that catalog. Authors choose fields in the Factory input editor without copying their schema or typing technical IDs.

Reuse Activity for comment append/read/reply and add an explicit action to include a retained comment as Issue material. The Issue Documents view creates, lists, previews and publishes new immutable versions of documents owned by that Issue. Reuse suitable document form/reader components with explicit owner context; creation stays in the Issue and never publishes into Project settings. A separate link-existing-Project-document action discovers authorized shared documents and attaches an exact reference without changing their owner or content. Show owner and exact version for both kinds, support reviewed selection/replacement/removal and preserve selections when newer content is published. Creation/publication, material selection and execution remain distinct; creating an Issue document makes it visible in that Issue's Documents view but does not select it as pipeline input. Show input eligibility/unavailability in the selection preview, not a generic chat/start button.

The start form previews selected fields and exact comment/document entries, byte/count bounds, field/material-specific blockers and the selected target. Display earlier-run selection for an explicitly chosen producer-result target; keep that mode authorable. Execution details show the retained snapshot and any difference from current Issue data or newly published materials. Keep profile permissions, protected test isolation, requirements gates, current candidate review/validation and confirmed PR receipt logic unchanged.

### 8. Preserve versioned legacy contracts and stage the rollout

Use additive, versioned schemas and storage fields with optional/legacy decoding. Publish new triage application and ticket-delivery example versions instead of changing immutable `v1` identities/digests. Old requests retain their exact predecessor checks; do not reinterpret or silently republish old Flows or Configurations. The explicit upgrade is a new reviewed Flow and Configuration.

Do not automatically promote every old triage result or visible card into canonical classification. An authorized operator can reapply an eligible exact assessment through the new guarded command or rerun triage; absent lineage remains unknown. On rollback, select the previous Configuration for new work and retain existing data/Run readers; do not destructively remove the new Issue history or downgrade running snapshots.

### 9. Separate document ownership from shared content storage and material selection

Represent document ownership explicitly in retained metadata: a shared document belongs to a Project; a task document belongs to an exact Project/Issue. New publications retain that owner and immutable content identity. Issue-scoped commands/queries create, list, version and read Issue documents under the parent Issue's mutation/visibility rules and independent content checks. Reuse the existing content-byte store, digest/size validation, recoverable publication/replay and retention/cleanup mechanisms; add the required owner metadata and checks instead of another mutable body store. Byte deduplication or a known Artifact ID grants no cross-owner access.

Project document catalogs/selectors and their content endpoints expose only Project-owned publications. Issue documents are absent there, including when the caller can access both owners. Issue endpoints enforce the exact parent Issue before metadata/body reads or replay; another Issue in the same Project cannot link them as its own material. Issue document writes use Issue update authority, without requiring publication into the shared Project catalog. Linking an existing same-Project document remains explicit and preserves its Project owner and independent content authorization. It neither copies nor promotes documents between owners; automatic sharing/promotion is outside scope. Existing Project publications keep their original ownership and references.

Extend immutable Issue revisions with an optional bounded typed material-selection collection, empty for old and ordinary Issues. Entries distinguish an existing same-Issue comment identity/digest, an exact same-Issue document publication or an explicitly linked exact same-Project document publication. Document entries retain source/owner identity, publication and Artifact/digest/size. They contain safe selection metadata and attribution, not another editable body. The existing Issue mutation owner enforces visibility, exact Issue-version preconditions, scoped replay and atomic history; selecting/removing a reference neither edits its source nor publishes Work. Dedicated readers continue to enforce comment/document content access independently of Issue visibility.

Appending a comment or creating/versioning an Issue document leaves the material selection unchanged until an authorized explicit selection. Publishing a newer Issue or Project document version leaves an existing selection pinned until explicit replacement. Removing a selected reference neither deletes source content nor transfers ownership; retained historical selections and admitted snapshots keep their exact identities under the existing content/input retention rules. Attaching a document alone does not supply it to every pipeline or step: a consumer contract and reviewed/published selection are still required. Capture selected bytes/digests once through managed intake, keep bounded source provenance and apply retention without resurrecting expired bodies. Required unavailable material blocks preparation/admission; optional omission is explicit in review and the retained snapshot, never an empty fabricated body or silent fallback.

This deliberately refines the old blanket exclusion of discussion from execution inputs: append/read remain non-executing, while a separate authorized snapshot selection can supply text as data to a new execution. Existing comments need no migration into input membership, human gates remain typed and explicit, and no chat session or comment-triggered agent is introduced.

### 10. External precedents inform the separation, not its guarantees

- [OpenHands Software Factory](https://docs.openhands.dev/openhands/usage/use-cases/software-factory) documents independent triage and developer automations communicating through an implementation-ready Issue. Its [Issue-to-PR contract](https://docs.openhands.dev/openhands/usage/agent-canvas/prebuilt/issue-to-pr) reads task requirements/dependencies and starts a separate conversation. These pages do not promise immutable Issue revision semantics.
- [GitHub Copilot task assignment](https://docs.github.com/en/copilot/how-tos/copilot-on-github/use-copilot-agents/kick-off-a-task) supplies the Issue title, description and existing comments at assignment; later Issue comments are not automatically consumed. This supports Issue-based initiation with execution-specific input, not a claim about GitHub's internal snapshot implementation.
- [Linear AgentSession](https://linear.app/developers/agent-interaction) separates Issue context from the execution session and its activities. OctaCity retains its existing Work/Run owner for that role.

## Risks / Trade-offs

- **Broader Issue mutation surface** -> Reuse current authorization, source ownership, preconditions, action claims and receipts; keep outcome display, application and external reporting independent.
- **Field values and provenance diverge** -> Persist them in one Issue revision/transaction and test manual, source and Factory updates through the same owner.
- **A forged or stale reproduced value skips testing** -> Require independently applicable evidence and check it against the admitted subject; unresolved routes are explicit, and later protected validation is unchanged.
- **A blocked item is admitted after an unrelated edit with mixed data** -> Revalidate and atomically freeze the exact reviewed snapshot at final admission, then recover that original input on replay.
- **New classification duplicates an admitted automatic run** -> Preserve stable input-scoped identity; require an explicit new processing request/cycle.
- **Legacy specs or saved configurations contradict the new model** -> Explicitly order spec synchronization and publish new versioned examples; qualify legacy decoding, admission and recovery separately.
- **More records must remain available for explanation** -> Reuse existing Issue, input Artifact, evidence and execution retention owners; retain safe identities and honest unavailable states without resurrecting expired bodies.
- **Field administration changes the meaning of historical values** -> Preserve stable IDs/types and exact catalog history; review compatible bounds/retirement and reject silent remapping.
- **A selected comment or document leaks into every step or becomes implicit authority** -> Require explicit bounded consumer and per-step selections, independent body reads, ordinary role/policy checks and immutable provenance.
- **A new comment or document publication changes a reviewed execution behind the operator** -> Pin exact references/digests and selected bytes, fence the Issue material selection at admission and preserve stable automatic identities.
- **Ticket documents clutter Project catalogs or leak across owners** -> Persist and authorize the exact Project/Issue owner at publication, discovery, content reads, selection and replay; qualify exclusion from Project catalogs and rejection of same-Project cross-Issue references. Reuse only content storage/recovery primitives.

## Migration Plan

1. Review this proposal and its explicit supersession of the predecessor handoff. Before synchronizing these deltas, synchronize/archive the completed `add-factory-ticket-delivery-pipeline` capability; do not rewrite its planning history as part of this change.
2. Preserve completed tasks 1.1–1.3. Add Issue-owned document publication/metadata and legacy-empty typed material references using existing migration conventions and shared content storage/intake. Keep existing Project documents Project-owned and old Issue selections empty; do not backfill attachments or reassign ownership. Qualify separate catalogs, owner-scoped authorization, immutable history and replay before snapshot consumption. Keep legacy fields/readers valid and publish additive generated contracts.
3. Qualify guarded application, manual editing, stale/conflict behavior and exact replay first. No existing classification card becomes authoritative through migration alone.
4. Add shared field/material snapshot preparation/admission and explicit Source selection/waiting/replay; qualify no-producer admission and exact-producer authoring/recovery side by side, including selected-body access/retention failures.
5. Publish new example and UI choices. On a disposable stand, create Project fields through settings, explicitly configure triage application, apply one classification, create/select an Issue document, separately link a shared Project document, select a comment and start delivery without selecting a triage Run. Verify the task document is absent from Project catalogs and inaccessible through another Issue. Publish newer text and confirm the running snapshot is unchanged. Then qualify a manually classified Issue and separately enabled automatic intake with explicit material selection.
6. Roll out by selecting new Flow/Configuration versions. Retain old publications and use them for rollback of new work; preserve all admitted snapshots and current authorization enforcement.
