## ADDED Requirements

### Requirement: Project preset resolution returns ordinary reviewed drafts
The management API SHALL expose one bounded Project-visible preset discovery/resolution contract shared by supported Factory presets. The triage preset SHALL advertise its version, typed parameter choices and required installed capabilities and SHALL resolve exact authorized Role/Profile, repository, context, input and effect selections into ordinary Flow/Configuration drafts. Resolution SHALL be read-only and SHALL NOT publish a role, input, credential, grant, managed document or Run. Unavailable dependencies SHALL produce actionable safe reasons without exposing hidden references or private installation paths. Ordinary validation and publication SHALL recheck current authority and exact parents with existing mutation preconditions and idempotency.

Selected MCP connections SHALL use existing Runtime/Profile settings and safe credential-reference metadata, with separate access scopes and no secret values. Resolution SHALL NOT call an MCP server or inspect secret values. Model-driven duplicate investigation SHALL use MCP tools; this preset SHALL introduce no duplicate-search REST API or provider-specific external-MCP wrapper contract.

#### Scenario: A headless client prepares triage
- **WHEN** an authorized client selects visible exact dependencies through preset discovery and resolution
- **THEN** it can validate and publish the returned ordinary drafts using the same publication contracts as Console without a separate triage lifecycle API

#### Scenario: A dependency changes after resolution
- **WHEN** a selected Profile, grant, workflow or effect scope is no longer valid before publication or activation
- **THEN** the command fails with the applicable precondition/capability error and does not silently rebind or enable a partial setup

### Requirement: Configuration readiness distinguishes execution from reporting claims
An exact Configuration/Repository readiness query SHALL expose bounded visible observations for the required executable profiles, qualified Agent/enforcement, input/context dependencies and configured Source/Reporter scope, with observation freshness and actionable blocked/unknown reasons. Missing or stale required execution facts SHALL prevent executable-ready claims. Successful read authentication SHALL NOT establish native write permission; internally configured reporting and unverified external write permission SHALL be distinguishable. The query SHALL perform no provider/model request, secret-value access, publication, execution, budget or lease reservation and SHALL NOT confer future dispatch authority.

Required observations SHALL include OctaCity MCP availability and scoped authentication, the selected AI plugin's qualified native MCP transport/authentication and evidence-capture support, and per-connection secret-reference/read-only scope compatibility. AI-provider authentication, MCP read authority and server Reporter write authority SHALL remain separate facts. Missing, stale or unsupported required MCP facts SHALL block executable-ready claims without making a live MCP request during the query.

#### Scenario: Only metadata authoring is installed
- **WHEN** a triage configuration references a visible Runtime with no compatible executable Agent path
- **THEN** readiness reports that execution prerequisite as blocked or unknown rather than using server health or model suggestions as proof of readiness

#### Scenario: Read access works but native write permission is unverified
- **WHEN** current installed update scope and Project grants exist but the provider has not confirmed a write
- **THEN** the report preserves that unverified reporting fact independently from executable readiness and never labels delivery confirmed

#### Scenario: MCP settings exist without usable scoped execution
- **WHEN** a selected connection lacks qualified plugin authentication, read-only/resource scope or required result capture
- **THEN** readiness reports the specific blocked or unknown MCP prerequisite using safe metadata without resolving tokens, calling tools or borrowing Reporter authority

### Requirement: Triage results use ordinary authorized execution reads
Management schemas and generated clients SHALL expose triage's retained schema-validated results, accepted outcome/evidence references and original Work/Run/optional canonical Issue correlation through ordinary execution reads. Decision facts, assessment text, reproduction evidence, local status and Reporter receipts SHALL remain distinguishable. Reads SHALL be bounded and visibility-filtered and SHALL NOT invoke models, refresh a provider or mutate planning/status fields.

#### Scenario: A user inspects a completed classification
- **WHEN** an authorized client opens the retained Run or associated Issue result
- **THEN** it can distinguish classified priority/component/size, verified versus unresolved reproduction and pending versus applied GitHub effects with exact originating stage references
