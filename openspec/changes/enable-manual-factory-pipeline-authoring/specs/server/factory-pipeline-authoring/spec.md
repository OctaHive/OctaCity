## Purpose

Defines construction of a first executable Factory pipeline from installed technical capabilities and explicit operator choices, without business presets or previously authored Flows.

## ADDED Requirements

### Requirement: First-step authoring is independent of existing Flows
An authorized operator SHALL be able to construct a complete first Factory Flow and Configuration in a new Project using installed technical capabilities and Project-owned definitions. Reasoning, command, deterministic-gate, human-gate and trusted-action bindings SHALL be authorable without copying a node from an existing Flow. Installed executables, policy adapters and enforcement capabilities SHALL remain explicit prerequisites. Missing support SHALL have a safe actionable reason rather than an empty successful choice or a dependency on creating a business preset first.

#### Scenario: Project has no Flows or business presets
- **WHEN** an operator creates its first reasoning step with an authorized Role/Profile and installed compatible execution capability
- **THEN** authoritative validation resolves its exact execution/result binding without an earlier Flow, Build, demonstration Run or triage installation

#### Scenario: Installed capability cannot execute
- **WHEN** a required executable, result verifier or enforceable permission is unavailable
- **THEN** authoring preserves the draft and identifies that prerequisite without inventing an executor or borrowing broader authority

### Requirement: Business behavior is configured through ordinary contracts
Operators SHALL be able to configure bounded step input/result schemas, deterministic predicates, finite outcomes, transitions, failure handling, permissions, budgets and accepted-result projections through common authoring contracts. Node names, role names and business labels SHALL NOT select hidden parsers, mandatory fields, source authority or routing. Independently accepted tool/evidence provenance SHALL remain necessary where a configured decision requires verified facts; a schema-valid model assertion SHALL NOT become such a fact.

#### Scenario: Operator assembles assessment and classification manually
- **WHEN** an operator configures two separate reasoning steps, their result schemas and gates, and a projection of an accepted first-step result
- **THEN** the resulting ordinary definition executes both steps without a triage resolver, fixed stage names or a built-in triage template

#### Scenario: Operator adds an unrelated third step
- **WHEN** an operator adds a documentation-check step with its own bounded schema and declared routes using installed primitives
- **THEN** it validates and executes through the same contracts without a business-specific Rust result type or runtime registration

#### Scenario: Model invents verified evidence
- **WHEN** model output claims a duplicate, reproduction or other verified fact without the required independently retained provenance
- **THEN** the common configured verifier/gate rejects or routes it to declared review without accepting the assertion or dispatching an unconfigured effect

### Requirement: Visual and headless authoring share one Flow contract
The visual constructor SHALL represent steps as nodes and declared transitions as edges of the ordinary Flow Definition. Adding, configuring and connecting nodes, including conditional outcomes, SHALL produce the same normalized definition and validation results as headless authoring. Visual position or drawing order SHALL grant no execution order, context or authority beyond explicit Flow contracts. Nested sub-Flow execution SHALL NOT be required by this change.

#### Scenario: Operator connects steps visually
- **WHEN** an operator adds two reasoning nodes and a gate and connects their accepted and review outcomes in the visual constructor
- **THEN** publication retains those explicit nodes and routes in the ordinary Flow, and headless validation of the same definition yields the same execution and handoff contracts

### Requirement: Classification guidance comes from selected Project documents
An operator SHALL be able to define components and priority scales/meanings in Project documents and attach their exact versions to an agent step through ordinary document selection. Common authoring SHALL NOT require a separate vocabulary registry or duplicate lists/enums of those values in the step/result editor. A bounded structural/type result contract SHALL suffice for document-guided assessment; optional generic enum facilities MAY remain. Structural validity SHALL NOT be presented as proof of semantic agreement with a prose document. Stronger acceptance checks or human review SHALL remain explicit, and missing/ambiguous guidance SHALL follow declared uncertainty handling without invented values or inherited private context.

