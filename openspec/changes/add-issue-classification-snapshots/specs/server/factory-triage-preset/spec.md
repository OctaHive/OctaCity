These requirements describe an optional triage template. Business fields, aliases and routes are template/Flow configuration, not mandatory roles or execution semantics for other pipelines. Superseded development formats require no compatibility path.

## MODIFIED Requirements

### Requirement: Classification retains assessments separately from verified facts
A triage template SHALL select the Issue fields its ordinary agent step fills. The server SHALL derive their result types/choices and save set from exact Project definitions without fixed semantic roles, manual alias mapping or a copied output schema. The model returns values in that generated form; the common trusted writer applies only the accepted result with atomic provenance/applicability. Existing gates SHALL reject invalid data/evidence before accepting effects. Assessment, applied fields, outcome display and downstream activation SHALL remain separate facts; no manual apply node is required for the standard field-filling path.

When an Issue snapshot supplies contextual properties or selected comment/document material, each triage step SHALL consume only its declared frozen projections. Textual guidance SHALL remain distinguishable from Project field definitions and independent evidence. Retained classification provenance SHALL identify explicitly declared material dependencies without importing every comment or implicitly treating text as verified reproduction or a human-gate response.

For defects, a reproduced or not-reproduced claim SHALL require independent applicable evidence bound to the exact subject, environment and ordinary command attempt. Missing tools, timeout, dependency/checkout failure, model prose or a generic nonzero exit SHALL NOT establish reproduction or a negative finding. Unknown, needs-input and needs-research outcomes SHALL remain available; features SHALL support not-applicable reproduction. Deep research SHALL be a separately configured route rather than an invented observation.

#### Scenario: A defect is classified with reproduction evidence
- **WHEN** the analyst consumes an authorized verified report for the exact code subject and its output satisfies the configured schema and gate
- **THEN** the accepted result retains the report, stage/profile and subject provenance together with its classified fields

#### Scenario: The reproduction environment fails
- **WHEN** a reproduction command cannot obtain its required dependencies or reaches its deadline
- **THEN** triage records an unresolved execution/reproduction condition and follows needs-input, research or review policy instead of asserting not-reproduced

#### Scenario: The model proposes an undeclared route
- **WHEN** analyst output names a route or component outside the reviewed vocabulary
- **THEN** validation rejects or safely routes that result to declared review and enqueues no guessed provider effect

#### Scenario: A classified Issue is prepared for another pipeline
- **WHEN** the configured triage action successfully applies its accepted assessment to the Issue
- **THEN** the Issue exposes that classification for independent execution review without requiring the consumer to select this triage Flow or Run

#### Scenario: Local application conflicts with an operator edit
- **WHEN** the accepted assessment is retained but the Issue rejects its stale or conflicting field updates
- **THEN** diagnostics distinguish triage completion from failed classification application and a consumer cannot treat the unapplied values as current Issue data

#### Scenario: Ordinary stand advertises the guarded local writer
- **WHEN** the ordinary stand installs its reviewed Factory catalog under its configured management principal
- **THEN** the Project advertises the local Issue classification action and the installer-owned Triage access profile includes its explicit plugin permission
- **AND** installation enables no classification effect or delivery intake, preserves custom reviewed policies and leaves previously published pipelines unchanged

## ADDED Requirements

### Requirement: Triage templates use universal field and policy configuration
Triage templates SHALL configure selected-field filling, ordinary inputs, supported evidence checks, conditions, agents and existing display settings over common Flow mechanisms. Any supported core/custom scalar field SHALL be selectable without a global role or shared-code vocabulary. Publication SHALL generate one typed form and automatic guarded save for the same set; operators SHALL not configure result paths, duplicate schemas or a separate apply node. Obsolete specialized formats SHALL be replaced, not wrapped in adapters.

#### Scenario: An operator changes the selected assessment fields
- **WHEN** an operator selects a custom boolean, date and integer in the triage agent field form
- **THEN** common authoring derives their Project types and the automatic trusted save records accepted values without modifying shared code or configuring mappings

#### Scenario: A claim requires independent evidence
- **WHEN** the triage template binds a field/value claim to a trusted observation policy
- **THEN** its configured proof checks retain exact subject/criterion/environment and unresolved routes without making that field name a global Issue semantic role
