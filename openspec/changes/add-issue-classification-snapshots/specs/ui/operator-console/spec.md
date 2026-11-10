## MODIFIED Requirements

### Requirement: Issue edits and execution start are distinct reviewed commands
The Console SHALL support preconditioned revision editing and a separate pipeline execution action. The start form SHALL select visible authorized targets, show the exact reviewed requirements revision, input mapping and resolved configuration/repository choices, selected classification/definition versions, evidence coverage and input freshness, and explain missing acceptance or execution prerequisites. Submission SHALL use a frozen confirmed intent and show pending input preparation/admission until the exact Work/Run link is returned. Transport retries SHALL preserve its original request. A later deliberate execution SHALL create a new request while node retry uses existing Run controls. Unsubmitted sensitive values and request keys SHALL remain in component memory only.

For an Issue-snapshot target the normal start form SHALL require no producing Flow, terminal, outcome or triage Run selection. It SHALL preview selected built-in/custom/classification fields, exact comment/document material selections, count/byte bounds, optional omissions and missing/stale/inaccessible required inputs, with links to authorized Issue editing or evidence detail. Earlier-run selection SHALL remain available for an explicitly selected producer-result target, including newly authored ones; changing targets SHALL not silently reinterpret a previous input choice.

#### Scenario: Requirements change while execution runs
- **WHEN** an operator saves a new task revision
- **THEN** the card shows that revision and labels active execution as using the previous snapshot without restarting or modifying it

#### Scenario: Execution start loses its response
- **WHEN** a confirmed request's transport acknowledgement is lost
- **THEN** the Console resumes the same request/status receipt and opens its original Run without a fresh command identity

#### Scenario: Reviewed version is stale
- **WHEN** the server rejects an edit or start request after another revision committed
- **THEN** the Console preserves the in-memory review context, shows the conflict and requires fresh review instead of silently substituting content

#### Scenario: Operator starts a manually classified Issue
- **WHEN** current Issue data satisfies a selected Issue-snapshot delivery target
- **THEN** it can review and start delivery without any prior triage execution, subject to the ordinary launch and evidence requirements

#### Scenario: Classification changes while the start review is open
- **WHEN** another actor changes a selected field before submission
- **THEN** the form requires a fresh review and does not silently accept the new classification

#### Scenario: A later comment follows input review
- **WHEN** an unselected comment is appended after the operator reviewed exact field/comment/document inputs
- **THEN** the preview does not silently include it and an admitted Run retains only its original selected sources and text

### Requirement: Activity discussion does not imply agent chat
Activity SHALL combine bounded safe execution/synchronization event references and operator-only plain-text discussion with actual server attribution and timestamps. Comments/replies SHALL use generated limits, separate append feedback and scoped replay; agent mentions or approval-like text SHALL remain non-executable discussion. A separate explicit material-selection action SHALL allow a permitted retained comment to be offered as task-context data to a new snapshot consumer, with exact-source review and no implicit launch or approval. Human clarification SHALL open a separate typed form for an exact declared gate. Bodies/response values SHALL not enter browser persistence, URLs, analytics or generic notifications, and unfinished drafts SHALL be discarded on leave/reload.

#### Scenario: Operator writes an agent mention
- **WHEN** a comment asks an agent to start work or approve a result
- **THEN** it appears as operator discussion with no execution/control success indicator

#### Scenario: Comment acknowledgement is lost
- **WHEN** the active append is retried
- **THEN** it uses the same frozen body/parent/key, preserves pending feedback and creates no duplicate comment

#### Scenario: Operator selects a comment as supplementary input
- **WHEN** an authorized operator explicitly includes a retained comment in the Issue material selection
- **THEN** the UI reviews that source and its selection separately from posting or starting execution, and the target's start form still confirms which material will be frozen


### Requirement: Flow authors can enable a card from an outcome

