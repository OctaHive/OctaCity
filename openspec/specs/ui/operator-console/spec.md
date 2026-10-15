# Operator Console Specification

## Purpose

Defines the trusted-network browser console through which operators discover, observe, and safely control an OctaCity deployment without replacing its REST control plane.

## Requirements

### Requirement: Trusted same-origin console deployment
The operator console SHALL be a separately deployable static browser application whose source and toolchain are rooted directly at `ui/`, without an additional single-application directory or repository-level JavaScript workspace. It SHALL be served from the same trusted origin that proxies `/api/v1` to the private management listener. It SHALL own an application entry point and browser bootstrap and SHALL produce a directly deployable SPA bundle containing `index.html` and content-hashed static assets. Its release archive SHALL include the copy-ready same-origin proxy fixture referenced by its deployment guide so deployment does not depend on a repository checkout. It SHALL NOT expose a reusable library entry point, package export surface, or registry publication contract. It SHALL use relative management URLs and SHALL NOT require or collect an operator credential. Operator deployment documentation SHALL identify the deployment as unauthenticated trusted-network access. The deployment contract SHALL NOT require cross-origin management access or expose the management listener directly to an untrusted network.

#### Scenario: Operator opens the console on the trusted network
- **WHEN** an operator opens the configured console origin
- **THEN** static application assets load from that origin, management requests use its `/api/v1` path, and the console presents no login flow or persistent trusted-network warning banner

#### Scenario: Deep link is refreshed
- **WHEN** an operator refreshes a console URL for a Project, Build, Agent, or Agent Pool
- **THEN** the static host returns the application shell while `/api/v1` remains routed only to the management API

#### Scenario: Cross-origin API configuration is absent
- **WHEN** the console is deployed according to the supported contract
- **THEN** it does not require permissive CORS, browser credentials, forwarded operator identity, or storage of an authentication token

#### Scenario: Production application is built
- **WHEN** the console production build completes
- **THEN** it emits a runnable static SPA rooted at `index.html` without a library bundle, public component exports, package-registry artifact, or Node runtime

#### Scenario: Released console is deployed without source checkout
- **WHEN** an operator verifies and extracts the console release archive
- **THEN** the archive provides both the static SPA and the copy-ready same-origin proxy fixture referenced by its deployment guide

### Requirement: Deterministic frontend authoring boundary
The console SHALL pin its supported Node, package-manager, compiler, formatter, linter, and test tool versions. Its lint gate SHALL cover JavaScript, TypeScript, React, React Hooks, and browser DOM correctness and SHALL reject unused lint suppressions. Repository-level editor and Git text rules SHALL normalize UTF-8 source text, final newlines, trailing whitespace, and LF line endings without introducing UI-only policy for shared file types.

#### Scenario: Frontend quality gates run
- **WHEN** a contributor runs the documented frontend format, lint, type-check, test, and build commands from `ui/`
- **THEN** the pinned tools evaluate the standalone application deterministically and stale lint suppressions fail the lint command

#### Scenario: Cross-platform source is checked in
- **WHEN** a supported text source file is edited on a platform with different editor or line-ending defaults
- **THEN** repository policy normalizes the committed text to the declared UTF-8 and LF conventions

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

### Requirement: Localized themes and bounded local preferences
The console SHALL provide complete English and Russian operator-facing translations and light, dark, and system-following variants of the evergreen token system. It SHALL update document language and locale-sensitive formatting with the language selected from the operator menu and SHALL preserve non-secret presentation preferences in one versioned, validated, bounded browser-local record. That record MAY contain only language, theme mode, explorer visibility or width, expanded hierarchy identities, recent resource identities, favorite resource identities, and the locally generated time at which the notification center was last opened. The console SHALL NOT persist API payloads, notification items, query caches, mutation bodies, logs, request identities, credentials, download capabilities, or other server-derived content. Without browser authentication or forwarded identity, the operator menu SHALL NOT claim a personal profile or offer a misleading sign-out action.

#### Scenario: Operator changes presentation preferences
- **WHEN** the operator selects a language or theme and later reloads the console in the same browser
- **THEN** the validated preference is restored without persisting server data or weakening the trusted same-origin deployment boundary

