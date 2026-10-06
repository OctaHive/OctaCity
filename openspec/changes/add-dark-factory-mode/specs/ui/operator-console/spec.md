## MODIFIED Requirements

### Requirement: Contextual operator workbench navigation
The console SHALL use one compact utility header, one primary section rail with large icon-and-short-label controls, one collapsible and resizable contextual explorer, and one primary resource detail area. The utility header SHALL expose bounded global resource search through a keyboard-accessible command center, deployment readiness, a direct theme control, a notification-center control, and a neutral operator menu containing the less-frequent language control. Selection controls SHALL use the console's token-owned listbox presentation rather than browser-native select popups. Non-modal popups SHALL close when pointer or focus interaction moves outside them and SHALL remain dismissible with the keyboard. The section rail SHALL switch between Projects, Builds, Factory, Agents, and Audit without consuming the contextual explorer with duplicate global navigation or relying on unlabeled icons. The explorer SHALL default to 288 pixels, SHALL be pointer- and keyboard-resizable only within 240–480 pixels, and SHALL persist only a validated clamped width. Its sticky header SHALL identify the active section and provide search and collapse controls. The Project, Build, and Agent search controls SHALL open the same command center as global search with the active resource kinds preselected; the Factory explorer SHALL provide bounded URL-backed server filters until Factory Runs are added to global resource search. A Favorites section SHALL precede the active section's lazily paginated resource hierarchy or filters. Selecting an explorer resource SHALL update the directly addressable detail view without discarding the explorer context.

#### Scenario: Operator switches workbench activity
- **WHEN** an operator activates Projects, Builds, Factory, Agents, or Audit from the primary section rail
- **THEN** the explorer changes to that section's navigation model, the large selected control and explorer title remain visibly and semantically identified, and the primary detail area preserves or opens the resource addressed by the URL

#### Scenario: Operator navigates leaf Projects
- **WHEN** a visible Project has no visible direct children
- **THEN** the Project explorer renders it without a disclosure control and does not request or display an empty child branch

#### Scenario: Operator explores Builds
- **WHEN** the operator expands the Build explorer
- **THEN** it presents Favorites before lazily loading visible Projects, their Build Configurations, and newest Builds as a bounded `Project -> Build Configuration -> Build` hierarchy with non-color state text

#### Scenario: Operator explores Factory Runs
- **WHEN** the operator opens the Factory explorer
- **THEN** it presents Favorites and bounded URL-backed filters before lazily loading visible Projects, Factory Configurations, and newest Factory Runs as a `Project -> Factory Configuration -> Factory Run` hierarchy with current-stage, decision, escalation, and delivery state text

#### Scenario: Operator explores Agents
- **WHEN** the operator expands the Agent explorer
- **THEN** it lazily presents visible Agent Pools and their assigned Agents as a bounded `Agent Pool -> Agent` hierarchy, places every enrolled Agent under its authoritative current Pool without a synthetic unassigned section, and distinguishes readiness and drain state without client-side scheduling inference

#### Scenario: Operator opens the command center
- **WHEN** the operator focuses global search or presses `Command/Ctrl+K`
- **THEN** the console offers keyboard-navigable, type-grouped, authorization-filtered results for Projects, Builds, Agents, and Agent Pools plus non-destructive navigation and presentation commands

#### Scenario: Operator uses a selection popup
- **WHEN** the operator opens a theme, language, filter, or command selection control
- **THEN** the console presents a consistently styled keyboard-navigable listbox and closes it after selection, `Escape`, or interaction outside the popup

#### Scenario: Operator searches from the explorer
- **WHEN** the operator activates the search control in the Build, Agent, or Project explorer header
- **THEN** the shared command center opens with the corresponding resource kinds preselected and permits explicitly clearing that scope for a global search

#### Scenario: Operator filters the Factory explorer
- **WHEN** the operator selects Factory Configuration, lifecycle state, decision, delivery state, or time filters
- **THEN** the filters round-trip through the URL and every loaded branch uses the same bounded server-side scope

#### Scenario: Operator resizes the explorer
- **WHEN** the operator drags the explorer separator or adjusts it with the keyboard
- **THEN** its width changes only within 240–480 pixels, the detail area remains usable, and the validated clamped width is restored on the next browser load

#### Scenario: Stored explorer width is invalid
- **WHEN** the persisted width is malformed or outside the supported bounds
- **THEN** the console clamps or resets it to the 288-pixel default without breaking workbench layout or focus order