The outcome editor SHALL provide **Show on Issue** and use the supported outcome type to supply built-in titles, field labels and layout. For custom outcomes the editor SHALL allow a title and a brief conclusion field. The publication review SHALL show which slot and result fields will appear. A separate **Apply classification to Issue** setting SHALL default to off and SHALL show the selected Issue classification fields, current Project definitions and ownership constraints it can apply. Existing priority/size-label effects SHALL remain available for legacy publications. Supported field contracts SHALL be reusable without re-entering Project vocabularies as result enums. Project label mappings SHALL be reusable across Flows. The ordinary read-only display path SHALL require no mapping editor, additional graph step, provider connection or credentials.

#### Scenario: An operator enables the standard triage card

- **WHEN** the operator enables display on a compatible triage outcome and reviews publication
- **THEN** the review shows the triage card and its fields without requiring per-field mapping

#### Scenario: Author configures local classification application
- **WHEN** it selects a compatible accepted assessment and Project-owned classification fields
- **THEN** review shows the bounded local updates separately from Show on Issue and external Reporter effects, with no provider credential or downstream pipeline activation implied


## ADDED Requirements

### Requirement: Project settings administer shared Issue field definitions
Project settings SHALL provide authorized creation and compatible maintenance of Project-owned Issue property definitions using the existing versioned publication contract. Controls SHALL support text/choice/boolean/bounded-integer/date kinds, their authoritative bounds, choice IDs/display names, compatible display renames, retirement and optional classification semantic bindings. Priority, component, type and size SHALL be identified as built-in attributes; Project settings SHALL provide dedicated settings for the Project's own priority/component/type choices separately from custom property definitions. Classification SHALL remain an optional subset whose remaining semantic bindings target distinct supported choice properties. Operators SHALL not need hand-written JSON, duplicate per-pipeline vocabulary lists or per-Issue schema administration.

Review SHALL show stable identities, Project-wide availability, exact current catalog version and the effect of additions/retirement. Existing keys/types and retained history SHALL remain explainable. Current-version conflicts and incompatible changes SHALL retain the draft and require renewed review; publication SHALL not silently rewrite Issue values or start execution. Issue value editing SHALL remain separate from Project administration. EN/RU, keyboard and narrow-layout support SHALL cover these controls.

#### Scenario: An operator creates fields through Project settings
- **WHEN** an authorized administrator creates an ordinary environment field and a size choice field with an optional semantic binding
- **THEN** both appear in Issue Properties from the shared catalog, individual Issues can leave them unset, and a pipeline author can select their contracts without retyping types or choices

#### Scenario: A field is renamed or retired
- **WHEN** an administrator publishes a compatible display change or retirement after Issues already reference a field/choice
- **THEN** current editing explains the new availability while old Issue revisions retain their original definitions and no stored value is silently remapped

#### Scenario: An administrator configures Project components
- **WHEN** an authorized administrator opens the priority/component/type-list settings for a Project and creates, renames or retires an option
- **THEN** the reviewed versioned list supplies that Project's built-in Issue selectors independently of custom fields, and existing Issue values/history are not silently rewritten

### Requirement: Issues manage their own documents and explicitly link shared Project documents
The Issue workspace SHALL provide a Documents view with Issue-owned text creation, immutable-version publication, safe identity/digest/availability metadata, retained history and independently authorized previews. Creation/editing of a task document SHALL remain within the owning Issue and SHALL NOT publish into shared Project documents. Issue-owned documents SHALL be absent from Project document catalogs and step document selectors, even for an operator authorized for both owners.

An explicitly separate link-existing-Project-document action SHALL offer authorized exact same-Project publications and retain their Project ownership without copying/editing the shared source. The view SHALL distinguish Issue-owned documents from linked Project documents and show each selected publication's exact version and availability. Operators SHALL be able to select, explicitly replace or remove material references under current Issue-version review. Creation/publication, linking, input selection and execution SHALL remain distinct actions; creating an Issue document SHALL list it there without automatically selecting pipeline input, and removing a reference SHALL neither delete source content nor transfer its owner.

