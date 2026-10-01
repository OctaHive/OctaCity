## Purpose

Defines the trusted-network browser console through which operators discover, observe, and safely control an OctaCity deployment without replacing its REST control plane.

## ADDED Requirements

### Requirement: Trusted same-origin console deployment
The operator console SHALL be a separately deployable static browser application served from the same trusted origin that proxies `/api/v1` to the private management listener. It SHALL use relative management URLs, SHALL NOT require or collect an operator credential, and SHALL identify the deployment as unauthenticated trusted-network access. The deployment contract SHALL NOT require cross-origin management access or expose the management listener directly to an untrusted network.

#### Scenario: Operator opens the console on the trusted network
- **WHEN** an operator opens the configured console origin
- **THEN** static application assets load from that origin, management requests use its `/api/v1` path, and the console visibly identifies the trusted-network security mode without presenting a login flow

#### Scenario: Deep link is refreshed
- **WHEN** an operator refreshes a console URL for a Project, Build, Agent, or Agent Pool
- **THEN** the static host returns the application shell while `/api/v1` remains routed only to the management API

#### Scenario: Cross-origin API configuration is absent
- **WHEN** the console is deployed according to the supported contract
- **THEN** it does not require permissive CORS, browser credentials, forwarded operator identity, or storage of an authentication token

### Requirement: URL-addressable operator navigation
The console SHALL provide stable, directly addressable views for the Project hierarchy, one Project, one Build, Agents, Agent Pools, and audit facts. A Project view SHALL discover its current Pipelines, Repositories, Build Configurations, Trigger definitions, and recent Builds through bounded REST collections rather than requiring an operator to enter opaque identifiers.

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
The console SHALL expose bounded Agent and Agent Pool collections, Agent inventory and state, pool assignment and version, drain state, and searchable audit facts using only the published management representations. Audit filtering SHALL support the server's actor, operation, target, request identity, and time-range filters without reconstructing authority from UI state.

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
