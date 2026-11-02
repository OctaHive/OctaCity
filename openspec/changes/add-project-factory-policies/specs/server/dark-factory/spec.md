## ADDED Requirements

### Requirement: Project launch policies and access profiles have durable contents
The server SHALL provide separately typed Project-owned launch policies and access profiles with bounded names, immutable versions and verified content digests. Authorized operators SHALL be able to create, read and replace their definitions through management contracts. Publication SHALL use current-version preconditions and atomically retain idempotency outcomes, audit and outbox facts. Project authorization SHALL apply to discovery, exact reads, writes and replay. Neither definition SHALL contain credential values or executable policy scripts.

#### Scenario: Policy replacement conflicts or loses its response
- **WHEN** operators compete to replace the same version, or retry an accepted publication after response loss and restart
- **THEN** only one replacement wins its precondition and the original scoped retry returns the same receipt without another version, while a changed body under the same key conflicts

#### Scenario: Another Project attempts to select a profile
- **WHEN** an operator supplies a launch or access reference owned by another Project without the required Project authorization
- **THEN** discovery does not expose it and publication/admission rejects its use without leaking its private contents

### Requirement: Launch rules govern the trusted admission origin
A launch policy SHALL explicitly allow or deny manual admission and automatic Source admission. The server SHALL derive origin from its trusted ingress, treating explicit Issue execution as manual regardless of the Issue's external association. It SHALL enforce the exact Configuration-pinned launch rules before admission-specific preparation and Work/Run/Build creation. Existing authorization, input validity, enabled Configuration and Source controls SHALL remain mandatory. Automatic permission SHALL NOT activate a Source or make discovery/preview an admission. Policy denial SHALL be distinguishable from a transient Source/provider error.

#### Scenario: Manual-only policy receives automatic work
- **WHEN** an enabled Source matches a task but its target Configuration pins a policy allowing only manual admission
- **THEN** automatic admission reports policy denial and creates no Work, Run or Build, while an otherwise authorized explicit manual admission remains permitted

#### Scenario: Explicit Issue execution is not an automatic-source bypass
- **WHEN** an operator requests execution of an imported Issue using a Configuration whose manual admission is denied
- **THEN** the server rejects it as manual policy denial even if the Issue has an authorized external Source link

#### Scenario: Source is disabled although automatic admission is allowed
- **WHEN** a policy permits automatic Source admission but that Source is disabled, unapproved or unauthorized
- **THEN** the policy alone cannot admit work, enable scanning or create a Run

#### Scenario: Accepted admission is retried after defaults change
- **WHEN** an accepted request is retried with its original scoped identity after a Project default or policy replacement
- **THEN** the authorized replay returns its original receipt and exact accepted policy references without new admission, source resolution or dispatch

### Requirement: Project access profiles narrow existing execution authority
Access profiles SHALL use the existing typed Factory permission vocabulary and SHALL be enforceable through the common Configuration/Flow/step/Profile compilation and protected-operation boundaries. They SHALL remain bounded by current installed capabilities, applicable Project/Runtime policy, independent Agent grants and backend enforcement. Publication or execution requiring broader, missing or unenforceable authority SHALL fail with safe limiting-scope diagnostics. Selecting an access profile SHALL NOT connect documents, repositories, MCP servers or secrets to an agent. Configuration WIP, Run budgets and business decisions SHALL retain their existing owners.

#### Scenario: Profile requests a host outside installed authority
- **WHEN** a Project access profile requests a network host or executable outside the current installed ceiling
- **THEN** publication rejects the requested authority with its limiting source and does not publish a partially clamped profile

#### Scenario: A narrower profile reaches dispatch
- **WHEN** a Configuration selects an access profile narrower than the installed ceiling
- **THEN** compilation and protected execution enforce the narrower profile and no sibling step, prompt or selected model can restore excluded authority

#### Scenario: Installation authority is narrowed after admission
- **WHEN** a retained invocation is retried after required installed authority is revoked or becomes unenforceable
- **THEN** the original profile content remains pinned and execution is blocked rather than adopting a newer default or weaker backend

### Requirement: Project Factory defaults are explicit and repeatable
Factory Project provisioning SHALL idempotently create named Standard launch and access records and retain exact default references. Standard launch rules SHALL initially permit manual and automatic admission subject to independent existing controls. Standard access SHALL derive from an explicitly designated reviewed installed default; absence SHALL produce no execution grants rather than unrestricted or first-listed authority. Existing-Project provisioning SHALL require an authorized mutation and SHALL NOT mutate state during GET. Reprovisioning SHALL preserve operator edits and defaults. New Configuration drafts SHALL receive exact defaults, while explicit overrides remain supported.

#### Scenario: Clean Project has a qualified installed default
- **WHEN** Factory settings are provisioned for a Project with an explicit qualified installed ceiling
- **THEN** inspectable Standard records and exact defaults exist without creating a Flow, enabling a Source or admitting Work

#### Scenario: Installation has no designated default
- **WHEN** a Project has no qualified designated installed access default, including an inventory with several undifferentiated ceilings
- **THEN** its Standard access profile has no execution grants and the server exposes an actionable configuration prerequisite without selecting by name or list order

#### Scenario: Stand restarts after operator edits
- **WHEN** the local stand is rebuilt or provisioning is retried after policy edits and default selection
- **THEN** it reuses the existing policy identities, versions and defaults without recreating triage-specific aliases or overwriting the edits

### Requirement: Policy version bindings preserve existing Configuration and Run history
New Configurations SHALL bind exact Project launch-policy and access-profile versions/digests, and accepted operations SHALL retain their resolved provenance. Later policy/default changes SHALL NOT alter published Configurations or accepted Runs. Legacy installation references SHALL resolve only through an explicit retained Project-scoped mapping to concrete verified policy content. Migration SHALL preserve original Configuration bytes/digests, Work/Run identities and accepted results. Unknown or unresolved legacy mappings SHALL block new admission with a specific reason rather than fall back to Standard. Existing accepted execution recovery SHALL preserve its original inputs and current authorization checks.

#### Scenario: Standard policy is replaced
- **WHEN** an operator publishes a new Standard version and explicitly changes the Project default
- **THEN** a new draft can select it but an existing Configuration and an already admitted Run retain their exact previous policy bindings

#### Scenario: Existing stand has legacy references
- **WHEN** a stand containing published Configurations, Runs and verified `admission_policy`/`permission_ceiling` references is upgraded
- **THEN** exact mappings expose concrete policy contents and preserve the saved Configuration digests, operator settings and execution history without resetting storage

#### Scenario: Legacy access contents cannot be recovered
- **WHEN** an existing access reference cannot be matched to exact verified installed declarations
- **THEN** new admission identifies the unresolved reference and never substitutes a similarly named or default profile
