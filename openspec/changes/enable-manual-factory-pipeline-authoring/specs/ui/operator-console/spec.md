## MODIFIED Requirements

### Requirement: Guided Factory Configuration management
The console SHALL provide typed guided creation and immutable-version replacement for Factory Flows and Configurations through one common pipeline-authoring journey. **Add pipeline** SHALL start an empty supported draft without a template, preset, existing Flow or demonstration Run. The editor SHALL organize step roles/profiles and installed execution capabilities, inputs, result schemas, deterministic checks, transitions, criteria, resources, WIP and hard budgets, permissions, optional Decision Signals, evaluators, delivery and enabled state into reviewable sections. It SHALL populate references from bounded authorization-filtered REST choices, require no opaque IDs, digests, credentials or provider payloads, and distinguish missing, incompatible, hidden and unconfigured choices without deriving authority in the browser.

The editor SHALL also support starting from a selected immutable version, keep unfinished drafts only in component memory, request authoritative normalized validation, and present exact selections, effective permissions, required Agent/backend capabilities, missing prerequisites and current-version preconditions before confirmation. Publication SHALL use the shared confirmed-intent mechanism and create immutable versions rather than mutate current versions. Private drafts, prompts, criteria, document bodies, credentials and mutation keys SHALL remain outside browser persistence, URLs, logs, analytics and generic errors. The UI SHALL distinguish a Factory pipeline's Flow and Configuration publications from an ordinary CI/CD Pipeline without introducing another execution resource.

#### Scenario: Operator creates a first Factory Configuration
- **WHEN** an authorized operator opens Factory in a new Project with no Flows or Configurations
- **THEN** Add pipeline offers an empty graph, installed technical choices, bounded schemas and per-step settings, validation and explicit review/publication without a triage setup action

#### Scenario: Operator replaces a Factory Configuration
- **WHEN** an operator changes a selected immutable version and confirms with its current-version precondition
- **THEN** Console publishes a new version, preserves previous versions for existing Runs/history and refreshes only affected queries

#### Scenario: Selected capability is incompatible
- **WHEN** authoritative validation reports an unavailable step, permission, evaluator, delivery or enforcement capability
- **THEN** the editor identifies the safe problem in its section, prevents invalid publication and preserves unrelated draft values

#### Scenario: Operator configures a Decision Signal profile
- **WHEN** an operator enables routing or tool-risk signals
- **THEN** the editor offers compatible server-published exact choices, purpose-specific rollout mode, thresholds, fallback and qualification metadata and explains the signal's non-authoritative scope

#### Scenario: Configuration changes during editing
- **WHEN** another operator replaces the current version before confirmation
- **THEN** Console preserves the in-memory draft and requires explicit reconciliation and a new confirmed intent instead of overwriting or replaying against a new baseline

#### Scenario: Operator abandons configuration authoring
- **WHEN** the operator leaves or reloads the editor before publication
- **THEN** unfinished values are discarded without a configuration version, browser-persisted draft, idempotency outcome or audit mutation

## ADDED Requirements

### Requirement: Pipeline policy sections explain effective limits and their origins
Pipeline-wide settings SHALL be organized into Start and queue, Execution limits, and Pipeline access rather than an unexplained combined root/configuration form. Start and queue SHALL describe admission and the distinct concurrency scopes of active Runs, configuration-wide active nodes and nodes within one Run. Execution limits SHALL present whole-Run budgets with readable units and explain how separately edited step/retry budgets fit within them. Pipeline access SHALL show requested access and its overall ceiling without implying automatic step attachments.

For each relevant setting, Console SHALL show the authoritative effective bound/access scope, the limiting source and its scope, including applicable Project, Configuration, Flow, step/Profile and runtime constraints. Unresolved or unenforceable settings SHALL remain explicit rather than shown as confirmed effective values. Equivalent operator intent SHALL NOT require duplicate unexplained fields. Inherited ceilings SHALL be identified as such; separately editable controls SHALL have distinct explained scopes. Technical references SHALL remain under Details while human-readable limits and blockers stay visible. The display SHALL preserve cumulative budgeting, current authorization and existing policy/enforcement semantics.