#### Scenario: Explorer space is constrained
- **WHEN** the operator collapses the explorer or uses a supported narrow viewport
- **THEN** the primary detail remains usable and the explorer can be reopened as a bounded overlay without hiding the active section, losing its title, search or filters, or Favorites controls, or trapping keyboard focus

### Requirement: URL-addressable operator navigation
The console SHALL provide stable, directly addressable views for the Project hierarchy, one Project, one Build, Factory Runs, one Factory Run, Agents, Agent Pools, and audit facts. A Project view SHALL discover its current Pipelines, Repositories, Build Configurations, Trigger definitions, Factory Configurations, recent Builds, and recent Factory Runs through bounded REST collections rather than requiring an operator to enter opaque identifiers. Activity, selected resource, shareable filters, current Factory stage, selected evaluation branch, and durable diagnostic selection SHALL remain URL-addressable; presentation-only explorer expansion, width, theme, language, favorites, recents, and notification-center last-opened time SHALL remain local preferences rather than URL state.

#### Scenario: Operator navigates from a Project to a Build
- **WHEN** an operator selects a Build from a Project's recent Build collection
- **THEN** the browser URL identifies that Build and can be copied or refreshed without losing the selected resource

#### Scenario: Operator navigates to a Factory Run
- **WHEN** an operator selects a Factory Run from a Project or Factory explorer
- **THEN** the browser URL identifies that run and preserves the selected stage or evaluation branch and bounded filters across copy and refresh

#### Scenario: Project collection is empty
- **WHEN** a Project has no current resources, Builds, or Factory Runs in one section
- **THEN** that section presents an explicit empty state and does not present absence as a transport failure

#### Scenario: Collection has another page
- **WHEN** a REST collection returns a continuation cursor
- **THEN** the console can request the next bounded page without guessing offsets or loading an unbounded collection

## ADDED Requirements

### Requirement: Guided Factory Configuration management
The console SHALL provide typed guided creation and immutable-version replacement for Factory Configurations rather than limiting Factory mode to read-only views or exposing a raw JSON editor. The editor SHALL organize admission, stages and Build Configurations, WIP and hard budgets, permissions, optional Decision Signal routing and tool-risk profiles, criterion packs, evaluators and quorum, rework, delivery, and enabled state into reviewable sections. It SHALL populate every resource reference from bounded authorization-filtered REST choices, SHALL NOT require an operator to enter an opaque identifier, provider credential, model alias, or provider payload manually, and SHALL distinguish unavailable, incompatible, hidden, and unconfigured choices without deriving authority on the client.

The editor SHALL start from an empty supported template or a selected current immutable version, keep an unfinished draft only in component memory, perform generated-type and bounded field validation, request authoritative server validation, and present the normalized configuration, effective permission narrowing, required Agent/backend capabilities, unresolved incompatibilities, and current-version precondition before confirmation. Publication SHALL use the shared immutable confirmed-intent helper and SHALL create one new version rather than mutating the current version. Secrets, task bodies, prompts, provider credentials, and unfinished configuration drafts SHALL NOT be placed in browser persistence, URLs, logs, analytics, or error reports.

#### Scenario: Operator creates a first Factory Configuration
- **WHEN** an authorized operator opens Factory mode for a visible Project with no current configuration
- **THEN** the console offers a guided empty configuration, selectable visible dependencies, contextual constraints, validation, and a review step before publishing one immutable version

#### Scenario: Operator replaces a Factory Configuration
- **WHEN** an operator starts from the current version, changes bounded policy, and confirms the reviewed request using the displayed current-version precondition
- **THEN** the console publishes a new immutable version, preserves the previous version for existing runs and history, and refreshes only affected Factory and Project queries

#### Scenario: Selected capability is incompatible
- **WHEN** authoritative validation reports that a selected stage, permission, evaluator, delivery adapter, or required enforcement capability is unavailable under current policy
- **THEN** the editor associates the safe error with its section, prevents publication, and allows correction without discarding unrelated entered values

#### Scenario: Operator configures a Decision Signal profile
- **WHEN** an operator enables routing or tool-risk signals
- **THEN** the editor offers only compatible server-published adapter/model/profile choices, shows the resolved exact identity, purpose-specific `shadow`, `advisory`, or `bounded_control` mode, thresholds, fail-closed fallback and calibration status, and explains that the signal cannot grant authority or skip mandatory gates

