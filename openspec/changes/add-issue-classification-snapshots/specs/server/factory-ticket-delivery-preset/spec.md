This delta is applied after the implemented `add-factory-ticket-delivery-pipeline` capability has been synchronized to main specs. That change's added requirements are this delta's baseline; its historical artifacts and published executions are not rewritten.

## RENAMED Requirements

- FROM: `### Requirement: Development consumes an accepted triage handoff`
- TO: `### Requirement: Development consumes a frozen Issue snapshot`
- FROM: `### Requirement: Triage size selects conditional OpenSpec authoring`
- TO: `### Requirement: Issue size selects conditional OpenSpec authoring`

## MODIFIED Requirements

### Requirement: Development consumes a frozen Issue snapshot

An explicit Issue execution request or separately enabled downstream intake SHALL obtain its implementation requirements, selected ordinary Project properties and classification from the canonical Issue and freeze them in a distinct Work and Run for the same Project and Issue. Its declared input contract SHALL also be able to consume explicitly selected same-Issue comments, exact Issue-owned document publications and separately linked exact Project document publications as bounded supplementary task context. Admission SHALL verify current authority, required field/definition compatibility, material availability, requirements applicability, supported dependency/evidence claims and the repository subject. It SHALL retain the exact Issue revision, classification provenance, property definitions, selected material bytes/source and owner/publication identities/digests, evidence identities and input-contract/policy versions. A producing triage Flow/version, terminal, outcome schema or Run SHALL NOT be a required consumer binding. A valid authorized manual assessment or a different triage implementation SHALL satisfy the same field contract without a fabricated predecessor.

Free-text conclusions, Issue cards, external labels and Reporter receipts SHALL NOT confer admission or verification authority. Missing, stale, incompatible, inaccessible or policy-ineligible Issue data SHALL block admission with a reason. Repeated automatic observations SHALL preserve the existing stable input-scoped identity; a classification edit SHALL NOT create another cycle after admission. Existing predecessor-based publications and executions SHALL retain their original contract until an explicit replacement is published.

#### Scenario: Accepted ticket enters delivery once
- **WHEN** an enabled downstream input repeatedly observes an Issue whose current classification satisfies its snapshot contract
- **THEN** one Work/Run freezes the reviewed Issue data without rerunning triage or requiring a producing Run selector

#### Scenario: An operator explicitly starts delivery
- **WHEN** an authorized operator reviews a compatible current Issue and its delivery target
- **THEN** admission freezes the selected requirements and classification without enabling automatic intake or requiring external Reporter delivery

#### Scenario: A human prepares the Issue
- **WHEN** valid manually attributed classification satisfies the target's current field and evidence requirements
- **THEN** delivery can start without any triage Run while configured human requirements acceptance and verification remain mandatory

#### Scenario: The ticket changed after triage
- **WHEN** the reviewed requirements or selected classification changes before final admission
- **THEN** the request reports a conflict or prerequisite failure and does not silently combine old requirements with new classification

#### Scenario: Label is added without an accepted result
- **WHEN** an Issue matches a delivery selection label but lacks required current classification, acceptance or evidence
- **THEN** no implementation is admitted and the missing prerequisite is visible

#### Scenario: Supplementary text changes after delivery admission
- **WHEN** a new comment or document publication appears after delivery froze its explicitly selected materials
- **THEN** delivery retains the original selected text and source identities without importing the new discussion, changing requirements mid-Run or creating another automatic cycle

### Requirement: Unresolved defects receive one deeper reproduction stage

When an explicit Reporter action is configured on the accepted triage outcome, the triage stage SHALL request the mapped existing label `need reproduce` through its configured reporting policy when classification accepts a defect with `not_reproduced`, `unknown` or `needs_research` preliminary reproducibility. This triage effect SHALL be independent of downstream delivery activation or admission. The delivery pipeline SHALL NOT add `need reproduce`. Defects requiring missing input SHALL remain paused until input is supplied. Delivery SHALL use the reproduction assessment and authorized applicable evidence frozen from the Issue snapshot to run one deeper reproduction stage before size routing for unresolved defects; reproduced defects and features SHALL bypass it. This stage SHALL retain the reproduction checks and observed outcome against the admitted subject. A completed `cannot_reproduce` outcome SHALL end delivery with the ticket open. When configured, an explicit Reporter action SHALL request adding `can't reproduce` and removing the managed `need reproduce` label. A reproduced outcome SHALL continue and request removal of the managed `need reproduce` label when configured. Reporter SHALL preserve unrelated labels. Pending, failed and confirmed external delivery SHALL be distinguishable from the accepted local outcome.

Timeouts, missing dependencies, unavailable environments and insufficient information SHALL pause or technically fail the affected stage without asserting `cannot_reproduce`. The pipeline SHALL NOT repeatedly re-admit the same unresolved ticket after termination.

#### Scenario: Triage marks an unresolved defect while delivery is disabled
- **WHEN** triage accepts an unresolved defect classification with an explicit Reporter action and the downstream delivery pipeline is disabled
- **THEN** triage requests the `need reproduce` label through its Reporter policy without admitting a delivery execution

#### Scenario: Defect is still not reproducible
- **WHEN** deeper reproduction checks complete without observing the reported symptom and the declared Reporter effects are configured
- **THEN** its retained outcome is `cannot_reproduce`, Reporter requests the configured label changes, the ticket stays open and no test authoring, implementation or PR publication starts

#### Scenario: Deeper reproduction succeeds
- **WHEN** the deeper reproduction stage observes the defect
- **THEN** delivery continues to size routing with its reproduction evidence and requests removal of the managed label if configured

