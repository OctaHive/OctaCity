## ADDED Requirements

### Requirement: Issue classification is canonical task data with attributable history
An Issue SHALL support effective classification values independently of a particular producer pipeline. Ordinary manual Issues SHALL remain valid without classification. Project property definitions and Project-owned Issue priority choices SHALL supply stable field/choice identities, types and versions shared by Issue editing, configured triage effects and execution inputs. Operators SHALL NOT have to repeat Project component, size or priority vocabularies as per-step result enums. Human-readable guidance SHALL remain distinguishable from mechanically validated field definitions.

Classification SHALL remain an optional subset of Issue attributes. Priority, component, type and size SHALL be built-in attributes. Each Project SHALL publish its own bounded priority/component/type choices in the shared immutable definition catalog; selecting a built-in attribute SHALL NOT require a custom property or semantic binding. Priority SHALL initially offer `blocker`, `high`, `medium`, `low`. New selected actions SHALL store Project choice IDs in canonical `content.priority`; legacy numeric values and receipts SHALL keep their original encoding without an inferred rescale. Work scheduling SHALL remain a separate numeric contract mapped by an explicit admission policy. Component changes SHALL use the ordinary Issue planning mutation, exact catalog/current-version checks, field ownership and scoped replay. Existing Issues SHALL decode without a component. Prior custom component values/bindings SHALL retain their historical meaning without automatic copying into the built-in field. Ordinary Project-specific properties SHALL retain their supported text, choice, boolean, bounded integer and date types without requiring classification semantics. Definitions SHALL be shared within a Project while each Issue retains its own optional values. Remaining classification semantic bindings SHALL continue to reference distinct supported choice properties; custom fields SHALL NOT redefine built-in priority, component or execution authority.

Built-in Issue type SHALL use Project-specific versioned choices with initial IDs `defect` and `feature_request`. Built-in size SHALL support `small`, `medium` and `large`. Type and size SHALL remain optional on ordinary and legacy Issues; available default choices SHALL NOT automatically classify an Issue. Earlier kind/size custom properties and semantic bindings SHALL remain historically readable without automatic promotion. New type choices SHALL NOT infer execution routing from names. The configured assessment `kind` SHALL address the built-in type for new explicitly selected applications.

Changes SHALL retain actor or trusted source attribution, applicability to requirements and any relevant code/evidence subject, and an immutable history. An authorized manual edit SHALL use the same field ownership, value validation, current-version and replay rules; it SHALL NOT fabricate a triage Run or claim independently verified reproduction. Missing or unknown assessments SHALL remain explicit. Generic metadata fields named accepted, approved or ready SHALL NOT confer execution or verification authority.

#### Scenario: Projects define different components
- **WHEN** two Projects publish different component catalogs and classify their Issues
- **THEN** each Issue uses the built-in component field with only its own Project's exact active choices, independently of custom properties

#### Scenario: A manually prepared Issue has no triage Run
- **WHEN** an authorized operator supplies all classification values required by a target under the current Project definitions
- **THEN** those values are available for execution review without manufacturing an upstream execution, while the target's independent admission and evidence checks still apply

#### Scenario: Definitions evolve after an Issue was assessed
- **WHEN** a Project changes or retires a classification choice after its use
- **THEN** prior revisions remain explainable by their original definitions and new execution review reports any current incompatibility without silently remapping values

### Requirement: Classification applicability distinguishes assessment from evidence
The Issue SHALL expose whether selected classification data remains applicable to the current requirements and, for evidence-dependent claims, the selected code revision, environment and criterion. Material requirements changes SHALL make dependent assessments stale. Unrelated planning changes such as assignee or due date SHALL NOT by themselves invalidate classification. A manual override of one field SHALL retain its own attribution and SHALL NOT renew other stale assessment or evidence claims.

An assessment depending on selected comment/document materials SHALL retain those declared source identities/digests and report incompatibility when the relevant reviewed dependency changes. Unselected comments, unrelated property edits and new unselected document publications SHALL NOT renew or invalidate that assessment merely by appearing.

