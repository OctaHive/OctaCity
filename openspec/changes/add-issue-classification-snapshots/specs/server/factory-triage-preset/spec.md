## MODIFIED Requirements

### Requirement: Classification retains assessments separately from verified facts
The analyst result SHALL retain bounded priority assessment, kind, component, severity, size, risk, dependencies, preliminary reproducibility and a route under the selected Project field definitions and their stable semantic bindings. Step result validation SHALL reuse those types and bounds without requiring a second operator-maintained vocabulary in the Flow. Configured gates SHALL reject invalid values, substituted references and incompatible combinations before accepting a result or effects. This result SHALL NOT rewrite immutable admitted scheduling priority or implicitly update authoritative Issue planning fields. An explicit configured trusted Issue action SHALL be able to apply the selected accepted classification through the Issue owner, retaining field provenance, requirements applicability and evidence references atomically. Applying or displaying a classification SHALL NOT activate delivery or imply implementation acceptance.

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