#### Scenario: System theme changes
- **WHEN** theme mode is `system` and the operating-system color preference changes
- **THEN** the console applies the corresponding complete token palette while preserving contrast, state meaning, and reduced-motion behavior

#### Scenario: Local preference data is malformed or excessive
- **WHEN** the stored preference record has an unknown version, invalid resource kind, invalid identifier, or exceeds its declared bounds
- **THEN** the console discards the unsafe portion or resets the record without sending it to the server or preventing application startup

#### Scenario: No authenticated browser identity exists
- **WHEN** the console runs in its declared trusted-network mode
- **THEN** the header presents a neutral operator menu for language, local preferences, documentation, and deployment information without fabricating a name, email address, profile, or logout flow

### Requirement: Relevant operator notification center
The console SHALL expose a header notification center containing only three bounded categories: outcomes of commands issued by the current browser session, attention-worthy state changes for locally favorited Builds, Agents, and Agent Pools, and server-classified critical system conditions. It SHALL NOT display all Builds, all audit facts, ordinary unrelated successes, raw Job events, or informational system activity as notifications. A command outcome SHALL first appear as immediate accessible feedback and MAY also remain in the in-memory current-session notification history. Favorite-resource and critical-system items SHALL come from the published bounded attention feed rather than from client-side severity inference. The badge SHALL represent locally unseen relevant items, not a server-side personal unread count, and opening the center SHALL update only its locally generated last-opened time.

#### Scenario: Current-browser command completes
- **WHEN** a confirmed command issued by the current browser receives a definitive success, rejection, conflict, or failure response
- **THEN** the console presents immediate accessible feedback and records a bounded in-memory notification correlated to that intent without persisting its request identity or response payload

#### Scenario: Favorite resource needs attention
- **WHEN** the bounded attention feed reports an attention-worthy transition for a Build, Agent, or Agent Pool whose identity is in the local favorites list
- **THEN** the notification center presents the safe typed item and links to the canonical resource view without treating the favorite as authority

#### Scenario: Critical system condition is active
- **WHEN** the server reports a current or newly observed critical system condition
- **THEN** every console may surface it regardless of local favorites while preserving the server-provided severity and resolution state

#### Scenario: Unrelated activity occurs
- **WHEN** another operator performs an ordinary action or an unrelated resource changes normally
- **THEN** the notification center does not add that event merely because it exists in audit or operational history

#### Scenario: Notification center is reopened after reload
- **WHEN** the browser reloads without an authenticated identity
- **THEN** favorite-resource and critical items are reconstructed from the bounded feed, current-session command history is not fabricated, and seen state is described as local to that browser

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

### Requirement: Build execution diagnostics
The Build view SHALL expose the Build and current Attempt state, the Attempt's Job dependency graph, Job state and failure classification, ordered Job events, redacted log search, published Artifacts, cache-session diagnostics, and Build Result retention state when each resource is available. Active Job event following SHALL use the REST API's bounded wait and sequence cursor and SHALL stop or back off when the view is no longer active or the Job is terminal.

#### Scenario: Operator investigates a failed Build
- **WHEN** an operator opens a failed Build and selects its failed Job
- **THEN** the console identifies the causal DAG state, failure classification, ordered events, and available redacted log evidence without exposing credentials or private storage locations

#### Scenario: Operator follows an active Job
- **WHEN** an active Job produces events after the last displayed sequence
- **THEN** the console requests a bounded follow from that sequence and appends only the next contiguous ordered events

#### Scenario: Search projection is behind
- **WHEN** a log-search response reports an indexed watermark behind its committed watermark
- **THEN** the console displays that results may be incomplete instead of presenting an empty result as authoritative

#### Scenario: Artifact download is requested
- **WHEN** an operator requests an available Artifact download
- **THEN** the console obtains a short-lived download capability through REST and starts the download without displaying, persisting, or logging its private URL

### Requirement: Guarded idempotent operator actions
The console SHALL support manual Build triggering, Build cancellation and retry, placing and releasing Build Result holds, Agent drain, and idle Agent pool reassignment. Each user intent SHALL receive one fresh idempotency key that is reused for transport retries of the same request, concurrent duplicate submission SHALL be prevented, and versioned operations SHALL send the resource precondition obtained by the console. Disruptive actions SHALL require an explicit confirmation that names the target and consequence.

