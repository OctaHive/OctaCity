## Purpose

Defines the trusted-network browser console through which operators discover, observe, and safely control an OctaCity deployment without replacing its REST control plane.

## ADDED Requirements

### Requirement: Trusted same-origin console deployment
The operator console SHALL be a separately deployable static browser application whose source and toolchain are rooted directly at `ui/`, without an additional single-application directory or repository-level JavaScript workspace. It SHALL be served from the same trusted origin that proxies `/api/v1` to the private management listener. It SHALL own an application entry point and browser bootstrap and SHALL produce a directly deployable SPA bundle containing `index.html` and content-hashed static assets. It SHALL NOT expose a reusable library entry point, package export surface, or registry publication contract. It SHALL use relative management URLs, SHALL NOT require or collect an operator credential, and SHALL identify the deployment as unauthenticated trusted-network access. The deployment contract SHALL NOT require cross-origin management access or expose the management listener directly to an untrusted network.

#### Scenario: Operator opens the console on the trusted network
- **WHEN** an operator opens the configured console origin
- **THEN** static application assets load from that origin, management requests use its `/api/v1` path, and the console visibly identifies the trusted-network security mode without presenting a login flow

#### Scenario: Deep link is refreshed
- **WHEN** an operator refreshes a console URL for a Project, Build, Agent, or Agent Pool
- **THEN** the static host returns the application shell while `/api/v1` remains routed only to the management API

#### Scenario: Cross-origin API configuration is absent
- **WHEN** the console is deployed according to the supported contract
- **THEN** it does not require permissive CORS, browser credentials, forwarded operator identity, or storage of an authentication token

#### Scenario: Production application is built
- **WHEN** the console production build completes
- **THEN** it emits a runnable static SPA rooted at `index.html` without a library bundle, public component exports, package-registry artifact, or Node runtime

### Requirement: Deterministic frontend authoring boundary
The console SHALL pin its supported Node, package-manager, compiler, formatter, linter, and test tool versions. Its lint gate SHALL cover JavaScript, TypeScript, React, React Hooks, and browser DOM correctness and SHALL reject unused lint suppressions. Repository-level editor and Git text rules SHALL normalize UTF-8 source text, final newlines, trailing whitespace, and LF line endings without introducing UI-only policy for shared file types.

#### Scenario: Frontend quality gates run
- **WHEN** a contributor runs the documented frontend format, lint, type-check, test, and build commands from `ui/`
- **THEN** the pinned tools evaluate the standalone application deterministically and stale lint suppressions fail the lint command

#### Scenario: Cross-platform source is checked in
- **WHEN** a supported text source file is edited on a platform with different editor or line-ending defaults
- **THEN** repository policy normalizes the committed text to the declared UTF-8 and LF conventions

### Requirement: Contextual operator workbench navigation
The console SHALL use one compact utility header, one primary section rail with large icon-and-short-label controls, one collapsible and resizable contextual explorer, and one primary resource detail area. The utility header SHALL expose bounded global resource search through a keyboard-accessible command center, deployment readiness, a direct theme control, a notification-center control, and a neutral operator menu containing the less-frequent language control. The section rail SHALL switch between Projects, Builds, Agents, and Audit without consuming the contextual explorer with duplicate global navigation or relying on unlabeled icons. The explorer SHALL default to 288 pixels, SHALL be pointer- and keyboard-resizable only within 240–480 pixels, and SHALL persist only a validated clamped width. Its sticky header SHALL identify the active section and provide search and collapse controls. The search control SHALL open the same command center as global search with the active resource kinds preselected. A Favorites section SHALL precede the active section's lazily paginated resource hierarchy or filters. Selecting an explorer resource SHALL update the directly addressable detail view without discarding the explorer context.

#### Scenario: Operator switches workbench activity
- **WHEN** an operator activates Projects, Builds, Agents, or Audit from the primary section rail
- **THEN** the explorer changes to that section's navigation model, the large selected control and explorer title remain visibly and semantically identified, and the primary detail area preserves or opens the resource addressed by the URL

