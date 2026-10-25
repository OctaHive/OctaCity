## ADDED Requirements

### Requirement: Local Factory Runtime setup supplies usable bounded MCP execution
The local Factory stand SHALL provide an explicit documented setup path for operator-approved Runtime installation capabilities and independent worker policy using the existing installation contracts. After that setup, authorized operators SHALL be able to configure HTTP and stdio MCP servers and finite permissions in Console, select their exact bindings in an AI-agent profile and execute them through ordinary Builds in Microsandbox. Stdio choices SHALL identify verified installed guest executables with explicit argument and environment contracts; a host path or executable digest alone SHALL NOT imply guest availability. HTTP endpoints SHALL remain subject to the approved network and credential boundaries. Browser settings SHALL NOT install programs, widen installation/Agent/backend grants, import ambient workstation settings or trigger execution. Model names and reasoning choices SHALL remain supplied by plugins. Without explicit execution setup, the existing model-authoring/manual-intake path SHALL remain usable and execution prerequisites SHALL be reported honestly.

#### Scenario: Operator configures HTTP MCP
- **WHEN** the stand has independently approved network and execution policy and the operator publishes an allowed HTTP MCP and selects it in a profile
- **THEN** an explicitly started ordinary Build invokes the selected endpoint under those bounds without installing another Runtime or requiring an unrelated provider integration

#### Scenario: Operator configures stdio MCP
- **WHEN** the operator selects a qualified installed stdio program, supported arguments and mapped environment names in Runtime settings and then selects that exact binding in a profile
- **THEN** ordinary materialization makes the verified executable available at its declared guest path and executes it without a shell or ambient credentials

#### Scenario: Requested permission exceeds the installed policy
- **WHEN** Runtime or profile settings request an unapproved executable, network host or resource permission
- **THEN** validation or dispatch rejects the unsupported selection without replacing independent worker policy or silently enabling broader access

### Requirement: Local Runtime setup updates preserve operator state
Applying or replacing an explicitly selected Runtime installation SHALL reuse existing private-file verification, bounded staging and lifecycle mechanisms. Failed preflight SHALL leave the previous usable installation unchanged. Successful setup and ordinary restart SHALL retain PostgreSQL settings/receipts, exact transport and role/profile history, admitted execution content and worker identity without destructive reset or replaying demonstration Work. Installation changes SHALL NOT adopt new model, transport or permission selections into existing profiles automatically; stale or revoked pins SHALL follow the existing qualification and revocation rules.

#### Scenario: Operator enables execution on an existing stand
- **WHEN** valid explicit Runtime and worker installation inputs are applied to a stand containing saved AI agents, profiles and reviewed Runtime settings
- **THEN** the stand preserves its durable state, exposes the newly qualified choices and requires explicit adoption into future profile versions

#### Scenario: Installation preflight fails
- **WHEN** an executable identity, permission contract or worker setup is incompatible
- **THEN** setup reports the failed prerequisite before replacing or stopping the usable installation and does not reset the stand