Reproduced and not-reproduced claims SHALL retain references to independently applicable evidence; model text, arbitrary IDs, user-entered booleans and technical failures SHALL NOT establish those claims. Missing, inaccessible, expired, inconclusive or wrong-subject required evidence SHALL produce an actionable admission blocker or the target's explicitly supported unresolved route. Private evidence bodies and transcripts SHALL retain their existing independent access controls.

#### Scenario: Acceptance criteria change after triage
- **WHEN** the Issue's expected implementation behavior changes after classification
- **THEN** the affected assessment is shown as stale and cannot satisfy a target that requires a current assessment until it is explicitly refreshed

#### Scenario: Responsibility changes without changing the work
- **WHEN** only the organizational assignee or due date changes
- **THEN** the prior applicable classification remains available and no execution is started or invalidated solely by that edit

#### Scenario: Reproduction evidence refers to another revision
- **WHEN** a stored reproduced claim has no applicable evidence for the new delivery subject
- **THEN** the claim cannot bypass the configured reproduction requirements merely because the Issue field contains reproduced

### Requirement: Task documents belong to their Issue independently of shared Project documents
An authorized operator SHALL be able to create text documents for an exact Issue and publish new immutable versions from that Issue. Each publication SHALL retain its Project/Issue owner, document/publication identity, content digest, encoded size and actual actor attribution with scoped replay and retained history. Issue visibility/update authority and independent content authorization/retention SHALL govern discovery, publication and body reads. Project membership or possession of an Artifact ID SHALL NOT grant access to another Issue's document.

Issue-owned documents SHALL appear in their owning Issue's Documents view and SHALL NOT appear in Project document catalogs, discovery or step document selectors, including for a caller authorized for both owners. Project document endpoints SHALL NOT return Issue-owned metadata or bodies. Linking an existing authorized same-Project document to an Issue SHALL remain a separate explicit operation preserving its Project owner, exact publication and independent access checks. Creating or linking documents SHALL NOT implicitly select execution input, start a Run or promote an Issue document to shared Project ownership. Existing Project publications SHALL retain their owners and references.

#### Scenario: Operator creates a task-specific document
- **WHEN** an authorized operator publishes a document from an Issue
- **THEN** it is retained with that Issue as owner and listed there, absent from Project document catalogs/selectors, and not automatically selected for execution

#### Scenario: Another Issue attempts to use the attachment
- **WHEN** a caller authorized for another Issue in the same Project submits the document identity for read or material selection without the required owner scope
- **THEN** access is denied without exposing metadata/content or associating the document with the other Issue

#### Scenario: Project document access is used for an Issue attachment
- **WHEN** a caller requests an Issue-owned document through Project document discovery or a Project content endpoint
- **THEN** it is not listed or returned even if the caller can otherwise access the owning Issue

#### Scenario: Issue document publication is retried
- **WHEN** an authorized publication committed but its acknowledgement was lost and the exact operation is replayed in its original scope
- **THEN** the original Issue-owned publication is returned without another version, another owner or a Project catalog entry

### Requirement: Issues retain explicit selections of comments and owned or linked documents
An authorized operator SHALL be able to select retained same-Issue comments, exact same-Issue document publications and explicitly linked exact same-Project document publications as bounded Issue materials. A selection SHALL retain source kind, exact source/owner identities, content digest, safe attribution and immutable Issue history through existing current-version, ownership and scoped replay rules. A document reference SHALL also retain its exact publication/Artifact identity and encoded size. There SHALL be at most 32 distinct material entries per Issue across source kinds; duplicates, cross-Issue/foreign references, forged owners/digests and inaccessible selections SHALL be rejected atomically. Generic lists SHALL expose safe metadata only, with independently authorized body reads.

Existing operator discussion SHALL remain the comment body owner; document publications SHALL retain their actual Issue or Project owner. Comment append and document creation/publication SHALL NOT automatically select material or start execution. Explicit selection/replacement/removal SHALL change only the Issue's reviewed references, never transfer document ownership or delete source content. New document publications SHALL NOT retarget an existing selection or alter retained history and admitted snapshots. Old Issues SHALL decode with empty selections and SHALL require no document or comment setup.

#### Scenario: Operator attaches a Project document
- **WHEN** an authorized operator selects a visible exact published Project document for the reviewed Issue revision
- **THEN** one Issue mutation retains its Project-owned exact reference and original receipt without copying an editable body, attaching it to every pipeline or starting execution

