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

### Requirement: Pipeline inputs expose guided GitHub Issue selection
The console SHALL allow an operator to configure GitHub Issues as a source at the selected Factory pipeline input, choosing an existing Project-authorized connection and external task container independently of the code Repository, visible installed capabilities, target configuration, Issue state, all-label, any-label and excluded-label sets, input mapping, optional exclusive group and enabled state. It SHALL explain empty label selection as unrestricted, provide bounded read-only preview and authoritative validation, and publish reviewed immutable replacements through the existing confirmed-intent workflow. It SHALL explain that these predicates configure automatic execution intake rather than global internal-task import/refresh or a manual card execution request. The operator SHALL NOT need to write provider JSON, enter opaque resource identities or supply credentials in the pipeline editor. A missing connection or grant SHALL link to the separate Integrations administration/access flow without enabling unsupported choices. Selector drafts and preview payloads SHALL remain outside browser persistence.

#### Scenario: Operator prepares assessment and implementation inputs
- **WHEN** an operator configures an assessment input with no required labels and separate `approved + bug` and `approved + feature-request` inputs
- **THEN** the console shows each input next to its exact target pipeline and makes exclusions and exclusive grouping reviewable before activation

#### Scenario: Operator selects no required labels
- **WHEN** all three label sets are empty
- **THEN** the console explains that labeled and unlabeled Issues can match and does not silently add a label restriction

#### Scenario: Server rejects an incompatible input
- **WHEN** validation reports a hidden dependency, contradictory selector, unsupported mapping or stale version
- **THEN** the console identifies the safe error in the appropriate section, preserves the in-memory draft and prevents activation until a new valid intent is confirmed

### Requirement: GitHub reporting mappings and source progress are reviewable
The console SHALL expose guided configuration of arbitrary Work status values mapped to permitted managed-label additions/removals and optional Issue open/close state. It SHALL present safe Source/Reporter status and Work/Run correlations from REST, distinguish matching from admitted and pending projection from applied effects, and identify ambiguous grouped inputs without deriving lifecycle authority from GitHub labels. Reporting and source choices SHALL be filtered by server-published capabilities; no control SHALL grant itself write authority or expose protected provider material.

#### Scenario: Operator configures assessment reporting
- **WHEN** an operator maps accepted `approved` and `rejected` outcomes to mutually exclusive managed labels
- **THEN** review shows exact label effects, unrelated-label preservation and whether Issue state will change before publication

#### Scenario: Status projection is pending
- **WHEN** a Run completed assessment but its GitHub update is retrying or exhausted
- **THEN** the console shows those distinct facts and does not display the downstream pipeline as admitted without an authoritative admission receipt

### Requirement: Integration UI interoperates with optional internal task cards
Integration/source views SHALL keep external native task identity, automatic input admission and internal Issue identity distinct. They SHALL link to a task card only when REST returns an enabled visible association, preserve existing Work/Run navigation when no card is available, and avoid another Issue editor or synchronization conflict resolver in pipeline forms. When the task capability is enabled, manual Issue creation SHALL stay outside integration setup and require no provider connection; existing manual Artifact admission SHALL remain available independently. Preview SHALL be presented as read-only, not an import/create/synchronize action. Business task status, execution result and reporting health SHALL remain distinct in shared progress presentation.

#### Scenario: Card capability is unavailable
- **WHEN** an operator inspects native source correlation and admitted Work/Run records
- **THEN** existing diagnostics remain usable without a mandatory Issue link, dead card action or task creation during navigation

#### Scenario: Operator wants to create a task manually
- **WHEN** an enabled task capability offers manual Issue creation without a provider connection
- **THEN** integration UI does not redirect that journey through token setup, while pipeline manual admission keeps its existing independent path

### Requirement: Stage authoring exposes source label-update actions
The console SHALL let an operator configure a source-update action for any declared stage outcome in the existing Flow editor, including intermediate and nested stages. The editor SHALL expose the original Issue source, permitted labels to add, labels to remove and the exact resulting mapping/effects, and SHALL persist them as ordinary immutable trusted-action configuration rather than a special stage implementation. Different outcomes SHALL support different effects, and stages without an action SHALL leave the Issue unchanged. No approval, rejection or classification vocabulary SHALL be mandatory.

#### Scenario: Operator adds an update after a nonterminal stage
- **WHEN** an operator selects an intermediate stage outcome and configures removal of one arbitrary label and addition of another
- **THEN** the editor shows the source-update action on that route, validates its authority through REST and allows publishing without moving it to the pipeline's terminal stage

#### Scenario: Operator inspects stage update progress
- **WHEN** an intermediate stage committed and its source update is pending, applied or failed
- **THEN** the console associates that reporting state with the exact stage/action and keeps it distinct from the overall pipeline result

### Requirement: Integrations has dedicated reusable connection administration
The console SHALL provide a dedicated provider-neutral Integrations section for authorized operators to list, create, inspect, replace, check and disable named connections, manage credentials and inspect permitted dependents. It SHALL present installed providers, allowed endpoints, supported authentication methods, permitted external resources and separate capability/readiness states through typed server choices. GitHub SHALL expose its repository owner/name context and token setup without making those fields mandatory for other providers. The section SHALL distinguish connection configuration from access granted to an OctaCity Project and from pipeline filtering/routing. Only installed supported capabilities SHALL be offered. Forms SHALL use existing review/version-conflict patterns, keyboard access and English/Russian messages.

#### Scenario: Operator creates one connection for multiple pipelines
- **WHEN** an administrator configures a named GitHub connection and grants selected resources to a Project
- **THEN** several pipeline editors can select that connection without copying its credentials or its per-pipeline predicates