Newer Issue or Project publications SHALL not silently replace selected references. The view SHALL show missing, forbidden and expired states without presenting unavailable content as empty or automatically choosing a replacement. Material selection SHALL not supply a document to every pipeline or step. EN/RU, keyboard and narrow-layout journeys SHALL include create/version/preview/link/select/replace/remove and conflict recovery with in-memory drafts preserved.

#### Scenario: A task-specific document does not yet exist
- **WHEN** an operator creates and publishes text in an Issue's Documents view
- **THEN** the document is owned by and visible in that Issue, absent from Project catalogs/selectors, and requires explicit material/input selection before any pipeline can consume it

#### Scenario: An existing shared document is needed
- **WHEN** an operator separately links an authorized exact Project document publication to an Issue
- **THEN** the view labels its Project ownership and exact version while the Project catalog/source remains unchanged and execution does not start

#### Scenario: A task document receives a new version
- **WHEN** an operator publishes changed text for an Issue-owned document
- **THEN** the view retains its Issue ownership and version history while earlier selected references and admitted inputs remain pinned until explicit reviewed replacement

#### Scenario: A shared document has newer text
- **WHEN** a new Project publication becomes available after Issue attachment or Run admission
- **THEN** the Issue can review an explicit replacement while the old association history and admitted Run retain their original references/text

### Requirement: Issue snapshot authoring selects fields and text explicitly
The Factory input editor SHALL offer built-in Issue fields, supported Project properties, classification semantics and explicit Issue material selections using authoritative definitions and bounded selectors. A required classification binding SHALL be distinguished from an ordinary property selection. Exact producer-result input SHALL remain an authorable alternative with its existing selectors. Unsupported competing sources for one input SHALL have actionable validation rather than inferred precedence.

The Issue start form SHALL show which comment/document sources are offered by the Issue and which subset the target will consume, with exact identity/digest, authorized text preview, required/optional status and count/byte limits. No whole discussion or newly posted comment SHALL be included by default. Automatic input setup SHALL separately review the published bounded material-selection policy. Each step SHALL explicitly choose its own projections; comment prose SHALL not appear as an approval, gate response or permission grant.

#### Scenario: A pipeline uses one comment and one document
- **WHEN** its declared input selects those approved Issue materials and one stage selects their projections
- **THEN** review exposes exactly those sources, execution detail retains their frozen identities/text under independent access checks, and unselected comments/sibling stages receive no implicit material

#### Scenario: An author chooses a direct producer result
- **WHEN** an author creates a new pipeline using the existing exact accepted-result input mode
- **THEN** its producer/schema/Run choices remain available and are not replaced by an obligatory Issue snapshot selector

### Requirement: Issue classification is visible independently of its producer Run
Issue properties SHALL show the effective classification, safe origin and freshness alongside requirements and ordinary planning. Authorized manual editing SHALL use Project choices and current field ownership. The card SHALL distinguish current applied values, unapplied or conflicting recommendations, stale assessments and verified reproduction evidence. Earlier revisions and producing Runs SHALL be discoverable where authorized, without making a producer Run selector part of normal delivery setup.

Execution detail SHALL show the exact Issue snapshot used by each Work/Run and identify when the card now contains different data. Changes to the card SHALL not rewrite historical results or the frozen inputs of active Runs. English/Russian, keyboard and narrow-layout journeys SHALL cover classification editing, stale/conflicting data, snapshot start review and explicit producer-result target selection.

#### Scenario: Current size differs from the running execution's input
- **WHEN** the Issue's size is changed after delivery admission
- **THEN** properties show the current size while execution detail retains and explains its earlier snapshot without rerouting the Run

#### Scenario: Triage proposes an update that was not applied
- **WHEN** a triage result exists but its local classification action conflicted
- **THEN** the card keeps the effective Issue values and exposes the separate application failure rather than presenting the proposed values as committed