#### Scenario: Requested budget exceeds a policy ceiling
- **WHEN** an operator requests 100000 tokens for a Run and authoritative validation identifies an applicable 50000-token upper bound
- **THEN** Execution limits shows the requested amount, the applicable maximum and its owning policy, preserves the draft and blocks publication until explicit correction without silently changing either value

#### Scenario: Operator reviews concurrent execution settings
- **WHEN** a configuration permits multiple active Runs with narrower per-Run node concurrency
- **THEN** Start and queue explains each distinct bound and its source rather than presenting indistinguishable parallelism fields or treating them as the same limit

#### Scenario: Pipeline access exceeds a step's permissions
- **WHEN** pipeline policy permits a capability that a selected step/Profile does not permit or attach
- **THEN** Pipeline access and the step's review show the narrowed effective scope and its source without granting that capability from the pipeline ceiling alone

### Requirement: Step task editing is separate from reusable agent settings
Each agent step inspector SHALL provide an editable Step task section, separate from Executor, Documents and Tools and access. It SHALL let the operator describe what this invocation must do and review its expected result contract without editing the selected reusable agent. The editor SHALL distinguish that task from agent capabilities/general instructions, pipeline goals and incoming Issue content. Renaming a step or selecting a Role/Profile SHALL NOT supply a hidden business assignment. Changing the task SHALL preserve executor/document selections subject to authoritative compatibility checks and publish a new immutable Flow version rather than mutate the Role/Profile or admitted Runs.

#### Scenario: Operator assigns fit and duplicate assessment to a Project agent
- **WHEN** the operator selects a reusable Project agent and writes the fit/duplicate assignment in Step task
- **THEN** review shows the assignment as step-owned alongside its expected result, separately from the profile summary, and publishing requires no edit to the agent

### Requirement: Step executor selection is separate from documents
Each agent step inspector SHALL provide a distinct Executor section with clearly labeled Role and Profile controls using existing authorized Role/Profile contracts. It SHALL show current exact selections/versions and distinguish browsing a role's profiles from assigning a profile. Documents SHALL remain in a separate input section. Step names SHALL NOT imply a selected executor. Browsing or selecting an executor SHALL NOT start execution, attach documents or grant permissions. Existing configured profile-selection modes and authoritative compatibility checks SHALL remain applicable.

#### Scenario: Operator chooses an executor for a step
- **WHEN** the operator opens Executor, browses an authorized role and explicitly selects one of its profiles
- **THEN** the draft shows that step's selected role/profile versions separately from its Documents list without starting a Job or modifying sibling steps

#### Scenario: Operator changes the selected role
- **WHEN** the operator changes a role after selecting a profile and documents
- **THEN** the editor requires reconciliation of a profile belonging to the old role, preserves the document selections and reports any incompatible or missing Profile context through authoritative validation without silently changing inputs

#### Scenario: Step title matches an existing role
- **WHEN** a new step is named after an available role but no profile has been explicitly selected
- **THEN** Executor visibly remains unconfigured and publication reports the missing selection rather than inferring it from the title

### Requirement: Executor summarizes the selected profile and separates tool access
Executor SHALL show a readable summary of the exact selected profile's runtime/model settings, instructions and skills where available through authorized Role/Profile contracts, with a View profile link to that exact version. Missing metadata SHALL remain explicitly unavailable rather than inferred. Opening the profile SHALL preserve the in-memory pipeline draft and SHALL NOT silently select a newer version or mutate the profile.

