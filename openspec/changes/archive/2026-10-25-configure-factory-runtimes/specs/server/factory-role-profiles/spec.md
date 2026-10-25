## ADDED Requirements

### Requirement: Project administrators configure installed Runtime choices
The server SHALL provide Project-scoped authorized reads and reviewed publication of Runtime settings bound to an existing installed Runtime identity. Settings SHALL define named supported MCP transports, exact existing Project-authorized integration bindings and finite permission choices through the existing execution-setting and permission contracts. Installation-owned executable/plugin identities, supported settings and independent backend/Agent ceilings SHALL remain authoritative. Configuration SHALL NOT install arbitrary executables, create another Runtime or connection registry, grant integration access, accept credential values or silently widen execution authority. Supported MCP transports SHALL use structured endpoint or executable/argv fields and explicit environment names without shell interpretation or ambient inheritance. Invalid, unsupported, foreign or unauthorized selections SHALL fail safely before publication; configuration and reads SHALL perform no model execution or remote MCP/provider operation.

#### Scenario: Administrator configures a Runtime for profile authoring
- **WHEN** a Project administrator publishes a valid named MCP server, existing authorized integration and supported permission choices for an installed Runtime
- **THEN** that Project discovers those choices through its ordinary profile capability contract without changing another Project or installing a second Runtime

#### Scenario: Configuration exceeds installed authority
- **WHEN** a request replaces a protected executable, includes a credential value or requests unsupported settings or authority outside installation/backend constraints
- **THEN** the server rejects it without partially publishing the settings or silently weakening the requested constraint

### Requirement: Runtime settings retain reviewed revisions and mutation outcomes
Runtime settings and referenced MCP transport content SHALL have immutable retained revisions and verified exact identities. Replacement SHALL require the displayed current-version precondition. Publication SHALL atomically retain its original scoped mutation result, audit and outbox facts through the existing management mechanisms. Reads and publication SHALL require current Project authorization, including replay. Concurrent replacements SHALL have at most one winner for one precondition, and a response-loss retry SHALL recover its original result after restart or later updates. Private transport content SHALL remain in authorized content reads rather than generic audit or discovery summaries.

#### Scenario: Publication response is lost and another update follows
- **WHEN** an accepted publication is retried with its original body and scoped key after restart or a later settings revision
- **THEN** the original receipt is returned without another version or duplicate audit/outbox effects, and a different body under that key conflicts

#### Scenario: Administrators edit the same settings revision
- **WHEN** two different reviewed replacements compete for the same current version
- **THEN** at most one is applied and the other receives a safe conflict while prior revisions remain readable under authorization

### Requirement: Runtime configuration feeds shared profile and compilation contracts
Profile discovery, details, validation and compilation SHALL resolve Runtime configuration through the same authoritative capability mechanism. Profiles SHALL select exact MCP and integration bindings rather than embedding transport definitions or inheriting future defaults. The existing compiler/materializer SHALL resolve the selected verified retained transport content, enforce the current integration grants and independent execution ceilings, and pass only explicitly selected settings to the plugin. Replacing Runtime defaults SHALL NOT rewrite published profiles or retained execution packages. Revocation or removal SHALL block newly unauthorized use without substituting another server, integration, model or profile. Unconfigured Runtime settings SHALL preserve existing installation behavior, static profiles and manual Factory intake.

#### Scenario: Profile uses a configured MCP server
- **WHEN** an authorized operator selects configured MCP and required integration bindings and the qualified ordinary execution is compiled
- **THEN** the plugin receives the exact selected transport/settings with only approved environment mappings and no unselected MCP server

#### Scenario: Settings change after an invocation is retained
- **WHEN** a Runtime setting is replaced before recovery of an earlier retained invocation
- **THEN** recovery resolves its original verified content rather than current defaults, while current authorization and revocation checks still apply
