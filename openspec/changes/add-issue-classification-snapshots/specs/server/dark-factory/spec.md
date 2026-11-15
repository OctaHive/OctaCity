## ADDED Requirements

### Requirement: Flows can consume Issue snapshots independently of producer executions
A versioned Flow/Configuration SHALL be able to declare an Issue snapshot input selecting bounded built-in fields, ordinary Project properties by stable ID, classification fields, Project definition bindings, explicit Issue comment/document materials, applicable evidence references and required input/readiness checks. Ordinary properties SHALL reuse their published supported scalar types/bounds without requiring a classification semantic slot. The contract SHALL describe what the consumer needs without naming the producer Flow/version, terminal, outcome schema or Run. Missing required field bindings, ambiguous semantic mappings and incompatible field types SHALL block publication or executable readiness with a field-specific reason. Generic intra-Flow accepted-result connections SHALL retain their existing behavior.

New consumers SHALL select fields and local aliases directly, with explicit required/optional, applicability, evidence and admission bindings. Fixed Issue risk/severity/reproduction roles SHALL NOT be publication prerequisites. Scheduling/risk conversion SHALL map selected values into the existing typed Work contract without assigning authority to Issue field names. Renaming a pipeline, step, output or field display label SHALL NOT select a different execution algorithm.

Manual Issue execution and separately enabled automatic intake SHALL use the same snapshot validation and existing Work/Run admission owner. Admission SHALL check current visibility, launch/access policies, field/evidence applicability, implementation acceptance and exact repository subject, and freeze the admitted snapshot and selected policy versions. Issue metadata SHALL NOT grant tools, credentials, Artifact access, a Factory Decision or permission to publish or merge code. Each step SHALL explicitly select its own subset of the frozen inputs through existing Context controls; selecting an Issue snapshot SHALL NOT expose every Issue attachment or prior Run.

Material consumption SHALL be explicitly declared as exact selections or a bounded approved Issue-material set, with required/optional handling and per-step projections. Document sources SHALL distinguish same-Issue-owned publications from explicitly linked same-Project publications and retain their exact owner/publication identities; snapshot selection SHALL neither transfer ownership nor expose Issue documents through Project catalogs. Cross-Issue attachments SHALL NOT be substituted even within one Project. Selections SHALL contain at most 32 entries and at most 1 MiB of combined UTF-8 text, subject to stricter consumer limits and existing individual source bounds. Source bodies SHALL be resolved under their independent content permissions/retention and frozen with exact provenance through managed input publication. No default whole-thread, all-Issue-documents, newest-comment or latest-document lookup SHALL substitute for selection. A required unavailable source or exceeded bound SHALL block admission; an optional omission SHALL be explicit in review and the retained snapshot. Comment/document prose SHALL NOT confer evidence, gate-response or controlling-policy authority.

#### Scenario: A different triage implementation prepares the Issue
- **WHEN** an Issue has current valid classification and authorized evidence satisfying the target's contract after preparation by another Flow or an authorized human
- **THEN** the same delivery Configuration can admit it without rebinding an upstream producer or requiring access to that producer's private Run data

#### Scenario: An Issue has a matching label but incomplete classification
- **WHEN** automatic selection matches an Issue whose required snapshot values or evidence are missing or stale
- **THEN** the consumer reports the failed prerequisite without admitting implementation merely because a label or status matches

#### Scenario: One stage cannot read private evidence
- **WHEN** implementation selects public requirements and classification while a verifier selects separately authorized protected material
- **THEN** their Context and enforced access remain distinct despite belonging to the same Issue

#### Scenario: A consumer needs an ordinary Project field
- **WHEN** its input selects a supported Project-specific environment or target-date property without classification semantics
- **THEN** publication and admission use the exact property ID/type/bounds and freeze only its selected value without demanding a triage Run or copied schema

#### Scenario: One step selects supplementary Issue text
- **WHEN** a reviewed snapshot includes selected comment/document materials and only one step declares their projections
- **THEN** that step receives their frozen permitted text while siblings receive only their own declared inputs and later source publications do not replace bytes

### Requirement: Issue snapshot intake preserves stable processing identities
Issue-snapshot automatic intake SHALL retain the existing stable input-scoped Work identity and recoverable preparation/admission receipts. Repeated events, classification/property/material edits, comment appends, new document publications, label edits, changes to input/Configuration versions and server restarts SHALL NOT create another automatic processing cycle after admission. An input waiting for valid Issue data SHALL be able to reconcile a newly applicable snapshot before admission without admitting twice. Independent items SHALL continue when one Issue is blocked. Automatic material consumption SHALL require an explicitly published bounded selection policy resolving approved Issue materials; merely enabling intake SHALL NOT include discussion, Issue-owned documents or Project documents. A deliberate new manual request SHALL remain a distinct reviewed execution, while infrastructure retry SHALL use its original snapshot and Run.

#### Scenario: Classification becomes available after an initial wait
- **WHEN** an enabled input revisits an unadmitted Issue after a valid classification has been committed
- **THEN** it can admit one snapshot through ordinary policy and retain that Work/Run on later observations and restart

