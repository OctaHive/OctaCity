## ADDED Requirements

### Requirement: Issue classification is canonical task data with attributable history
An Issue SHALL support effective classification values independently of a particular producer pipeline. Ordinary manual Issues SHALL remain valid without classification. Project property definitions and Project-owned Issue priority choices SHALL supply stable field/choice identities, types and versions shared by Issue editing, configured triage effects and execution inputs. Operators SHALL NOT have to repeat Project component, size or priority vocabularies as per-step result enums. Human-readable guidance SHALL remain distinguishable from mechanically validated field definitions.

Classification SHALL remain an optional subset of Issue attributes. Priority, component, type and size SHALL be built-in attributes with Project-owned bounded immutable choice catalogs. Configured initial choices include priorities `blocker`, `high`, `medium`, `low`; default choices do not populate Issue values. Work scheduling SHALL remain separately and explicitly mapped. Custom fields SHALL retain supported text, choice, boolean, bounded integer and date kinds with shared Project definitions and optional per-Issue values. Field selections for filling and reading SHALL address fields directly by stable identity and exact definitions without fixed business roles. One current canonical representation SHALL replace superseded numeric-priority and fixed-slot development formats; no dual reader or compatibility adapter is required.

Built-in Issue type SHALL use Project-specific choices initially `defect` and `feature_request`; built-in size SHALL use ordered Project choices initially `small`, `medium`, `large`. The catalogs SHALL apply the same stable identity, finite bounds, retirement and new-model history/replay rules. Type and size SHALL remain optional values on ordinary Issues. Selected snapshot choice definitions SHALL be retained uniformly, including configured defaults, without special omitted-list/legacy-byte fallback behavior. Field/value names SHALL infer no route. An explicit source mapping SHALL select built-in type independently of the source alias.

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


#### Scenario: Project size choices differ and evolve
- **WHEN** a Project publishes ordered custom size IDs, renames/reorders or retires them and edits an Issue
- **THEN** only its active choices may be newly selected, unchanged retired values and old revisions remain readable, and another Project does not acquire these choices

### Requirement: Classification applicability distinguishes assessment from evidence
The Issue SHALL expose whether selected classification data remains applicable to the current requirements and, for evidence-dependent claims, the selected code revision, environment and criterion. Material requirements changes SHALL make dependent assessments stale. Unrelated planning changes such as assignee or due date SHALL NOT by themselves invalidate classification. A manual override of one field SHALL retain its own attribution and SHALL NOT renew other stale assessment or evidence claims.

An assessment depending on selected comment/document materials SHALL retain those declared source identities/digests and report incompatibility when the relevant reviewed dependency changes. Unselected comments, unrelated property edits and new unselected document publications SHALL NOT renew or invalidate that assessment merely by appearing.

Explicitly configured claim/value bindings SHALL require existing independently qualified observations for proof-dependent paths. Familiar field names or values SHALL confer no evidence semantics. Model text, arbitrary IDs, user booleans and technical failures SHALL NOT establish verified claims. Missing, inaccessible, expired, inconclusive or wrong-subject evidence SHALL block the configured path or follow its declared unresolved route. Existing readers SHALL retain independent body/transcript access; no separate general-purpose policy language is required.

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

## RENAMED Requirements

- FROM: `### Requirement: Explicit triage classification can update local Issue planning`
- TO: `### Requirement: Agent steps can fill selected Issue fields`

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


### Requirement: Agent steps can fill selected Issue fields

Any ordinary agent step SHALL be able to fill a bounded selection of existing writable core and supported Project text/choice/boolean/bounded-integer/date fields. One selection of stable identities and exact definitions SHALL derive both the agent form and the permitted save set. No fixed business roles, manual source/destination mapping, copied schema or user-authored apply step SHALL be required. The trusted owner SHALL apply only the successful accepted form result for that invocation; arbitrary foreign result import, multi-source field merging and clear-operation configuration are outside scope.

The existing Issue mutation owner SHALL atomically enforce the trusted Issue association, authority/source ownership, exact definitions/types, invocation baseline, accepted form/result identity and scoped replay. Each changed field SHALL retain actual Run/node/attempt/result and definition provenance. Required values SHALL be present; omitted optional fields SHALL preserve current values. Invalid, unknown or unselected fields SHALL reject the application without partial writes or implicit clearing. Analysis acceptance and successful save SHALL remain distinguishable; only the server receipt establishes application. Lost-ack retries SHALL reuse the same accepted result/save receipt without another model invocation. System/control/evidence state keeps its separate typed owners; filling fields grants no provider write, scheduling change or downstream launch.

#### Scenario: An authorized classification is delivered twice

- **WHEN** a successful generated field-form result selecting priority and size is applied and the same current-contract operation is retried
- **THEN** the Issue retains one resulting revision and original receipt for those fields while all unrelated values are preserved

#### Scenario: An operator changes priority during triage

- **WHEN** an older outcome attempts to write priority after an intervening conflicting operator edit
- **THEN** the action reports a conflict and the Issue keeps the operator's edit

#### Scenario: Triage returns an unknown priority

- **WHEN** the analyst cannot assess priority, priority is optional and omitted from the generated form result, while the selected size is valid
- **THEN** the local action applies only the valid size update

#### Scenario: One accepted assessment updates the Issue
- **WHEN** a configured action proposes a valid component, size, priority and unresolved reproduction assessment for the same current requirements
- **THEN** the selected fields and their provenance commit together as one Issue revision without starting delivery or rewriting existing Work

#### Scenario: An older triage finishes after a newer assessment
- **WHEN** a stale classification attempts to replace a newer applicable assessment or an intervening conflicting operator edit
- **THEN** the Issue preserves the newer values and exposes the rejected application's conflict separately from the retained triage result

#### Scenario: A non-triage pipeline fills supported fields
- **WHEN** an operator selects core size and Project text/choice/boolean/integer/date fields on an ordinary agent step without role setup
- **THEN** one generated form and guarded save retain their values and provenance without manual schema, source-path mapping or apply-node configuration

#### Scenario: CI analysis supplies field values
- **WHEN** an agent receives a retained CI report through its declared ordinary inputs and completes its selected-field form
- **THEN** the same trusted save path records those field values without deriving a mapping from CI keys or making the assessment a substitute for the trusted CI verdict

#### Scenario: A form contains invalid or unauthorized values
- **WHEN** a result has an incompatible value, missing required field, unknown/unselected identity or an intervening conflicting edit
- **THEN** no selected field is partially committed and the accepted analysis remains separately inspectable under ordinary access

#### Scenario: An optional form value is omitted
- **WHEN** a selected optional field is absent from an otherwise valid form result
- **THEN** its current value is preserved without a clearing operation or fabricated assessment for that field

#### Scenario: The model claims a save that did not commit
- **WHEN** analysis succeeds but the server denies, delays or rejects its field application
- **THEN** the Issue and execution view distinguish the accepted result from pending/failed application and never present model prose as a committed save