#### Scenario: Provider is not installed
- **WHEN** a provider has no compatible installed adapter
- **THEN** the console identifies it as unavailable and cannot present metadata alone as a working source

### Requirement: Credential entry stays isolated and transient
The console SHALL collect raw credentials only in a separately authorized write-only integration setup/rotation flow, never in pipeline or ordinary connection metadata forms. Existing credentials SHALL be represented by configured/missing state and safe metadata, with replacement rather than value read-back. Secret input SHALL remain transient, SHALL NOT enter browser persistence, URLs, query/mutation caches, analytics, confirmation summaries or errors, and SHALL be cleared after submission or leaving the flow. Server errors SHALL be rendered as safe classifications without echoing the submitted value. Selecting an existing permitted server-side handle SHALL NOT require retrieving its contents.

#### Scenario: Administrator submits or rotates a token
- **WHEN** the protected credential form is submitted
- **THEN** the console shows a safe outcome, clears the secret field and exposes no token value in detail/review screens or cached state

#### Scenario: Administrator leaves an unfinished credential form
- **WHEN** the credential flow is closed or navigation leaves it
- **THEN** returning does not restore the secret from browser storage or client caches

### Requirement: Project access and capability readiness are explicit
The console SHALL let authorized administrators grant/revoke selected connection resources and purposes for an OctaCity Project, showing task-read, task-update, repository-read and Delivery access independently. Pipeline users SHALL see only their authorized choices and SHALL NOT gain connection-administration rights by creating a pipeline. Connection checks SHALL be visibly read-only and distinguish configuration, authentication, resource access and capability readiness from pipeline activation. Disablement/revocation SHALL show safe dependency effects without claiming to cancel already accepted remote requests or undo Work.

#### Scenario: Read access is available without Reporter access
- **WHEN** a Project has task-read access and no task-update grant
- **THEN** the console permits read intake, explains unavailable reporting and does not require a write credential to use the read source

#### Scenario: Administrator revokes access
- **WHEN** an administrator reviews revocation for a connection used by visible inputs and pending reporting
- **THEN** the console shows those dependents, explains blocked subsequent provider operations and preserves the distinction from accepted pipeline results

#### Scenario: Manual Factory intake remains selected
- **WHEN** the operator chooses manual intake with no provider connection
- **THEN** the console allows ordinary manual Work creation without opening integration setup or requiring tokens

### Requirement: Task navigation and manual creation precede execution setup
The Console SHALL provide a task-oriented list and manual-create entry point within the existing product shell. The list SHALL show human-readable keys and safe titles with bounded Project/business-status/priority/source/execution/synchronization filters. Manual creation SHALL ask for title, description and optional acceptance/metadata using generated bounds and SHALL work without an integration, repository or Factory Configuration. Saving SHALL open the canonical Issue card without starting execution. Missing execution prerequisites SHALL be explained at the explicit start action rather than blocking task creation.

#### Scenario: First task is created in an unconfigured Project
- **WHEN** an operator creates a valid manual task before configuring Factory or providers
- **THEN** the task appears under its human-readable key and opens its card with no Run and a clear execution-setup entry point

#### Scenario: Provider is unavailable
- **WHEN** an external source cannot be reached
- **THEN** local task navigation and manual creation remain available and affected linked tasks show their scoped synchronization condition

### Requirement: Project status settings and card moves are guided and independent
The Console SHALL expose bounded Project workflow settings through a graph canvas with editable status nodes, directed transition arrows and a contextual properties inspector. Initial and terminal selections SHALL remain workflow-level settings, with exactly one initial status and zero or more terminal statuses; permitted transition origins and supported evidence requirements SHALL use generated authoritative choices and reviewed immutable publication/default selection. The supplied default and custom workflows SHALL use the same viewing and editing controls. Manual creation SHALL work with the provided editable default without setup. Existing cards SHALL show their pinned version and require explicit reviewed compatible status correspondence for workflow adoption. Card controls SHALL enumerate only server-authorized organizational moves, explain ownership/evidence/applicability restrictions and confirm the current Issue/status precondition. History SHALL identify before/after status, reason and actor or pipeline/source provenance. A status move SHALL NOT be presented as execution start, gate approval or verification success. Stale intents SHALL require fresh review and preserve in-memory context without automatic resubmission.

#### Scenario: Project uses its own status vocabulary
- **WHEN** an operator publishes custom statuses and edges without a provider or configured pipeline
- **THEN** tasks use those settings through the same manual-create/move/card UI and no label spelling or phase name is required

#### Scenario: Old execution cannot move the current card
- **WHEN** the server classifies its configured action as stale or conflicting after a newer revision or status intent
- **THEN** the card retains its exact historical result and shows the unapplied transition rather than moving the card or relabeling the Run result

### Requirement: Workflow canvas displays the declared status graph
The Console SHALL use a graph canvas as the primary representation of the selected Project default and opened catalog workflow versions, including historical versions. Each status SHALL appear as a named node; each declared transition SHALL appear as a selectable, named, directed arrow. Initial and terminal selections SHALL have non-color-only markers. Branches, cycles, opposite directions and distinct transitions between the same statuses SHALL remain distinguishable. Status order or visual placement SHALL NOT imply undeclared transitions. Selecting a published node or arrow SHALL show its properties in a read-only inspector without mutating the workflow.

#### Scenario: Operator reads a branching workflow with a return path
- **WHEN** the opened version declares branches and a transition back to an earlier status
- **THEN** the canvas shows every declared node and directed transition, including the return path, and selection exposes the exact element's settings

