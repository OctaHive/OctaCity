## ADDED Requirements

### Requirement: Flows can consume Issue snapshots independently of producer executions
A versioned Flow/Configuration SHALL be able to declare an Issue snapshot input selecting bounded built-in fields, ordinary Project properties by stable ID, classification fields, Project definition bindings, explicit Issue comment/document materials, applicable evidence references and required input/readiness checks. Ordinary properties SHALL reuse their published supported scalar types/bounds without requiring a classification semantic slot. The contract SHALL describe what the consumer needs without naming the producer Flow/version, terminal, outcome schema or Run. Missing required field bindings, ambiguous semantic mappings and incompatible field types SHALL block publication or executable readiness with a field-specific reason. Generic intra-Flow accepted-result connections SHALL retain their existing behavior.

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
Existing predecessor-bound Flows, Configurations, execution requests and Runs SHALL remain readable and executable under their pinned contract, including original source Run/schema checks and recovery. Explicit pipeline-to-pipeline accepted-result inputs SHALL also remain selectable and publishable for new Flows/Configurations when a consumer needs an exact producer result. Issue-snapshot inputs SHALL be an additional explicitly selected mode; an active or historical Run SHALL NOT be migrated in place. Unsupported combinations of competing sources for the same classification input SHALL be rejected before admission rather than inferring precedence.

#### Scenario: An operator upgrades the delivery example
- **WHEN** it publishes a new Issue-snapshot Flow and selects it in a new Configuration version
- **THEN** new requests can use that contract while previously published predecessor-based executions retain their exact original inputs and diagnostics

#### Scenario: A new pipeline intentionally consumes another pipeline's result
- **WHEN** an author explicitly publishes an accepted-result input with exact producing Flow/terminal/schema constraints and selects an eligible Run
- **THEN** its ordinary validation and recovery remain available without requiring the result to be copied into Issue fields or disabling other Issue-snapshot consumers