#### Scenario: Reproduction environment is unavailable
- **WHEN** the reproduction stage cannot prepare its required environment
- **THEN** the failure remains unresolved, no `can't reproduce` effect is requested and implementation does not start

#### Scenario: Feature or already reproduced defect enters
- **WHEN** the admitted Issue snapshot identifies a feature or a defect with independently applicable reproduction evidence
- **THEN** delivery goes directly to size routing without another reproduction invocation

### Requirement: Issue size selects conditional OpenSpec authoring

The pipeline SHALL use the size frozen in its Issue snapshot and route `large` tickets through an isolated OpenSpec authoring invocation producing a coherent proposal, delta specifications, design and implementation tasks before test authoring. The retained requirements artifact set SHALL be the input to subsequent stages and part of the delivered candidate. `small` and `medium` tickets SHALL proceed to test authoring using an immutable Accepted Work Contract derived from the ticket's explicit expected behavior and frozen Issue classification, after any configured human requirements acceptance. Triage process-completion criteria SHALL NOT substitute for implementation acceptance. Missing expected behavior or unresolved material requirements SHALL pause the ticket rather than invent acceptance criteria. Human requirements acceptance SHALL follow the pinned component/size/priority policy for either branch. No additional independent AI specification-review invocation SHALL be required by this pipeline.

#### Scenario: Large feature or defect needs specifications
- **WHEN** size routing receives an accepted `large` ticket
- **THEN** OpenSpec artifacts are prepared and retained before tests or implementation use those exact requirements

#### Scenario: Non-large ticket has sufficient expected behavior
- **WHEN** size routing receives a `small` or `medium` ticket with explicit implementation acceptance
- **THEN** test authoring starts without an OpenSpec authoring invocation

#### Scenario: Triage succeeded without implementation acceptance
- **WHEN** the only acceptance document describes completion of triage and the ticket supplies no usable expected behavior
- **THEN** delivery pauses for requirements input and does not reuse that document to certify implementation

#### Scenario: Size changes after delivery starts
- **WHEN** the Issue receives a new size after its Work has been admitted
- **THEN** that execution retains its original requirements branch and a new deliberate request is needed to use the changed classification


### Requirement: Ticket delivery is authorable through the ordinary Factory editor

An authorized operator SHALL be able to assemble ticket delivery from an empty visual Flow graph, publish an exact Flow version and select it in an ordinary Factory Configuration with pinned Project launch and access policies. Ordinary editor choices SHALL support the declared steps, branches, accepted inputs, requirements gate, Reporter actions and Delivery target. A new versioned `ticket-delivery` example SHALL select the Issue snapshot contract rather than require an accepted triage predecessor. The editor SHALL expose the selected Project fields, applicability/evidence checks and frozen input preview without requiring a producer Flow, terminal or Run. Existing examples and their pinned contracts SHALL remain recoverable. The versioned `ticket-delivery` preset SHALL be an optional starting point through the existing resolver and SHALL produce the same editable, validated definition. Existing readiness and validation SHALL identify unsupported or missing bindings before executable readiness is claimed. Automatic intake and external effects SHALL require explicit configuration. The successful path SHALL need no per-stage confirmation beyond configured requirements acceptance, reviewer quorum, model-scored acceptance or repeated general readiness stage.

#### Scenario: An operator builds the flow from an empty graph

- **WHEN** an authorized operator assembles ticket delivery from **Add pipeline** and publishes the Flow and Configuration
- **THEN** the operator can start an eligible Issue manually using the same runtime and validation as the optional preset while automatic intake remains disabled

#### Scenario: A required execution binding is unavailable

- **WHEN** a step needs protected-test execution or trusted publication that the selected installation cannot supply
- **THEN** the common review identifies the blocked step and capability before the pipeline is considered executable

#### Scenario: Operator replaces the triage pipeline
- **WHEN** another authorized classification process prepares the same Issue field contract
- **THEN** the delivery Flow remains usable without changing its input binding to name the replacement producer


### Requirement: Human acceptance follows requirements policy

The pipeline SHALL support pinned requirements-acceptance rules combining component, size and priority from the admitted Issue snapshot. Matching rules SHALL request human acceptance of the exact prepared specification or Accepted Work Contract before test authoring; unmatched tickets SHALL continue automatically. Priority SHALL use the retained effective Issue priority under the pinned input mapping without a second agent assessment. Classification source attribution SHALL not by itself satisfy an independent human requirements-acceptance rule. A pending or rejected acceptance SHALL NOT permit downstream stages. A correction SHALL follow the declared requirements route and require acceptance of the new version when the rule applies.

Acceptance SHALL bind the authorized actor, exact requirements or contract digest, policy and current gate attempt. Changed requirements SHALL NOT reuse a previous approval for the changed version. Existing human-gate authorization, durable responses and recovery SHALL own these decisions; AI agents SHALL NOT approve themselves. Pending and decided gates SHALL be visible through Console and headless diagnostics. This requirements gate SHALL NOT grant merge authority.

#### Scenario: Ticket matches human requirements acceptance
- **WHEN** its retained component, size and priority match a configured rule and Analytic has prepared the specification
- **THEN** execution waits for human acceptance of that exact result before creating tests, and rejection does not start implementation

#### Scenario: Ticket does not match a requirements rule
- **WHEN** its frozen requirements are ready and no configured human-acceptance rule matches
- **THEN** the Tester authoring stage starts without a human acceptance request

#### Scenario: Requirements change after approval
- **WHEN** requirements change after human approval
- **THEN** the prior approval cannot authorize that changed version and the applicable gate requests a fresh decision