#### Scenario: Operator opens an older workflow version
- **WHEN** an exact historical version is loaded from the catalog
- **THEN** its canvas and inspector show that version's statuses and transitions, with no arrows inferred from the status display order or current publication

### Requirement: Workflow authoring uses canvas nodes and a contextual inspector
Creating a workflow SHALL open an empty canvas with no copied default statuses or transitions. Operators SHALL be able to add and delete status nodes, create directed transitions by connecting nodes, delete transitions, and select either element to edit its properties in an inspector to the right of the desktop canvas. The inspector SHALL expose status names, workflow initial/terminal selections, and transition names, endpoints, origins and supported evidence through the existing workflow definition. Adding the first status SHALL select it as initial; removing a status SHALL remove its incident transitions and terminal reference, and removing the initial status SHALL select the first remaining status. Clearing a local draft SHALL be allowed, but an empty workflow SHALL NOT be published. All authoring controls SHALL edit one in-memory draft and use the existing REST validation, creation/publication and default-selection operations; pointer gestures SHALL NOT publish or select a default automatically. Existing task pins SHALL remain unchanged.

#### Scenario: Operator creates a workflow by drawing a transition
- **WHEN** the operator adds two named statuses to an empty canvas, connects them and configures the arrow in the inspector
- **THEN** validation and reviewed publication submit only those statuses and the configured transition through the existing workflow REST contract

#### Scenario: Operator removes the initial status
- **WHEN** the operator deletes the initial node with incoming or outgoing transitions
- **THEN** those transitions and its terminal reference disappear from the same draft, and the first remaining node becomes initial; a fully cleared draft remains unpublished

#### Scenario: Operator edits only the selected status
- **WHEN** an operator selects a status card on the canvas
- **THEN** the inspector exposes that status's name, initial selection, terminal flag and delete action without another status list, technical IDs or manual order controls
- **AND** the stored statuses array retains its order and identities, while only declared arrows define transitions

#### Scenario: Publication is rejected as stale
- **WHEN** the server rejects the reviewed workflow version precondition
- **THEN** the existing conflict flow preserves the local draft and requires refresh, validation and fresh review without silently overwriting the newer publication

### Requirement: Canvas navigation is accessible and separate from workflow policy
Operators SHALL be able to reposition nodes, pan, zoom and fit the graph to view. Layout, viewport and element selection SHALL remain transient presentation state and SHALL NOT change workflow status order, identities, transition policy or REST mutation bodies. Keyboard users SHALL be able to select nodes and transitions, add/remove them and establish connections through the same inspector and draft without pointer dragging. Focus and state labels SHALL remain visible, English/Russian presentation SHALL be complete, and the inspector SHALL remain reachable at supported narrow widths without horizontal page overflow. Draft and layout data SHALL NOT be persisted in browser storage.

#### Scenario: Operator rearranges the canvas
- **WHEN** the operator moves a node or changes the canvas viewport without changing policy fields
- **THEN** the graph presentation changes while the workflow definition and publication precondition remain unchanged and no mutation request is sent

#### Scenario: Operator authors a workflow with the keyboard at narrow width
- **WHEN** the operator adds/selects statuses, opens properties and creates a transition without dragging
- **THEN** the same draft can be configured and reviewed with understandable focus, and the inspector remains accessible within the page width

### Requirement: Pipeline Issue-status actions and native mapping have separate editors
Existing Flow authoring SHALL allow an authorized operator to select an installed Issue-status action on a declared outcome and review the exact workflow/version/edge and supported evidence, independently of source-update actions. Source-link mapping review SHALL separately show internal status IDs/names, native supported values/effects and field authority/direction; it SHALL not edit Project workflow structure. The UI SHALL explain unmapped/ambiguous observations, forbidden edges and overlapping producers, and offer only installed supported operations. A local action/manual task SHALL require no connection, token or external mapping. Completion without a configured status action SHALL display only execution progress. Both surfaces SHALL preserve generated bounds, stale-version review, keyboard/focus, English/Russian strings and in-memory-only drafts.

#### Scenario: Operator configures Triage business outcomes
- **WHEN** separate accepted eligible and declined branches select different explicit Issue transitions
- **THEN** review shows Ready versus Declined as configurable examples and no automatic Done move is attached to Run completion

#### Scenario: Business status is saved while native update is pending
- **WHEN** a configured local move succeeds and its external push is retrying
- **THEN** the UI separately shows the current task status, exact execution/action outcome and pending synchronization with no false rollback or verification claim

### Requirement: Canonical Issue card presents task execution and source state separately
The Console SHALL provide one URL-addressable Issue card with Overview, Activity, Runs, Results and Actions sections, exact historical selection, and links to original inputs and existing advanced Run/Build/log views. It SHALL separately display task business metadata, all active executions/current steps, available AI execution profile/model and assigned enrolled worker, and source link/synchronization health. Queued/unassigned/parallel/unavailable facts SHALL be explicit. Editable requirements SHALL show their revision and source ownership; no task-status edit SHALL be presented as verification acceptance or a Factory state setter.

#### Scenario: Two pipelines have processed the same Issue
- **WHEN** the operator opens the task after assessment and implementation executions
- **THEN** both Runs remain visible in the same card with exact input revisions, individual outcomes and separately accessible logs

#### Scenario: Operator selects a historical result
- **WHEN** its URL identifies an earlier execution/candidate
- **THEN** refresh preserves that historical selection and does not label it current proof or an actionable current request

