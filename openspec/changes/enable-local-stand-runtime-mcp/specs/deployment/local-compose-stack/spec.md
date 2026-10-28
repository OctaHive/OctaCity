## ADDED Requirements

### Requirement: Local Factory Runtime setup supplies usable bounded MCP execution
The local Factory stand SHALL provide an explicit documented setup path for operator-approved Runtime installation capabilities and independent worker policy using the existing installation contracts. After that setup, authorized operators SHALL be able to configure HTTP and stdio MCP servers and finite permissions in Console, select their exact bindings in an AI-agent profile and execute them through ordinary Builds in Microsandbox. Stdio choices SHALL identify verified installed guest executables with explicit argument and environment contracts; a host path or executable digest alone SHALL NOT imply guest availability. HTTP endpoints SHALL remain subject to the approved network and credential boundaries. Browser settings SHALL NOT install programs, widen installation/Agent/backend grants, import ambient workstation settings or trigger execution. Model names and reasoning choices SHALL remain supplied by plugins. Without explicit execution setup, the existing model-authoring/manual-intake path SHALL remain usable and execution prerequisites SHALL be reported honestly.

The selected AI plugin SHALL supply reviewed MCP connections to its Agent-side executor's native client in that executor's supported format. Compatible ready-made servers SHALL retain their native tools and schemas without an OctaCity provider wrapper or additional MCP client. Unselected and ambient workstation connections SHALL remain unavailable. Unsupported required plugin, transport or authentication capabilities SHALL block execution without a fallback server. The explicit installation path SHALL use the selected complete catalog and independent Agent policy without merging generated demo authority or creating demonstration Runs; ordinary no-selection startup SHALL retain its existing behavior.

#### Scenario: Operator configures HTTP MCP
- **WHEN** the stand has independently approved network and execution policy and the operator publishes an allowed HTTP MCP and selects it in a profile
- **THEN** an explicitly started ordinary Build invokes the selected endpoint under those bounds without installing another Runtime or requiring an unrelated provider integration

#### Scenario: Operator configures stdio MCP
- **WHEN** the operator selects a qualified installed stdio program, supported arguments and mapped environment names in Runtime settings and then selects that exact binding in a profile
- **THEN** ordinary materialization makes the verified executable available at its declared guest path and executes it without a shell or ambient credentials

#### Scenario: Requested permission exceeds the installed policy
- **WHEN** Runtime or profile settings request an unapproved executable, network host or resource permission
- **THEN** validation or dispatch rejects the unsupported selection without replacing independent worker policy or silently enabling broader access

#### Scenario: Operator supplies a complete execution installation
- **WHEN** the Factory stand command selects an operator-owned catalog, optional Runtime bundle and independently approved Agent policy
- **THEN** the ordinary installer uses those explicit inputs, rejects invalid selections before service activation and creates no extra demonstration Run or implicit provider connection

### Requirement: MCP connections retain independent credentials and enforced access
Each authenticated MCP connection SHALL select an authorized token/secret reference, credential purpose and resource/access scope through existing owners, separate from AI-provider login and server Source/Reporter authority. Secret values SHALL be supplied only to the intended client/server process or approved transport for the authorized execution and SHALL NOT appear in published settings, prompts, Context, public results or diagnostics. Unrelated MCP child processes SHALL NOT inherit one another's credentials. Read-only/resource limits SHALL use actual server/tool and credential enforcement rather than model instructions or annotations alone. Missing or revoked local authority and unsupported authentication SHALL block newly unauthorized use without ambient or other-connection fallback; remote revocation SHALL follow the selected server's enforcement contract. Server Source intake and Reporter delivery SHALL remain independent of agent MCP investigation.

#### Scenario: Two MCP connections use different read credentials
- **WHEN** one execution selects HTTP and stdio MCP connections with distinct approved credential references and resource scopes
- **THEN** each connection receives only its intended credentials, reads only permitted resources and cannot invoke a write using AI-provider or Reporter authority

#### Scenario: A connection loses authorization
- **WHEN** its local credential/connection authority is revoked or its required authentication method is unsupported
- **THEN** newly unauthorized execution is blocked with a safe reason and no fallback token, server or ambient configuration is selected

### Requirement: Native MCP execution produces bounded retained evidence
The qualified AI plugin/native-client path SHALL expose actual MCP calls and consumed results through ordinary invocation events/results and Artifact provenance, identifying connection/tool, arguments, observation time, content identity and available scope/coverage. Missing coverage SHALL remain unknown, and absent native revisions SHALL NOT be invented. Failed, truncated, unavailable and cancelled calls SHALL remain distinct from successful empty results. Execution SHALL obey configured deadlines and bounds, and cancellation SHALL stop or terminate the owned execution through its supported lifecycle rather than leave unbounded tool activity. Model assertions alone SHALL NOT establish tool execution. Missing required native capture or enforcement support SHALL be an explicit capability blocker. Evidence SHALL contain no secret values and SHALL NOT require a new tool journal or provider-specific response parser.

#### Scenario: The executor searches through a finite MCP peer
- **WHEN** a qualified native client performs permitted tool calls and a later call is cancelled or fails
- **THEN** ordinary retained evidence distinguishes consumed results and their exact invocation from the interrupted call without treating failure as empty search or exposing credentials

#### Scenario: A stopped execution has unknown terminal consumption
- **WHEN** a failed or cancelled native execution retains capture but cannot supply trustworthy terminal usage
- **THEN** its exact execution and budget reservation remain waiting without a semantic result or invented zero consumption, while other Runs can still reconcile and stop their owned executions
- **AND** later verified consumption can settle the original execution without dispatching another attempt

#### Scenario: A selected AI profile is cancelled during an MCP call
- **WHEN** the operator cancels an admitted native execution with an explicitly selected immutable AI profile
- **THEN** the owner validates the original selected intent and stops its already accepted ordinary Build without changing the profile, recreating a static intent or dispatching another attempt

### Requirement: Local Runtime setup updates preserve operator state
Applying or replacing an explicitly selected Runtime installation SHALL reuse existing private-file verification, bounded staging and lifecycle mechanisms. Failed preflight SHALL leave the previous usable installation unchanged. Successful setup and ordinary restart SHALL retain PostgreSQL settings/receipts, exact transport and role/profile history, admitted execution content and worker identity without destructive reset or replaying demonstration Work. Installation changes SHALL NOT adopt new model, transport or permission selections into existing profiles automatically; stale or revoked pins SHALL follow the existing qualification and revocation rules.

#### Scenario: Operator enables execution on an existing stand
- **WHEN** valid explicit Runtime and worker installation inputs are applied to a stand containing saved AI agents, profiles and reviewed Runtime settings
- **THEN** the stand preserves its durable state, exposes the newly qualified choices and requires explicit adoption into future profile versions

#### Scenario: Installation preflight fails
- **WHEN** an executable identity, permission contract or worker setup is incompatible
- **THEN** setup reports the failed prerequisite before replacing or stopping the usable installation and does not reset the stand
