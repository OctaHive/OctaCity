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
The Project stage SHALL receive the frozen source ticket and required reviewed Project goals/context, and SHALL be able to investigate duplicates through its selected read-only MCP connections. The model SHALL be able to refine queries and read bounded candidates during the invocation rather than receive only a fixed server-selected candidate list. Consumed evidence SHALL identify actual candidate identities, observations and coverage. The analyst SHALL receive only its declared context and explicitly projected accepted handoff. Missing required context, unavailable lookup or unknown required coverage SHALL NOT be represented as an empty successful search. Hidden candidates, private reasoning and undeclared cross-Project data SHALL NOT be disclosed. Provider/ticket/tool text SHALL NOT change instructions, authority or destinations.

#### Scenario: Required Project goals are missing
- **WHEN** a ticket is selected but required Project goals cannot be resolved under the reviewed configuration
- **THEN** the configured readiness or stage policy reports the missing context or routes to review without recording a fabricated fit assessment

#### Scenario: Duplicate lookup is incomplete
- **WHEN** an authorized lookup returns a bounded page with more candidates remaining
- **THEN** the retained assessment names the consumed scope and coverage and cannot claim that no duplicate exists globally

#### Scenario: Permission changes during lookup
- **WHEN** OctaCity candidate visibility is revoked while an MCP read is outstanding
- **THEN** the service rejects or filters the response under current authority before delivery, with no fallback to another Project or credential

### Requirement: OctaCity exposes scoped read-only Issue tools through MCP
The system SHALL expose MCP task search and exact Issue reads over existing Issue authorization and storage contracts. Each authenticated connection SHALL be bound to an authorized principal and explicit Project/resource/read scope; an invalid, expired or revoked credential SHALL NOT be treated as an unrestricted management principal. Search and detail reads SHALL enforce bounded query/result sizes, pagination, cancellation and current visibility, including authorization recheck after asynchronous reads. Results SHALL identify the observed Issue version or content digest/time and distinguish a successful empty result from unavailable, incomplete, forbidden or failed lookup. This investigation surface SHALL offer no mutation tools and SHALL create no Issue, import, execution or reporting effect.

#### Scenario: An agent searches the allowed Project
- **WHEN** an authorized read-only MCP connection searches and then reads a candidate in its allowed Project
- **THEN** the responses contain only visible bounded task data and exact observed identity/version or content provenance, without creating or modifying any task or execution

#### Scenario: A token or requested resource is outside authority
- **WHEN** a call uses an invalid, expired or revoked token, requests a foreign Project/Issue or attempts a mutation tool
- **THEN** the call fails safely without disclosing hidden entity existence, widening permissions or substituting another credential

### Requirement: Agent AI plugins consume configured MCP servers without provider wrappers
An ordinary Role/Profile execution package SHALL select exact reviewed MCP connection settings for the qualified AI executor on the Agent. The selected AI plugin SHALL materialize those settings in the executor's supported native MCP configuration; the executor's MCP client SHALL call the connected servers' native tools. Compatible ready-made external MCP servers SHALL be usable through configuration and qualification without an OctaCity search wrapper, common provider query schema or triage provider dispatch branch. Adding a different AI executor SHALL require its own qualified plugin support. Unselected or ambient workstation MCP connections SHALL NOT be inherited, and unsupported required transport/authentication/tool capabilities SHALL block executable readiness rather than select a fallback server.

Each authenticated MCP connection SHALL have a separately authorized credential/token reference, purpose and resource/access scope, independent of AI-provider authentication and server-side Source/Reporter authority. Secret values SHALL be resolved through existing credential owners only for the authorized execution and SHALL NOT appear in profile content, prompts, Context, public results or logs. Read-only and resource restrictions SHALL be enforced by actual server/tool and credential permissions rather than model instructions or annotations alone. Current local revocation SHALL block newly unauthorized dispatch; external revocation SHALL obey the selected server's enforcement contract without an implied atomic cross-provider snapshot.

#### Scenario: A ready-made external MCP server is configured
- **WHEN** an operator selects a compatible external server with approved read-only tools, resource scope and its own credential reference for a qualified AI plugin
- **THEN** the Agent-side executor can use its native tools without an OctaCity provider adapter or access to the AI-provider or Reporter credential through that connection

#### Scenario: The selected AI executor cannot enforce the connection
- **WHEN** its plugin/client lacks required transport, authentication, scope or result-capture support
- **THEN** readiness reports the unsupported dependency and execution does not borrow another connection, credential or ambient client configuration

### Requirement: MCP investigation retains verifiable bounded evidence
Actual MCP calls and consumed results SHALL retain bounded ordinary execution/Artifact provenance identifying the invocation, connection/tool, query or arguments, observed scope/time and result content identity. Evidence SHALL retain the native candidate identity and revision when available; a missing provider revision SHALL use observed content digest/time without inventing an immutable version. Pagination, truncation, unknown coverage and failed, cancelled or unavailable calls SHALL remain distinguishable from successful empty results. The stage conclusion SHALL reference retained observed evidence; model-asserted tool calls or candidate references alone SHALL NOT establish a duplicate or global absence. Secret values SHALL remain absent from retained evidence. Missing required capture or unverifiable identity/coverage SHALL lead to the declared blocked, needs-input or review outcome. Capture SHALL reuse ordinary execution/result/Context retention rather than create a duplicate-search journal or require a custom parser for every external MCP server.

#### Scenario: The model refines its search
- **WHEN** the model makes several permitted MCP searches and reads a candidate before proposing a duplicate
- **THEN** the conclusion references the actual retained observations and identifies the consumed scope and known coverage independently of the model's description of its actions

#### Scenario: A tool result cannot support the claimed conclusion
- **WHEN** an external response is truncated, has unknown required coverage or cannot substantiate the candidate reference
- **THEN** the stage preserves that limitation and cannot authorize duplicate rejection or closure using fabricated certainty

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

Source intake and Reporter delivery SHALL remain server-owned connector operations with their existing authorization, cursor, recovery and ordered-effect contracts. Agent MCP investigation SHALL NOT replace those operations or inherit their write authority. Successful MCP reads SHALL NOT establish authority for an external mutation.

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