### Requirement: Issue edits and execution start are distinct reviewed commands
The Console SHALL support preconditioned revision editing and a separate pipeline execution action. The start form SHALL select visible authorized targets, show the exact reviewed requirements revision, input mapping and resolved configuration/repository choices, and explain missing acceptance or execution prerequisites. Submission SHALL use a frozen confirmed intent and show pending input preparation/admission until the exact Work/Run link is returned. Transport retries SHALL preserve its original request. A later deliberate execution SHALL create a new request while node retry uses existing Run controls. Unsubmitted sensitive values and request keys SHALL remain in component memory only.

#### Scenario: Requirements change while execution runs
- **WHEN** an operator saves a new task revision
- **THEN** the card shows that revision and labels active execution as using the previous snapshot without restarting or modifying it

#### Scenario: Execution start loses its response
- **WHEN** a confirmed request's transport acknowledgement is lost
- **THEN** the Console resumes the same request/status receipt and opens its original Run without a fresh command identity

#### Scenario: Reviewed version is stale
- **WHEN** the server rejects an edit or start request after another revision committed
- **THEN** the Console preserves the in-memory review context, shows the conflict and requires fresh review instead of silently substituting content

### Requirement: Source ownership and synchronization conflicts are understandable
Linked task views SHALL show provider/resource display context, source link, supported managed fields/directions, last successful observation, pending updates and safe blocked/conflict/exhaustion reasons. Source-owned fields SHALL be visibly read-only with an explanation. Explicit attachment of a manual task SHALL use authorized existing connection/resource choices and ownership review; conflicts SHALL not merge task histories. Mapping and connection setup SHALL link to their existing management surfaces without asking for tokens in the task. Resolution SHALL show exact observed/desired values and submit only currently authorized preconditioned choices.

#### Scenario: Triage succeeded but source update is pending
- **WHEN** the external provider is unavailable after an accepted Triage result
- **THEN** the card separately shows completed execution and pending synchronization without presenting the check as failed

#### Scenario: Source has changed a managed field concurrently
- **WHEN** the server exposes a synchronization conflict
- **THEN** the UI shows its managed-field scope and supported resolution rather than an unconditional overwrite/retry action

#### Scenario: Installed adapter lacks a requested operation
- **WHEN** the operator configures an unsupported field direction or absent provider
- **THEN** the UI explains the unavailable capability and does not imply that generic integration metadata implements it

### Requirement: Activity discussion does not imply agent chat
Activity SHALL combine bounded safe execution/synchronization event references and operator-only plain-text discussion with actual server attribution and timestamps. Comments/replies SHALL use generated limits, separate append feedback and scoped replay; agent mentions or approval-like text SHALL remain non-executable discussion. Human clarification SHALL open a separate typed form for an exact declared gate. Bodies/response values SHALL not enter browser persistence, URLs, analytics or generic notifications, and unfinished drafts SHALL be discarded on leave/reload.

#### Scenario: Operator writes an agent mention
- **WHEN** a comment asks an agent to start work or approve a result
- **THEN** it appears as operator discussion with no execution/control success indicator

#### Scenario: Comment acknowledgement is lost
- **WHEN** the active append is retried
- **THEN** it uses the same frozen body/parent/key, preserves pending feedback and creates no duplicate comment

### Requirement: Results and operator actions retain exact execution authority
The card SHALL render exact server-published execution, evidence and Decision facts with non-color labels, distinguishing pending/failed/missing/indeterminate/superseded/accepted outcomes. Issue metadata, model text and previous-execution success SHALL not establish current acceptance. Shared Project/global and Issue-scoped action views SHALL use server-classified exact targets/contracts and existing guarded commands with Run preconditions. Recorded intent, owner processing and completed action SHALL remain distinct; the view SHALL not collapse multiple executions into one approve-task action or change the notification-center contract.

#### Scenario: Required verification has not passed
- **WHEN** an implementation Build succeeds but a required current check is missing or failed
- **THEN** the card shows technical completion and the unresolved check without declaring accepted work

#### Scenario: Another operator answered a gate
- **WHEN** a confirmed response receives a stale/precondition conflict
- **THEN** the UI refreshes its exact target and requires new review without upgrading or retargeting the intent

#### Scenario: Response is recorded before owner consumption
- **WHEN** the existing control returns an accepted intent receipt
- **THEN** the item is no longer unanswered and the card shows awaiting owner processing rather than node completion

### Requirement: Task views preserve bounded access and legacy journeys
Every task/card section SHALL use bounded REST queries with explicit loading/empty/stale/forbidden/not-found/conflict/unavailable states and independent observation coverage. Refresh failure SHALL retain visible stale data rather than invent success or an empty queue. Polling SHALL stop or back off when inactive/terminal as appropriate. Existing Run URLs, Artifact-based intake, Factory authoring and ordinary CI/CD SHALL remain usable. Supported keyboard/focus, English/Russian localization and narrow layouts SHALL cover create/edit/start/source-review/card/action journeys.

#### Scenario: Card refresh partially fails
- **WHEN** history, synchronization or results cannot refresh
- **THEN** the affected section is visibly stale/incomplete and unsafe confirmation is disabled until fresh review

#### Scenario: Operator follows an old Run URL
- **WHEN** its Issue association is available or still awaiting backfill
- **THEN** the original diagnostics remain reachable and an available Issue link is additive rather than required

### Requirement: Card layout and optional properties form one task workspace
The desktop card SHALL have a stable human-key/title/status/start header, main description/acceptance and Activity area, and right-side Properties/Execution/source/review summaries within the existing shell. Overview SHALL be the default with an Activity preview; Activity, Runs, Results and Actions SHALL provide focused deeper views without requiring advanced configuration pages. Properties SHALL include priority, organizational assignee, configurable type/component/size, Project labels, due date and supported simple custom fields, plus parent/child navigation. Safe metadata details/copy controls SHALL expose technical identifiers progressively. Server-authorized choices, source ownership and current-version review SHALL drive editing. Unavailable identity directories SHALL leave Unassigned and all manual/create/edit/start journeys usable; assignment SHALL remain distinct from AI profile/worker and SHALL start no execution. Narrow layouts SHALL stack content with an accessible Properties trigger and no horizontal page overflow, preserving status/start controls and selected historical subject. English/Russian component/browser fixtures SHALL cover empty, manual, preparing, queued, parallel, active, waiting, failed, completed and partially stale/restricted states.