#### Scenario: Operator explores Builds
- **WHEN** the operator expands the Build explorer
- **THEN** it presents Favorites before lazily loading visible Projects, their Build Configurations, and newest Builds as a bounded `Project -> Build Configuration -> Build` hierarchy with non-color state text

#### Scenario: Operator explores Agents
- **WHEN** the operator expands the Agent explorer
- **THEN** it lazily presents visible Agent Pools and their assigned Agents as a bounded `Agent Pool -> Agent` hierarchy, presents unassigned Agents separately, and distinguishes readiness and drain state without client-side scheduling inference

#### Scenario: Operator opens the command center
- **WHEN** the operator focuses global search or presses `Command/Ctrl+K`
- **THEN** the console offers keyboard-navigable, type-grouped, authorization-filtered results for Projects, Builds, Agents, and Agent Pools plus non-destructive navigation and presentation commands

#### Scenario: Operator searches from the explorer
- **WHEN** the operator activates the search control in the Build, Agent, or Project explorer header
- **THEN** the shared command center opens with the corresponding resource kinds preselected and permits explicitly clearing that scope for a global search

#### Scenario: Operator resizes the explorer
- **WHEN** the operator drags the explorer separator or adjusts it with the keyboard
- **THEN** its width changes only within 240–480 pixels, the detail area remains usable, and the validated clamped width is restored on the next browser load

#### Scenario: Stored explorer width is invalid
- **WHEN** the persisted width is malformed or outside the supported bounds
- **THEN** the console clamps or resets it to the 288-pixel default without breaking workbench layout or focus order

#### Scenario: Explorer space is constrained
- **WHEN** the operator collapses the explorer or uses a supported narrow viewport
- **THEN** the primary detail remains usable and the explorer can be reopened as a bounded overlay without hiding the active section, losing its title, search, or Favorites controls, or trapping keyboard focus

### Requirement: Localized themes and bounded local preferences
The console SHALL provide complete English and Russian operator-facing translations and light, dark, and system-following variants of the evergreen token system. It SHALL update document language and locale-sensitive formatting with the language selected from the operator menu and SHALL preserve non-secret presentation preferences in one versioned, validated, bounded browser-local record. That record MAY contain only language, theme mode, explorer visibility or width, expanded hierarchy identities, recent resource identities, favorite resource identities, and the locally generated time at which the notification center was last opened. The console SHALL NOT persist API payloads, notification items, query caches, mutation bodies, logs, request identities, credentials, download capabilities, or other server-derived content. Without browser authentication or forwarded identity, the operator menu SHALL NOT claim a personal profile or offer a misleading sign-out action.

#### Scenario: Operator changes presentation preferences
- **WHEN** the operator selects a language or theme and later reloads the console in the same browser
- **THEN** the validated preference is restored without persisting server data or weakening the trusted-network warning

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
The console SHALL provide stable, directly addressable views for the Project hierarchy, one Project, one Build, Agents, Agent Pools, and audit facts. A Project view SHALL discover its current Pipelines, Repositories, Build Configurations, Trigger definitions, and recent Builds through bounded REST collections rather than requiring an operator to enter opaque identifiers. Activity, selected resource, shareable filters, and durable diagnostic selection SHALL remain URL-addressable; presentation-only explorer expansion, width, theme, language, favorites, recents, and notification-center last-opened time SHALL remain local preferences rather than URL state.

#### Scenario: Operator navigates from a Project to a Build
- **WHEN** an operator selects a Build from a Project's recent Build collection
- **THEN** the browser URL identifies that Build and can be copied or refreshed without losing the selected resource

#### Scenario: Project collection is empty
- **WHEN** a Project has no current resources or Builds in one section
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
The console SHALL expose bounded Agent and Agent Pool collections, their contextual `Agent Pool -> Agent` explorer hierarchy, Agent inventory and state, pool assignment and version, drain state, and searchable audit facts using only the published management representations. Audit filtering SHALL support the server's actor, operation, target, request identity, and time-range filters without reconstructing authority from UI state.

#### Scenario: Operator diagnoses unavailable capacity
- **WHEN** an operator opens an Agent or Agent Pool associated with unavailable capacity
- **THEN** the console shows the published inventory, readiness, assignment, drain, and current execution information needed to distinguish capacity from scheduling policy

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