MCP connections SHALL appear separately in the step's Tools and access section, showing understandable connection/resource names, configured scope and actionable missing/incompatible states. Technical identifiers, digests, configuration references and logical credential/secret references SHALL be placed under an accessible Details disclosure. Secret values SHALL never be displayed or resolved for this view. Configured access SHALL remain distinguishable from verified access; metadata SHALL NOT imply a successful authenticated call. Viewing these sections SHALL NOT invoke a model/MCP peer or widen the selected immutable bindings and effective permissions. These sections and profile navigation SHALL support English/Russian, keyboard use and narrow layouts under existing private-draft protections.

#### Scenario: Operator inspects the selected profile
- **WHEN** an operator selects a profile and opens View profile from Executor
- **THEN** the summary and destination identify the same exact version, the pipeline draft is preserved, and no sibling input, selected version or execution state changes

#### Scenario: Operator reviews configured MCP access
- **WHEN** a selected profile declares an MCP connection and only installed configuration metadata is available
- **THEN** Tools and access shows its readable scope as configured without claiming a successful connection, technical references remain under Details, and no secret read or peer invocation occurs

#### Scenario: Operator expands technical details
- **WHEN** an authorized operator expands Details by keyboard
- **THEN** the permitted logical references and exact identifiers are shown without secret values, and collapsing the disclosure leaves the readable scope and any blocker visible

### Requirement: Project settings own shared document editing
Project settings SHALL provide named document creation, exact-version viewing, immutable edits and access to retained versions through existing Project document contracts. Pipeline inspectors SHALL contain only exact step document selections and read-only previews, plus a clearly labeled navigation action to Project documents that preserves the current in-memory pipeline draft. The current-document picker SHALL explain that it selects content to open, not content to attach; creation and attachment SHALL be distinct actions. Generic None labels SHALL not conceal which selection is absent. Empty, unavailable, expired and forbidden states SHALL remain distinguishable.

Each step SHALL expose a single Documents list supporting bounded multiple selection from authorized Project documents, exact-version preview and removal. Separate Goals, Rules and Additional documents slots SHALL NOT be required. Each selected entry SHALL show an explicit agent-instructions or reference-material role without requiring the operator to enter a technical binding alias. The same Project document SHALL be selectable independently for different steps; editing or removing one step's selection SHALL not change another step or an admitted Run. An empty list SHALL be supported unless an explicit selected Profile/input contract requires a document; such a requirement SHALL have an actionable explanation.

#### Scenario: Operator opens a document to inspect it
- **WHEN** an operator chooses a saved Project document in Project settings and opens it
- **THEN** its exact content is shown for viewing/editing without adding it to a pipeline or changing any step selection

#### Scenario: Required document does not exist yet
- **WHEN** an unfinished step needs a Project document absent from the current catalog
- **THEN** the operator can open Project documents, publish it and return to the preserved draft, then explicitly select its version for only the intended step

#### Scenario: Operator selects several documents for one step
- **WHEN** the operator selects three Project documents in one step's Documents list, marks their instruction/reference roles, previews an exact version and removes one entry
- **THEN** review retains only the two selected exact references and their roles for that step without requiring Goals/Rules fields or manual aliases, and sibling selections remain unchanged

### Requirement: Document-guided classification and Reporter editing remain separate
The operator SHALL be able to attach Project documents defining components and priorities to a step without re-entering those values as constructor lists or result enums. The result editor SHALL support bounded structural/type fields and explain that structural validation does not certify semantic compliance with document prose. The task and document selections SHALL remain separate from Reporter action settings.

Reporter action inspectors SHALL separately select accepted-result conditions, the authorized target/Reporter, permitted label additions/removals and preserve/change-state choices with explicit unmatched-case handling. No second exhaustive component/priority list SHALL be required to configure a finite set of literal action conditions. Preview/review SHALL show the intended effects; document selection or agent task editing SHALL not silently create reporting actions.

#### Scenario: Operator configures a document-guided analyst and a label-only action
- **WHEN** the operator attaches component/priority documents, defines the analyst's bounded result fields and separately connects an action adding an allowed label while preserving state
- **THEN** the editor requires no duplicate classification lists, review distinguishes guidance from write effects, and execution uses only the separately authorized action