#### Scenario: Operator opens an unconfigured manual task
- **WHEN** the task has no integration, Factory target, assigned person or Run
- **THEN** Overview shows editable requirements and useful optional properties, empty execution/results explanations and a separate setup/start entry point without opaque ID forms

#### Scenario: Operator edits responsibility and planning values
- **WHEN** it saves supported assignee/labels/component/size/date choices under the current version
- **THEN** the card shows the retained planning revision without starting an agent, moving status or modifying the inputs of an active Run

#### Scenario: Keyboard user opens the card at narrow width
- **WHEN** the user opens Properties, navigates sections and follows a parent/child or historical execution link
- **THEN** focus, selected subject and all available actions remain understandable without horizontal scrolling or hidden desktop-only controls

### Requirement: Run inspection shows detailed qualified activity and usable log navigation
Runs SHALL present an execution selector, stage/Job hierarchy and bounded event/log detail with safe lifecycle/tool/file-operation rows actually reported by the execution backend. Rows SHALL expose available original times/durations, measured usage, outcome/errors, profile/model/worker facts and authorized detail/log links; Unknown and Not reported SHALL not look like zero usage or a successful operation. Expansion SHALL fetch only the selected authorized bounded content and preserve redaction/privacy. Live following SHALL resume the same cursor/stream, preserve user scroll/history selection, mark gaps/stale/expired content and stop/back off when inactive or terminal. Retry/cancel/respond controls SHALL retain their exact Run/attempt authority; inspecting a row SHALL cause no tool, provider write or execution.

#### Scenario: Operator inspects a failed command
- **WHEN** its qualified execution reports a failed tool operation under one Job
- **THEN** the detail shows safe operation/result/timing and opens the exact authorized log location while other stage attempts remain distinct

#### Scenario: User revisits a historical run while another is active
- **WHEN** the URL selects the older execution and live progress exists elsewhere on the card
- **THEN** its journal/results stay historical through refresh and no current control or new log tail is silently substituted

#### Scenario: Connection to live logs is interrupted
- **WHEN** journal following resumes or a retained range is unavailable
- **THEN** rows are not duplicated, missing coverage is explicit and the UI preserves scroll/selection instead of claiming a complete empty log

### Requirement: Review checks and artifact panels expose complete subject-bound results
Results SHALL provide separate PR/MR, Checks and Artifacts panels plus compact summary links from the card. PR/MR cards SHALL show supported repository/number/title/link, visible author, draft/state, head/base, change counts, review/check summaries and observation freshness, with explicit unknown/stale/unsupported states. Checks SHALL distinguish Factory evidence from external CI, include missing required checks and exact execution/candidate/head binding, and preserve historical results after a new head. Artifacts SHALL group authorized original inputs, outputs, reports and screenshots with names/types/sizes/retention and progressive provenance, bounded safe preview and explicit download. Hidden/protected content SHALL not leak through lists/previews; expired content SHALL remain visibly unavailable. Provider failures SHALL not disable local task creation or rewrite accepted results, and no review/CI/merge badge SHALL substitute for Factory acceptance. Attachment SHALL use authorized reviewed choices and no credentials or arbitrary endpoint fields.

#### Scenario: Review head changes and CI was green for the old head
- **WHEN** the card receives the newer head observation
- **THEN** it labels previous checks historical, exposes current coverage/missing checks and preserves the original execution/candidate association

#### Scenario: Provider becomes unavailable
- **WHEN** PR/MR refresh cannot obtain a fresh authorized observation
- **THEN** the last known review remains visibly stale/blocked while local properties, execution history and independently authorized artifacts remain usable

#### Scenario: Operator previews a screenshot and opens a report
- **WHEN** the selected execution has visible raster output and an authorized report
- **THEN** previews/details are fetched on demand through their existing readers, downloads are explicit and an expired or restricted item has an honest unavailable state

### Requirement: Project AI agent and profile authoring
The Console SHALL provide Project-scoped discovery, creation, validation, and immutable-version replacement of arbitrary Factory Roles and multiple Role Profiles per role through an **AI agents** catalog (`AI-агенты` in Russian). One AI-agent card SHALL represent one Factory Role using its stable identity and SHALL contain its Role Profiles. Name, uploaded raster avatar, description, shared versioned skill folders, ordered shared instruction entries and agent-wide concurrency limit SHALL be edited as role content. Runtime, Model, Thinking level, additional ordered instructions, environment variables, CLI argument lists, MCP servers, external integration bindings, tools and resource ceilings SHALL be edited as profile content through the existing role/profile REST contracts and authorized choices. Secret variables SHALL use logical references without revealing stored credentials; runtime-specific unsupported settings SHALL be identified rather than silently ignored. For a selected exact profile, the card SHALL show its pinned shared and additional instruction layers and assigned skills from authorized role/profile details and normalized validation data. Unresolved or incompatible settings SHALL be clearly identified. The AI-agent catalog SHALL be separate from enrolled CI/CD Agent/Agent Pool management and SHALL NOT introduce another agent registry, storage model, or worker enrollment flow. It SHALL offer copy-on-create coder/default/pro, reviewer/default, and architect/default starter templates and allow independent naming and configuration of their copies. Authoring SHALL preserve the existing in-memory-only draft, reviewed immutable intent, idempotency, current-version precondition, localization, accessibility, and safe error contracts.