#### Scenario: Analyst classifies from Project documents
- **WHEN** an operator attaches component and priority documents, writes the classification task and defines bounded result fields without duplicating those vocabularies
- **THEN** ordinary validation/publication and isolated execution support that task, retain the exact selected guidance and result, and require no matching business enums or specialized agent profile

### Requirement: Reporter effects are configured independently of classification guidance
Reporter effects SHALL be separately authored ordinary actions conditioned on accepted results, with explicit targets, installed supported effects and current authorization. Label addition/removal and state preservation/change SHALL be independent choices. Document-guided scalar results SHALL be usable with bounded literal predicates and explicit no-effect/review fallbacks without requiring a copied exhaustive classification enum. Unmatched values SHALL follow the declared fallback rather than generate provider literals. Agent/document text SHALL grant no write authority, and delivery status SHALL remain separate from accepted assessment.

#### Scenario: Add a tag while preserving ticket state
- **WHEN** a configured action matches an accepted assessment and requests only an allowed label addition
- **THEN** Reporter requests that label change while preserving state and unrelated labels, without deriving a close command from the model response

#### Scenario: Accepted component has no configured reporting case
- **WHEN** a structurally valid document-guided component value matches none of the explicit Reporter conditions
- **THEN** the declared no-effect/review fallback applies without creating a label, choosing another target or closing the ticket

### Requirement: Reusable agents are independent of step assignments
Role/Profile SHALL describe reusable capabilities, general working instructions, skills and execution settings. Each agent step SHALL separately own editable task instructions and expected result/check contracts in the versioned Flow. The new manual authoring path SHALL NOT require a Role/Profile or installed runtime that embeds that step's business assignment. Selecting a role or skill SHALL NOT implicitly assign Project-fit, duplicate assessment or another business operation. The same exact Role/Profile SHALL be usable for different explicit step tasks without cloning or editing the agent. Existing immutable Role/Profile versions SHALL retain their historical meaning.

#### Scenario: One Project agent performs different tasks
- **WHEN** two steps select the same exact Project Role/Profile, one explicitly requests Project-fit/duplicate assessment and another requests requirements clarification
- **THEN** each isolated invocation receives its own reviewed assignment and selected context, with no duty inherited from the other step and no Role/Profile mutation

#### Scenario: Step assignment changes after admission
- **WHEN** the operator publishes a changed step task in a new Flow version
- **THEN** existing Runs and retries retain the original task bytes/digest, the selected Role/Profile and sibling assignments remain unchanged, and future admission can explicitly select the new Flow version

### Requirement: Authoring has no execution side effects
Discovery, draft assembly, normalization and validation SHALL NOT publish definitions, read credential values, invoke a model/MCP peer, start a Build, reserve an execution budget or perform provider effects. Confirmed publication SHALL use existing versioned resource owners and remain distinct from source preview, import, activation and execution. The same ordinary definitions SHALL be publishable and operable headlessly.

#### Scenario: Operator validates an unfinished pipeline
- **WHEN** compatible installed metadata exists but the operator has not confirmed publication
- **THEN** validation returns exact normalized choices and safe missing fields while creating no Flow, Configuration, Work, Run or external effect

### Requirement: Empty-system execution is the acceptance boundary
The supported manual journey SHALL be demonstrated from a Project with no Factory Flows, Configurations or business presets. Technical installation SHALL contain no pre-authored business steps. The operator SHALL author the two-agent assessment/classification example, criteria, documents, gates, resource selections, input and optional reporting through ordinary controls. Execution SHALL retain isolated invocations, server-owned states/results, exact input versions and only explicitly selected accepted handoffs.

#### Scenario: Complete manual journey survives restart
- **WHEN** the operator authors and executes the pipeline, the server restarts after the first accepted result and a Project document is subsequently edited
- **THEN** recovery retains the original Work/Run and frozen inputs, resumes only the declared successor and exposes neither the predecessor's private context nor a second logical execution or reporting effect