#### Scenario: Operator selects an Issue-owned document
- **WHEN** an authorized operator selects an exact document publication owned by the reviewed Issue
- **THEN** the Issue retains its owner/publication identity and digest as material without adding a Project document or admitting an execution

#### Scenario: A new document publication follows attachment
- **WHEN** changed content is published for an Issue-owned or linked Project document already selected by an Issue
- **THEN** the Issue retains its earlier exact reference until explicit replacement and prior revisions remain explainable

#### Scenario: A selected reference is removed
- **WHEN** an authorized operator removes a document from the current material selection
- **THEN** source ownership/content, historical references and admitted snapshots remain unchanged under their retention rules

#### Scenario: A comment is posted without input selection
- **WHEN** an operator appends a comment or reply to an Issue
- **THEN** discussion records it under the existing contract while the Issue's material selection and all admitted inputs remain unchanged

#### Scenario: Material selection exceeds its scope or bounds
- **WHEN** a mutation includes a hidden, cross-Issue or foreign source, forged owner, mismatched digest, duplicate entry or more than 32 selections
- **THEN** it fails without a partial association, source-body disclosure or execution side effect

## MODIFIED Requirements

### Requirement: Comments cannot become Factory authority
Reading or appending discussion SHALL NOT invoke a model, start/retry execution, reserve WIP, grant approval, choose a transition or update an admitted Context. Discussion SHALL remain outside execution inputs by default. A separate authorized execution request or explicitly configured automatic input SHALL be able to include bounded, explicitly selected same-Issue comment text in a new immutable Issue snapshot, retaining exact source identity/digest and independent body authorization. No discussion thread SHALL automatically enter a later model's Context; an unselected appended comment SHALL NOT change a reviewed material selection.

Selected comment text SHALL be task-context/reference data under the declared input mapping and SHALL NOT establish evidence, approval, controlling-policy authority or a human-gate response. Clarification for a waiting gate SHALL still require an independently confirmed schema-valid response to that exact declared gate, consumed through the existing owner contract. Changes to authoritative requirements SHALL still use the reviewed Issue content/edit contract.

#### Scenario: Comment mentions an agent or asks for approval
- **WHEN** its text asks an agent to retry, approve or deploy
- **THEN** it remains operator discussion and creates no execution/control intent

#### Scenario: Operator answers a waiting clarification
- **WHEN** the operator separately confirms the exact current gate's declared response
- **THEN** the ordinary control command records immutable intent and the Flow owner later consumes it without importing the whole discussion

#### Scenario: Operator explicitly selects a comment for a new execution
- **WHEN** an authorized reviewed snapshot request selects a permitted retained same-Issue comment
- **THEN** its exact text/digest is frozen as declared task-context data without importing other comments, granting approval or changing an existing Run

### Requirement: Starting execution freezes a reviewed Issue revision
An authorized execution request SHALL bind the reviewed Issue version and content digests to an exact visible Factory target, repository/base policy and versioned task/acceptance mapping. For an Issue-snapshot target it SHALL also freeze selected built-in/Project property values, effective classification, exact Project definitions, classification applicability/provenance, selected comment/document bytes and source identities/digests, and authorized evidence references. No producing Flow, terminal, outcome or triage Run selection SHALL be required. The Issue revision, selected fields and material references SHALL be checked again at final admission so asynchronous input preparation cannot admit a substituted or stale snapshot. Selected bodies SHALL use independent current authorization and retention checks; their combined UTF-8 content SHALL be at most 1 MiB and within any stricter published consumer bound. It SHALL validate ordinary admission authority and required inputs, publish immutable Project-owned inputs through the shared recoverable intake boundary, and admit Work through existing scoped replay. Publication, admission and Issue association SHALL recover to the same logical request after restart or lost acknowledgement. A request SHALL NOT be reported admitted before its exact Work/Run association is durable. A later Issue edit, comment append or document publication SHALL NOT alter queued or running Work, Context, profile, priority or retries.

#### Scenario: Requirements change during execution
- **WHEN** an operator edits an Issue after its first execution is admitted
- **THEN** the Issue exposes the new revision while the first execution retains its original input and is labeled as using that earlier revision