#### Scenario: Operator distinguishes AI agents from CI/CD workers
- **WHEN** a Project contains a Factory Role with two profiles and the deployment has enrolled worker Agents
- **THEN** its AI-agent catalog shows one role-backed agent card with two profiles, while worker Agent/Pool capacity and enrollment remain in their existing Console area

#### Scenario: Operator configures default and pro coding profiles
- **WHEN** an operator copies the coder template into a Project
- **THEN** the editor presents two independently configurable profiles and does not imply that pro grants extra permissions or that updating the template will update the copies

#### Scenario: Operator creates a custom specialist
- **WHEN** an operator creates a custom AI agent, enters shared instructions, and configures profile instructions using visible installed choices and shared skills on the AI agent
- **THEN** the Console validates and publishes ordinary role/profile definitions through REST without a second AI-agent identity, fixed role category, or hand-entered opaque reference

#### Scenario: Profile changes while being edited
- **WHEN** another operator publishes a replacement before the current draft is confirmed
- **THEN** the Console preserves the in-memory draft, reports the conflict, and requires explicit review against the new version instead of silently resubmitting

### Requirement: Skill folder import preserves the AI-agent editor draft
The Console SHALL offer import of a local skill folder beside the shared skill list without closing the AI-agent editor. The picker SHALL accept either one skill folder or a collection of skill folders. The outermost directories containing case-sensitive `SKILL.md` SHALL be reviewed and imported independently through the same existing contract, preserving their complete supporting subtrees and excluding collection files outside those directories. Default identifiers SHALL use lowercase folder names; editable identifiers SHALL satisfy the exported import key constraints and be distinct within the pending selection before confirmation. The operator SHALL select the folder, review its `SKILL.md` and supporting relative-path file set, and explicitly import it through the narrow authorized Project skill-import contract. The Console SHALL add the resulting exact bundle/version to the selected skills of the current AI-agent draft and SHALL preserve other in-memory draft fields on import success, cancellation, failure or a lost-response retry. Updating a local folder SHALL require explicit reimport, which selects the new version only in the current draft; it SHALL NOT silently modify a saved role or profile. Skill bytes, environment values, instruction content and private connection data SHALL NOT be copied to URLs, browser persistence or analytics. The import control SHALL follow existing localization, keyboard/focus and narrow-layout conventions.

#### Scenario: Operator imports a folder while editing an AI agent
- **WHEN** an operator has entered agent instructions and selected other skills and imports a folder containing `SKILL.md` and supporting files
- **THEN** the saved immutable bundle is selected in the same open draft, other selected skills and entered fields remain intact, and publishing the agent explicitly pins the shared skill version

#### Scenario: Operator selects a collection of skill folders
- **WHEN** the selected collection contains `Alpha/SKILL.md`, supporting files and `group/Beta/SKILL.md`
- **THEN** Console offers independent reviewed imports with editable default identifiers `alpha` and `beta`, retains each skill’s supporting subtree, and preserves successful draft bindings when a later import is cancelled or retried

#### Scenario: Folder import fails or is cancelled
- **WHEN** the operator cancels the folder picker or the server rejects an unsafe/oversized/incomplete folder
- **THEN** the existing AI-agent draft and selected skills remain intact and no successful import or profile publication is claimed

### Requirement: Stage selection and recovery policy editing
The Factory stage editor SHALL support `manual`, criteria-based `rules`, and `decision_api` selection over the selected AI agent's role-bound pinned compatible profiles. Selectors SHALL identify the AI agent and profile using the same exact role/profile references. It SHALL expose ordered typed criteria for rules mode and semantic criteria, installed provider choice, qualification/rollout status, and applicable thresholds for Decision API mode. Every stage SHALL explicitly choose either an exact reserve profile or manual selection on failure. The editor SHALL show candidate compatibility and the total bounded selection/execution requirements from server validation and SHALL NOT infer authority, invent unsupported provider/model parameters, or evaluate business rules in the browser.

#### Scenario: Stage uses rules and a reserve
- **WHEN** an operator configures high complexity to select coder-pro and chooses coder as reserve
- **THEN** the reviewed configuration includes exact candidate, criterion, order, and reserve versions before publication

#### Scenario: Stage delegates profile choice to JEV
- **WHEN** an operator selects a qualified JEV profile-selection capability
- **THEN** the editor shows the bounded candidates, supplied criteria, rollout semantics, and stage-specific failure behavior while distinguishing profile selection from routing configuration

### Requirement: Manual profile choices and selection history
The Console SHALL support optional explicit stage profile overrides during Work admission and eligible pre-dispatch selection through REST. A stage awaiting manual selection SHALL show its exact target, safe failure reason if present, eligible pinned profiles, current version, and deadline; submission SHALL require a target-specific reviewed intent with duplicate prevention. History SHALL distinguish automatic recommendations, manual/reserve choices, accepted intents, committed dispatch, and execution results. A selection SHALL NOT be displayed as stage success or candidate acceptance. Existing static Run views SHALL remain usable without role setup.

#### Scenario: Operator overrides automatic choice for one stage
- **WHEN** an operator selects coder-pro for a stage in the admission form
- **THEN** confirmation names that stage and exact profile, while other stages retain their configured selection policies

#### Scenario: Operator responds after selection failure
- **WHEN** a stage reports a current pending manual choice
- **THEN** the Console offers its authorized pinned candidates, submits one preconditioned selection intent, and waits for authoritative owner/dispatch state before showing execution

