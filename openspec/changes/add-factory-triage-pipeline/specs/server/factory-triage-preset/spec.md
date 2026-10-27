## Purpose

Defines a supported, configurable two-agent ticket triage preset that reuses ordinary Factory execution, explicit context and guarded reporting to turn raw GitHub Issues into retained assessments and reviewed outcomes.

## ADDED Requirements

### Requirement: Triage is a versioned preset of ordinary Factory primitives
The system SHALL provide a discoverable versioned triage preset resolving to ordinary immutable Flow and Factory Configuration definitions. It SHALL contain two separately bound reasoning stages with independently selected Project-owned Roles and exact Profiles: Project assessment followed, when eligibility policy permits, by Systems analyst assessment. Deterministic gates, human review, evidence commands and reporting SHALL use existing generic node contracts. Names SHALL NOT dispatch behavior. Replacing the preset or a Role/Profile SHALL NOT change a published definition or admitted Run.

#### Scenario: Operator configures the two stages
- **WHEN** an operator resolves triage with two compatible visible profiles and reviewed policies
- **THEN** the resulting drafts preserve separate stage identities, permissions, budgets, instructions, schemas and explicit handoff without creating an executable Run

#### Scenario: Upstream execution is missing
- **WHEN** profiles are visible for authoring but their required executable, Agent policy or enforced capability is unavailable
- **THEN** setup identifies the missing prerequisite and cannot represent that configuration as executable or silently choose another backend

### Requirement: Triage context is bounded and provenance-bearing
The Project stage SHALL receive the frozen source ticket, required reviewed Project goals/context and authorized bounded duplicate candidates with exact identities, observations and coverage. The analyst SHALL receive only its declared context and explicitly projected accepted handoff. Missing required context or unavailable lookup SHALL NOT be represented as an empty successful search. Hidden candidates, private reasoning and undeclared cross-Project data SHALL NOT be disclosed. Provider/ticket/tool text SHALL NOT change instructions, authority or destinations.

#### Scenario: Required Project goals are missing
- **WHEN** a ticket is selected but required Project goals cannot be resolved under the reviewed configuration
- **THEN** the configured readiness or stage policy reports the missing context or routes to review without recording a fabricated fit assessment

#### Scenario: Duplicate lookup is incomplete
- **WHEN** an authorized lookup returns a bounded page with more candidates remaining
- **THEN** the retained assessment names the consumed scope and coverage and cannot claim that no duplicate exists globally

#### Scenario: Permission changes during lookup
- **WHEN** source or candidate visibility is revoked while a context read is outstanding
- **THEN** the result is rejected or filtered under the current authority before it reaches model context, with no fallback to another Project or credential

### Requirement: Eligibility decisions follow reviewed finite policy
Project assessment SHALL produce schema-validated fit, duplicate-candidate and uncertainty observations with bounded reasons and evidence references. Accepted duplicate candidates SHALL resolve to authorized observed entities. Only reviewed deterministic policy SHALL choose declared eligible, duplicate, out-of-scope, needs-input or review outcomes. The default SHALL route unresolved judgments to review. A rejected or review-pending first stage SHALL NOT dispatch classification merely because the model recommends continuing.

#### Scenario: A duplicate is accepted under policy
- **WHEN** an observed exact duplicate candidate and Project assessment satisfy the configured duplicate acceptance rule
- **THEN** the retained duplicate decision selects its declared rejection/reporting path and no analyst invocation occurs

#### Scenario: A model invents a duplicate identity
- **WHEN** Project output references a candidate not present in its authorized observed context
- **THEN** the result cannot establish an accepted duplicate or cause a native close operation

### Requirement: Classification retains assessments separately from verified facts
The analyst result SHALL retain bounded priority assessment, kind, component, severity, size, risk, dependencies, preliminary reproducibility and a route from the declared finite domain. Configured gates SHALL reject invalid values, substituted references and incompatible combinations before accepting a result or effects. This result SHALL NOT rewrite immutable admitted scheduling priority or implicitly update authoritative Issue planning fields.

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

### Requirement: Triage reporting uses reviewed effects and independent status authority
Only accepted configured actions SHALL request mapped existing GitHub label additions/removals or open/closed state changes for the original admitted native ticket. Model strings, import, preview, configuration publication and Run completion SHALL NOT independently authorize writes. Labels, effect combinations and outcome coverage SHALL be bounded and reviewed. Unrelated labels and fields SHALL be preserved. Missing labels or write scope SHALL be actionable setup failures, not permission to create labels or broaden an installation.

Optional canonical Issue import SHALL remain independent. Local business status changes SHALL use the exact associated Issue/workflow and guarded transition boundary. Overlapping Issue-push and Work-backed external producers SHALL be rejected; a later association or mapping SHALL NOT retarget accepted effects. Accepted decision, local status, execution and delivery health SHALL remain distinct.

#### Scenario: A classified ticket is reported
- **WHEN** an accepted classification selects its configured label/state action under current update authority
- **THEN** the ordered Reporter applies only that original reviewed effect and retains independently inspectable delivery receipts

#### Scenario: Delivery is uncertain after restart
- **WHEN** a provider response is lost after a triage effect and the server restarts
- **THEN** recovery observes the original effect before an uncertain retry without creating another Run, renewing the deadline or changing the accepted decision

#### Scenario: A linked task already owns overlapping outgoing fields
- **WHEN** triage setup or action review selects Work reporting that conflicts with the associated Issue's push producer
- **THEN** existing producer-conflict policy blocks the overlap and requires an explicit consistent configuration

### Requirement: Triage finishes with a retained decision rather than implicit implementation
The preset SHALL finish or wait at declared triage outcomes with retained public results and exact Work/Run/optional Issue correlation. It SHALL NOT start a development, bugfix, PR or delivery pipeline implicitly. Other independently enabled source inputs SHALL retain their own stable intake identities and ordinary admission policy. Partial setup, source echoes, retries and ordinary input edits SHALL NOT create a new triage intake cycle.

#### Scenario: Triage recommends development
- **WHEN** the accepted result recommends a declared development route
- **THEN** the triage result and configured source effects are available while execution of another pipeline requires its own enabled input or explicit reviewed command

#### Scenario: The same ticket is observed again
- **WHEN** polling or restart revisits a native ticket already admitted through the same triage input identity
- **THEN** the existing Work/Run is retained without another pair of AI invocations or another logical reporting operation