### Requirement: Completion criteria have an explicit editor and recipient selection
The common pipeline editor SHALL show editable pipeline goals and process-completion criteria with explicit create/edit/review actions and state their scope. These SHALL remain distinct from Project goals, an individual step's result schema and an Issue author's implementation acceptance. The operator SHALL explicitly select recipients and review the exact frozen content before publication. Editing pipeline goals/criteria SHALL not silently edit a shared Project document or existing Run. Long technical detail SHALL use optional disclosure; authentication guidance SHALL appear with connection/readiness settings rather than criteria.

#### Scenario: Operator edits the displayed criteria
- **WHEN** the operator opens Edit criteria and changes the process goal
- **THEN** the current draft shows the edited content and exact recipient steps, and publication creates a reviewed new version while existing Runs retain their original inputs

### Requirement: Each step exposes its actual resource and handoff scope
The editor SHALL declare available pipeline code sources from authorized Project repositories with revision-selection rules and access/scope ceilings. Step inspectors SHALL independently attach no code or one declared source with its exact repository/revision/access scope, named document versions, authorized tool/MCP bindings, goals/criteria inputs and accepted predecessor fields. The UI SHALL distinguish pipeline availability from actual step attachment, identify affected steps before source removal and require explicit reconciliation of dangling selections. Connecting a control edge SHALL grant no context. A pipeline source declaration or shared Work subject SHALL not silently select code for every step. Resource modes unsupported by installation or policy SHALL have actionable blockers. These controls SHALL be usable in English/Russian, narrow layouts and by keyboard without raw JSON or opaque IDs.

#### Scenario: Operator declares pipeline sources before attaching them
- **WHEN** the operator adds sources A and B to the pipeline and attaches A only to the second step
- **THEN** the first step remains without code, review shows B as available but unused, and no source is inherited from pipeline availability

#### Scenario: First agent has documents and the next agent has code
- **WHEN** the operator leaves code absent for the first step and selects read-only repository A for the second
- **THEN** review clearly shows those different scopes and only the explicitly projected accepted predecessor fields are attached to the second step

#### Scenario: Operator selects repository B for another step
- **WHEN** the operator selects a different authorized repository and revision in that step's inspector
- **THEN** review and diagnostics identify that exact selection without changing sibling resources or granting access from the task's source repository

### Requirement: Manual construction does not require a triage product concept
The common Factory entry point and step catalog SHALL not require the operator to choose triage, instantiate a business template or create a demonstration pipeline. The two-agent triage example SHALL be constructible using ordinary custom names, instructions, schemas, gates and projections. Existing saved definitions SHALL remain inspectable/editable through common controls after the dedicated triage wizard is removed. No separate business-specific editor, publication journal or state owner SHALL replace it.

#### Scenario: Operator builds an arbitrary workflow first
- **WHEN** the operator adds, renames, reorders and connects ordinary supported steps in an empty Project
- **THEN** the same constructor can publish that workflow or the assessment/classification example without a special triage entry point or hidden seeded Flow

### Requirement: Steps and transitions are authored as a visual Flow graph
The common editor SHALL provide a visual graph for adding, selecting, configuring, connecting and removing step nodes and declared outcome/conditional transitions. Node inspectors SHALL expose each step's inputs, resources, result and routing settings. Keyboard-accessible controls SHALL provide equivalent graph editing without pointer-only operations. Graph edits SHALL use the same ordinary Flow contract and authoritative validation as headless authoring. Invalid connections and dangling routes SHALL have actionable errors; rearranging visual positions SHALL not silently change execution order or handoffs. This requirement concerns graph authoring, not nested sub-Flows.

#### Scenario: Operator builds and revises the graph
- **WHEN** an operator adds reasoning and gate nodes, connects accepted/review routes, edits a condition and removes a referenced node using keyboard or pointer controls
- **THEN** the graph and inspectors reflect the same draft, invalid dangling routes block publication, and explicit correction publishes the reviewed ordinary Flow without inferred context transfer