#### Scenario: Choice becomes stale while the form is open
- **WHEN** dispatch, cancellation, expiry, or another accepted choice invalidates the displayed target
- **THEN** confirmation is blocked or returns the server's conflict, and the Console requires refresh and review rather than silently choosing a new target/profile

#### Scenario: Selection history contains sensitive context
- **WHEN** the stage's criteria or skill package reference protected content
- **THEN** the Console displays safe policy, rule, profile, and receipt metadata without copying raw content into history, URLs, browser storage, analytics, or errors

### Requirement: Plugin model choices and manual model entry
Console SHALL offer registered runtimes, plugin-owned model suggestions, manual model identifier entry and plugin-declared parameter/thinking settings independently for each ordinary named profile. An explicit refresh control SHALL preserve the current draft; discovery unavailability SHALL explain manual entry without fabricating choices. No provider-specific model/thinking list SHALL be hardcoded in Console. Identity, avatar, instructions and shared-skill authoring SHALL remain usable without discovery. Skill browsing SHALL preserve existing retained inventory and inert previews.

#### Scenario: Operator binds two profiles to different models
- **WHEN** an operator edits default and pro and enters an unlisted model for one profile
- **THEN** each profile retains its own selected model and parameters, and refreshing suggestions neither clears the draft nor changes the saved sibling profile

### Requirement: AI-agent deletion from its card
The Console SHALL offer Delete AI agent in the AI-agent card through the existing confirmed-command and generated REST mechanisms. Confirmation SHALL explain that profiles become unavailable for new selection while Run history remains retained. Cancellation and definitive failure SHALL leave the card intact; transport retry SHALL retain the original key and displayed version. Success SHALL refresh current catalogs, close the removed card and restore keyboard focus to Create AI agent. The action SHALL be localized and independent of CI/CD enrollment and provider configuration.

#### Scenario: Review and delete an AI agent
- **WHEN** an operator confirms deletion of the displayed AI agent
- **THEN** the original version/key is submitted, the removed agent no longer appears in current choices, retained history is unchanged and keyboard focus returns to the catalog control

### Requirement: Runtime settings are separate from AI-agent profiles
Console SHALL provide a separate AI settings section containing AI agents and Runtimes views under the selected Project context. The AI agents view SHALL reuse the existing Role-backed catalog/editor, with profiles inside each agent. CI/CD Agent and Agent Pool management SHALL remain in its existing separate area. The Runtimes view SHALL configure existing installed runtimes without another agent or Runtime registry. Existing Factory entry points SHALL navigate to the same AI-agent views rather than maintain duplicate editors. Authorized administrators SHALL be able to add/edit/remove supported named MCP entries, select existing authorized integration resources and configure supported finite permission choices through typed REST contracts. The editor SHALL explain MCP transport, integration dependency and permission effects, expose applicable installed limits and direct missing integration access to the existing administration flow. It SHALL use human-readable server choices rather than requiring opaque reference IDs, digests, provider credentials or a raw JSON document. Publication SHALL reuse reviewed immutable intent, scoped retry, current-version conflicts and in-memory draft handling, with English/Russian labels and keyboard/narrow-layout support. Saving SHALL start no Build, MCP process or model request.

#### Scenario: Operator opens AI settings
- **WHEN** an operator opens AI settings and selects a Project
- **THEN** the section offers the existing AI-agent cards and their profiles plus Runtime settings, while worker enrollment and pools remain in their separate area

#### Scenario: Administrator prepares Runtime choices
- **WHEN** an administrator opens a visible Runtime, adds a supported named MCP server, selects a permitted integration and reviews permission choices
- **THEN** Console publishes one reviewed settings revision and makes the resulting exact choices available to profile editors without editing AI-agent identity or sibling profiles

#### Scenario: Settings conflict while a draft is open
- **WHEN** another administrator replaces the displayed Runtime revision before confirmation
- **THEN** Console retains the draft, explains the conflict and requires explicit refresh/review without silently overwriting settings or retrying with a newer version

### Requirement: Profiles explain unavailable Runtime choices and preserve drafts during setup
Profile editors SHALL select MCP, integration and permission entries from the server-qualified Runtime configuration and explain required MCP integration bindings. Empty categories SHALL show a concise explanation and a Runtime setup entry point instead of empty selection frames; a deny-all permission set SHALL not appear as editable zero-only controls. Absence, unsupported capability and authorization failure SHALL remain distinguishable. Opening and returning from Runtime setup SHALL preserve the unfinished profile draft in component memory without publishing it or changing saved profiles. A newly configured choice SHALL be adopted only by explicit selection; Runtime refresh SHALL not reset other draft fields or silently replace stale bindings.

#### Scenario: Runtime has no optional choices yet
- **WHEN** an operator edits a profile for an unconfigured Runtime
- **THEN** the editor explains what is missing and where to configure it, preserves ordinary profile/model authoring and shows no empty MCP, integration or permission groups

#### Scenario: Operator returns from Runtime setup
- **WHEN** an operator prepares a new Runtime choice while a profile draft contains unsaved name, model and instructions
- **THEN** returning preserves those fields, refreshes available choices and allows explicit selection through the same profile validation/publication contract

### Requirement: Automatic-intake authoring works against the released runtime
The existing pipeline-input Console journey SHALL use the deployed protected API to show current authorized connections, compatible Source/preparation choices, independent code Repository/target, predicates, optional Reporter mappings, preview and confirmed publication. Input activation SHALL be explicit and independent of Project Issue import and synchronization. The form SHALL distinguish missing connection/grant/runtime/profile, incompatible configuration, stale version, provider throttling and transport failure with localized actionable safe guidance. Generic safe-failure text alone, opaque identity entry or arbitrary defaults SHALL NOT conceal missing installed support. Manual tasks and intake SHALL remain accessible when automatic intake is absent or fails.