#### Scenario: Configuration changes during editing
- **WHEN** publication fails because another operator replaced the current version after the editor opened
- **THEN** the console preserves the in-memory form, shows the version conflict and changed authoritative baseline, and requires explicit reconciliation and a new confirmed intent instead of silently overwriting or replaying against the new version

#### Scenario: Operator abandons configuration authoring
- **WHEN** the operator leaves or reloads the editor before publication
- **THEN** unfinished values are discarded and no configuration version, browser-persisted draft, idempotency outcome, or audit mutation is created

### Requirement: Factory Run diagnostics
The Factory Run view SHALL expose the safe Work summary, exact base and candidate identities, current state and version, Stage Attempt timeline and linked Builds, bounded budget consumption, Task Envelope and permission summary, Decision Signal requests, receipts and consuming dispositions, ChangeSets, deterministic evidence, evaluation plan and branch states, Assessments, Decisions and reasons, escalation, delivery-for-review, reporting state, and audit correlation available through REST. The console SHALL present program-selected lifecycle state and SHALL NOT derive a Decision, scheduling compatibility, missing transition, authorization, or delivery success from model prose, probability, or client state.

#### Scenario: Operator investigates an active run
- **WHEN** a Factory Run is waiting on parallel validation or evaluation branches
- **THEN** the view distinguishes completed, active, retryable, failed, and pending-join branches and links each ordinary Build without claiming a final decision

#### Scenario: Operator investigates a rejected candidate
- **WHEN** the Decision Engine rejects or requests rework
- **THEN** the view associates reasons with the exact candidate, deterministic evidence, Assessments, policy version, and next bounded action using non-color labels and accessible structured text

#### Scenario: Operator investigates a Decision Signal
- **WHEN** routing or tool-risk assessment was requested in shadow, advisory, or bounded-control mode
- **THEN** the view separately labels its purpose, exact provider/model, typed answers, confidence semantics, mode, fallback, receipt digest, deterministic baseline and final policy disposition without presenting the signal as an authoritative Decision

#### Scenario: Sensitive factory data exists
- **WHEN** Task Envelopes, provider traces, delivery attempts, or Assessments reference protected data
- **THEN** the console renders only the published safe projection and never displays, persists, or logs raw prompts, hidden reasoning, credentials, private transfer URLs, or unredacted findings

### Requirement: Guarded Factory commands
The console SHALL support manual Work admission from selected visible Project, Factory Configuration, Repository, task and specification inputs; Factory Run cancellation; eligible Stage Attempt retry; bounded escalation disposition; and delivery-for-review request for the current accepted candidate. Each intent SHALL use the shared immutable confirmed-intent helper, one idempotency key for transport replay, the current strong precondition, target-specific confirmation, targeted query invalidation, and visible request/audit correlation. The console SHALL NOT expose arbitrary lifecycle-state, Assessment, Decision, candidate, success, or merge setters.

#### Scenario: Operator admits Work
- **WHEN** an operator selects a valid visible Factory Configuration and confirms bounded manual Work input
- **THEN** the console submits one typed command and opens the returned Factory Run without requiring an opaque identity to be typed manually

#### Scenario: Operator confirms delivery for review
- **WHEN** a Factory Run has a current accepted Decision and the operator confirms the exact repository and candidate
- **THEN** the console submits one preconditioned delivery intent, prevents duplicate submission, and presents the authoritative delivery outcome without claiming merge

#### Scenario: Factory Run changes before confirmation
- **WHEN** the server rejects a command because the displayed run or candidate version is stale
- **THEN** the console preserves context, explains the conflict, and offers a refresh rather than rebuilding or silently resubmitting the intent

### Requirement: Factory states remain optional and explicit
Every Factory collection, detail, branch, and command boundary SHALL use the console's existing loading, incremental refresh, empty, stale, rate-limited, forbidden, not-found, conflict, unavailable, and internal-error presentations. A deployment or Project with no Factory Configuration SHALL present an intentional optional-mode empty state and SHALL NOT make existing Project, Build, Agent, or Audit activities unavailable.

#### Scenario: Model provider is unavailable
- **WHEN** Factory diagnostics report a blocked or unavailable provider while ordinary server readiness remains available
- **THEN** the Factory view reports the scoped failure and request correlation while ordinary CI/CD navigation and actions remain usable

#### Scenario: Factory mode is not configured
- **WHEN** the operator opens Factory for a deployment with no visible Factory Configuration
- **THEN** the detail area explains that the optional mode is not configured and does not present this as a transport or global readiness failure
