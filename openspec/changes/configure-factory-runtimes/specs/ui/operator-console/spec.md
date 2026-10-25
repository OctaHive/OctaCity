## ADDED Requirements

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