#### Scenario: Operator configures a source from an existing integration
- **WHEN** an operator with compatible installation and current Project access opens automatic execution input settings
- **THEN** authorized choices, preview and confirmed publication work without token/private-file entry, with actual input progress and independent Issue/Work/Run links when present

#### Scenario: Runtime profile is not installed
- **WHEN** a connection exists but lacks a compatible consumer/preparation runtime profile
- **THEN** the form explains the missing capability and deployment action without presenting a successful empty source scan or requiring unrelated task/workflow setup

#### Scenario: Operator retries after server recovery
- **WHEN** a failed input query succeeds after runtime recovery
- **THEN** retry replaces the old failure with actual data/empty state without another credential submission or duplicate publication

### Requirement: Project tasks expose import and separate status synchronization
The Console SHALL offer authorized Project Source discovery, bounded preview and reviewed import controls before an Issue or Factory Configuration exists. A successfully imported native task SHALL appear through the existing Issue list/card with its primary source and current synchronization state. The completed workflow canvas and inspector SHALL remain the business-status editor with existing validation, frozen publication, conflict review and separate default selection. Native field/status translation SHALL remain a separately reviewed supported source mapping against the Issue's exact workflow pin, explicit ownership and current read/write grants. Import, field refresh, business status, execution and delivery SHALL be presented separately; neither connection setup, workflow publication nor import SHALL imply a Run or native mutation. Controls SHALL retain English/Russian, keyboard/narrow-width access, safe errors and transient draft behavior.

#### Scenario: Operator imports a task before configuring a pipeline
- **WHEN** the operator previews an authorized native resource and confirms Project import without a Factory target or writer
- **THEN** a canonical Issue becomes visible with ordinary initial status and source correlation, and the UI shows no execution or completed remote write

#### Scenario: Operator maps an existing business status
- **WHEN** a linked Issue uses a published workflow and the operator reviews a supported OctaCity-owned push mapping
- **THEN** native states/labels and granted directions are reviewed separately from the workflow canvas and a permitted explicit status move exposes its actual outbound projection

#### Scenario: Workflow default changes during source mapping review
- **WHEN** a different workflow/default is published while the operator edits a linked Issue mapping
- **THEN** the Issue retains its pinned workflow until explicit adoption and stale/incompatible mapping review cannot silently switch it to the latest default

#### Scenario: Accepted status has deferred reporting
- **WHEN** a permitted local or pipeline business transition is retained but its native update cannot yet be delivered
- **THEN** the card keeps the accepted local status and actual execution results and shows pending/conflicting/exhausted synchronization rather than claiming remote success or rolling back the status

### Requirement: Runtime setup leads clearly to profile model and reasoning selection
The AI settings Runtime view SHALL explain that it prepares shared MCP/integration choices and finite permissions, while each AI-agent profile owns its model, plugin-supported reasoning parameters and budget. It SHALL provide a direct keyboard-accessible handoff to the existing AI-agent/profile authoring view under the same Project, with guidance for choosing or creating a profile. Navigation between Runtime setup and profiles SHALL preserve the existing in-memory profile draft and SHALL NOT publish it, select another model or create an AI agent/profile automatically. English and Russian labels and narrow layouts SHALL use existing Console navigation and form components.

#### Scenario: Operator wants to select a model from Runtime settings
- **WHEN** the operator follows the model/reasoning setup action from a Runtime under a selected Project
- **THEN** Console opens that Project's existing AI-agent/profile journey and explains where model and plugin-declared reasoning parameters are edited without another model editor or losing the unfinished profile draft

### Requirement: Runtime setup explains installation prerequisites and validation outcomes
Runtime settings SHALL distinguish an absent installation capability, an unsupported transport, missing executable/network authority and an unconfigured Project choice. Empty permission states SHALL identify the installation prerequisite and its supported setup path rather than telling the operator to configure unavailable controls on the same screen. HTTP/stdio entry and permission controls SHALL reflect qualified server choices. Configuration validation/publication feedback SHALL state its scope and SHALL NOT imply that an MCP connection was tested or that execution succeeded. Existing integration setup SHALL remain a separate optional flow rather than a prerequisite for every MCP server.

Existing Runtime/Profile editors SHALL expose safe per-connection credential-reference and enforced access-scope metadata, distinguishing MCP authentication from AI-provider login and server Source/Reporter authority. Token values SHALL NOT be accepted as Runtime/profile content or exposed in diagnostics. Missing qualified AI-plugin transport/authentication, read-only/resource enforcement or required tool-evidence capture SHALL have actionable unsupported or installation reasons. Compatible ready-made MCP choices SHALL NOT require a provider-specific editor, wrapper or unrelated Source connection.

#### Scenario: Stand provides model discovery but no execution policy
- **WHEN** an operator opens Runtime settings on the existing metadata-only installation
- **THEN** the view keeps model/profile authoring reachable, explains the explicit local installation prerequisite and does not present empty permission controls as configurable execution authority

#### Scenario: Operator reviews settings without starting execution
- **WHEN** reviewed Runtime settings pass validation or publication
- **THEN** the outcome confirms configuration only and does not claim an MCP process or remote connectivity check occurred

#### Scenario: An MCP connection lacks qualified authentication or evidence support
- **WHEN** an operator selects a connection that the installed AI plugin cannot authenticate, restrict or observe as required
- **THEN** the editor identifies the missing capability using safe connection metadata without requesting a token in profile content or claiming executable readiness