#### Scenario: Admission committed before association acknowledgement
- **WHEN** a request is retried after its admission committed but its association or response was interrupted
- **THEN** recovery attaches and returns the original Work/Run without another publication identity, base resolution or admission

#### Scenario: Acceptance input or reviewed version is invalid
- **WHEN** the selected pipeline requires missing acceptance input or the reviewed Issue version is stale
- **THEN** the request is rejected with an actionable validation/conflict result and no silently updated or fabricated execution input

#### Scenario: Classification changes during input preparation
- **WHEN** the reviewed classification or requirements changes before a new Issue-snapshot execution commits admission
- **THEN** admission reports a conflict or failed prerequisite without silently taking newer values or publishing a Run for a mixture of revisions

#### Scenario: Execution resumes after a later Issue edit
- **WHEN** an admitted Issue-snapshot request is replayed after the Issue has changed
- **THEN** it returns the original Work and snapshot without re-reading current values as replacement inputs

#### Scenario: Selected material changes while admission is pending
- **WHEN** a reviewed Issue material selection is replaced before admission commits
- **THEN** admission requires fresh review and publishes no Run with a mixture of old fields and newly selected text

#### Scenario: New discussion arrives after review
- **WHEN** an unselected comment is appended after snapshot review or admission
- **THEN** it is absent from the selected snapshot and neither substitutes input nor creates another execution

#### Scenario: Selected text is unavailable or exceeds its aggregate limit
- **WHEN** required comment/document content is inaccessible, expired, digest-incompatible or above the declared count/byte bounds
- **THEN** preparation reports a bounded material-specific blocker without guessing another source or fabricating empty text


### Requirement: Explicit triage classification can update local Issue planning

An explicitly enabled triage action SHALL apply selected accepted classification fields to the exact associated Issue through existing guarded Issue mutation rules. Supported fields SHALL include kind, component, size, priority, severity, risk, recommended route and preliminary reproducibility with authorized dependency/evidence references. Effective values SHALL reuse the existing Issue content/planning fields and exact Project definitions; dedicated bounded classification metadata SHALL retain applicability and evidence without a competing mutable copy of those values. New selected priority applications SHALL use active Project priority choice IDs; existing numeric priority/size-label effects SHALL retain their legacy contract. Legacy size-label effects SHALL remain supported separately from a typed size property. A size label SHALL resolve to an existing active Project label through a reviewed mapping. The action SHALL preserve unrelated labels and SHALL replace only a prior size label owned by the same configured classification policy. Unknown assessments SHALL leave the corresponding field alone.

The mutation SHALL verify current Project and Issue write authority, requirements applicability, ordering and the current values of fields the action intends to change. A conflicting operator edit or newer applicable classification SHALL take precedence over a stale action. The selected valid changes SHALL commit together in one Issue revision with field attribution, requirements/definition versions, Run and outcome provenance and scoped idempotent replay. A conflict or missing authority SHALL leave the accepted assessment readable for an authorized viewer. This local action SHALL produce no external provider intent and SHALL preserve the original Work scheduling priority.

#### Scenario: An authorized classification is delivered twice

- **WHEN** an accepted classification with priority and a mapped size is applied and the same operation is retried
- **THEN** the Issue retains one resulting revision, its priority and one owned size label, while unrelated labels are preserved

#### Scenario: An operator changes priority during triage

- **WHEN** an older outcome attempts to write priority after an intervening conflicting operator edit
- **THEN** the action reports a conflict and the Issue keeps the operator's edit

#### Scenario: Triage returns an unknown priority

- **WHEN** the accepted result has an unknown priority and a valid mapped size
- **THEN** the local action applies only the valid size update

#### Scenario: One accepted assessment updates the Issue
- **WHEN** a configured action proposes a valid component, size, priority and unresolved reproduction assessment for the same current requirements
- **THEN** the selected fields and their provenance commit together as one Issue revision without starting delivery or rewriting existing Work

#### Scenario: An older triage finishes after a newer assessment
- **WHEN** a stale classification attempts to replace a newer applicable assessment or an intervening conflicting operator edit
- **THEN** the Issue preserves the newer values and exposes the rejected application's conflict separately from the retained triage result