#### Scenario: Mutation response is lost
- **WHEN** a mutation request may have committed but its response is lost and the operator retries it from the still-active intent
- **THEN** the console reuses the original idempotency key and unchanged request rather than creating a second logical command

#### Scenario: Operator confirms Build cancellation
- **WHEN** an operator confirms cancellation of a named active Build
- **THEN** the console submits one cancellation command, prevents another submission while it is pending, and refreshes affected Build data after a definitive response

#### Scenario: Resource version is stale
- **WHEN** the server rejects an action because the submitted precondition is stale
- **THEN** the console preserves the operator's context, explains the conflict, and offers a refresh instead of silently resubmitting against a newer version

### Requirement: Capacity and audit visibility
The console SHALL expose bounded Agent and Agent Pool collections, their contextual `Agent Pool -> Agent` explorer hierarchy, Agent inventory and state, pool assignment and version, drain state, optional current execution, and searchable audit facts using only the published management representations. Every enrolled Agent SHALL appear under its authoritative current Pool; the console SHALL NOT synthesize an unassigned state or derive workload scheduling compatibility from capacity facts. Audit filtering SHALL support the server's actor, operation, target, request identity, and time-range filters without reconstructing authority from UI state.

#### Scenario: Operator diagnoses unavailable capacity
- **WHEN** an operator opens an Agent or Agent Pool associated with unavailable capacity
- **THEN** the console shows the published inventory, readiness, assignment, drain, and current execution information needed to distinguish capacity from scheduling policy

#### Scenario: Operator opens an idle Agent
- **WHEN** the selected Agent has no published current execution
- **THEN** the console identifies the Agent as idle without treating it as unassigned or claiming that it can satisfy a particular workload

#### Scenario: Operator follows a failed mutation to audit evidence
- **WHEN** a management response includes a safe request identity
- **THEN** the operator can use that identity in the audit view without the console exposing credentials or treating audit data as an authorization source

### Requirement: Explicit asynchronous and error states
Every console data boundary SHALL distinguish initial loading, incremental refresh, empty data, stale data, bounded retry, and terminal failure. Stable REST error codes and safe request correlation SHALL remain visible to the operator. Rate limits SHALL honor the server-provided retry delay, forbidden responses SHALL NOT trigger credential prompts, and not-found responses SHALL NOT be converted into empty collections.

#### Scenario: Management read is rate limited
- **WHEN** a console request receives the stable rate-limited response with a bounded retry delay
- **THEN** the console retains the current view, reports the delay, and does not retry sooner than the server permits

#### Scenario: Management action is forbidden
- **WHEN** the API returns the stable forbidden response
- **THEN** the console reports the denied action and request correlation without attempting login, revealing policy details, or assuming the target exists

#### Scenario: Refresh fails after data was displayed
- **WHEN** a background refresh fails after a successful response was already rendered
- **THEN** the console retains and marks the prior data as stale instead of replacing it with an empty state

### Requirement: Accessible data-dense interaction
The initial console SHALL support keyboard operation, semantic control labels, visible focus, non-color-only state communication, readable contrast, and text alternatives for graphical Build dependencies. Its primary layout SHALL support desktop operator workflows while preserving access to content and actions at narrower supported widths without requiring a separate mobile application.

#### Scenario: Operator uses only a keyboard
- **WHEN** an operator navigates lists, tabs, dialogs, filters, and actions without a pointing device
- **THEN** focus order remains understandable, the current focus is visible, and every supported action remains reachable

#### Scenario: Build graph cannot be interpreted visually
- **WHEN** an operator uses assistive technology or switches to the non-graph representation
- **THEN** the same Jobs, states, and dependency relationships are available as structured text or a table

### Requirement: REST remains the only business-operation boundary
The console SHALL perform management work only through the published REST contract and SHALL NOT duplicate domain decisions, infer hidden state, connect directly to PostgreSQL or object storage, or require a console-specific server session. Existing HTTP clients SHALL remain able to complete every workflow exposed by the console.

#### Scenario: Console is not deployed
- **WHEN** an OctaCity installation omits the static console package
- **THEN** the management REST API and all existing headless workflows continue to operate unchanged

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