#### Scenario: Classification changes after automatic admission
- **WHEN** a previously admitted Issue receives a new size or priority and its label is removed and re-added
- **THEN** the input retains its existing admission rather than creating another delivery attempt or mutating the original Work

#### Scenario: New comments arrive at an automatically processed Issue
- **WHEN** an input observes additional discussion or a new document publication after admission
- **THEN** it preserves the original selected bytes and Work/Run without ingesting those sources or manufacturing a new processing identity

### Requirement: Explicit producer-result inputs remain authorable and recoverable
The current Flow contract SHALL support explicit pipeline-to-pipeline accepted-result inputs when a consumer needs an exact producer result, with exact source Run/schema checks and recovery. Issue snapshots SHALL be another explicitly selected mode. Both modes SHALL preserve their admitted inputs and scoped receipts across ordinary edits/restart. Unsupported competing sources SHALL be rejected rather than assigned inferred precedence. Old development encodings, fixed-slot payloads and compatibility adapters SHALL NOT be required.

#### Scenario: An operator upgrades the delivery example
- **WHEN** it publishes a new Issue-snapshot Flow and selects it in a new Configuration version
- **THEN** new requests use its selected input contract while already admitted new-model executions retain their original snapshots and receipts

#### Scenario: A new pipeline intentionally consumes another pipeline's result
- **WHEN** an author explicitly publishes an accepted-result input with exact producing Flow/terminal/schema constraints and selects an eligible Run
- **THEN** its ordinary validation and recovery remain available without requiring the result to be copied into Issue fields or disabling other Issue-snapshot consumers

### Requirement: Business behavior is published configuration over common Flow primitives
Any Flow SHALL use the same ordinary inputs, accepted results, selected-field agent forms, trusted saves, snapshots, admission mappings, predicates/gates and observation checks. One field selection SHALL generate the agent contract and permitted save set without manual result-to-Issue mapping or a separate apply node. Triage and Delivery SHALL be template data rather than execution kinds inferred from names/roles. Existing finite-choice conditions SHALL freeze a reviewed field/order/threshold and concrete routes. Reuse current mechanisms; no new workflow/rules engine, mapping API or compatibility adapter is required.

#### Scenario: An unrelated pipeline uses the same field and condition mechanisms
- **WHEN** an author creates a differently named process using selected custom fields, an ordinary field-form agent step and a threshold on a non-Size choice field
- **THEN** ordinary publication and execution use its declared field selection, ordinary inputs/order/routes without a triage/Delivery mode or business-name lookup

#### Scenario: A published choice order changes later
- **WHEN** a Project choice catalog changes after a condition was published or a Run admitted
- **THEN** the retained concrete rule and snapshot remain unchanged until an explicit new publication/request selects the new contract

### Requirement: Evidence-dependent meaning is explicitly bound to trusted observations
Flows SHALL identify evidence-dependent claims through selected field/value bindings and existing supported trusted observation/verdict checks, including exact subject, criterion, environment and freshness. No familiar field name, scalar value or arbitrary boolean SHALL independently establish verified reproduction, approval or candidate validation. Proof-consuming paths SHALL require their declared independently qualified records; trusted publication and protected validation SHALL retain their non-forgeable contracts. A separate general-purpose policy interpreter or fixed-role fallback SHALL NOT be required.

#### Scenario: A different vocabulary represents an evidence-dependent claim
- **WHEN** a policy binds a custom field/value to a required trusted observation and that observation is missing, stale, contradictory or for another subject
- **THEN** the declared proof-consuming path is blocked despite a matching metadata value or a renamed alias

#### Scenario: Ordinary metadata resembles a verified claim
- **WHEN** an unrelated field contains reproduced or approved without an applicable trusted policy/record
- **THEN** it remains ordinary data and grants no verification, approval or publication authority

### Requirement: CI analysis and Decision Signals compose with field-filling steps
An agent SHALL be able to consume retained CI/CD results through existing declared inputs and fill its selected Issue form. Configured routes SHALL distinguish reported test failure from missing reports/infrastructure failure; no successful CI result or unrequested whole log access SHALL be fabricated. Existing provider-neutral Decision Signal nodes SHALL remain usable for configured finite-choice evaluation/routing before or after the filling step under their installed capabilities, rollout/fallback and authority checks. JEV/OpenAI provider wire details SHALL NOT enter the field-form/save contract. This change SHALL require no new provider adapter, arbitrary provider-result-to-Issue mapper or live-provider qualification.

#### Scenario: Failed tests are analyzed before updating an Issue
- **WHEN** the configured failed-test route supplies the retained report to an agent with selected Issue fields
- **THEN** the agent can complete that form and the common owner can save it without requiring CI to have passed or interpreting report keys as write destinations

#### Scenario: A Decision Signal selects the next route
- **WHEN** an installed qualified provider evaluates declared CI/report/context input and returns a consumed finite routing disposition
- **THEN** the ordinary configured route selects analysis, review or another declared outcome while scalar field-save authority remains governed by the selected form and common writer
