## ADDED Requirements

### Requirement: Project preset resolution returns ordinary reviewed drafts
The management API SHALL expose one bounded Project-visible preset discovery/resolution contract shared by supported Factory presets. The triage preset SHALL advertise its version, typed parameter choices and required installed capabilities and SHALL resolve exact authorized Role/Profile, repository, context, input and effect selections into ordinary Flow/Configuration drafts. Resolution SHALL be read-only and SHALL NOT publish a role, input, credential, grant, managed document or Run. Unavailable dependencies SHALL produce actionable safe reasons without exposing hidden references or private installation paths. Ordinary validation and publication SHALL recheck current authority and exact parents with existing mutation preconditions and idempotency.

#### Scenario: A headless client prepares triage
- **WHEN** an authorized client selects visible exact dependencies through preset discovery and resolution
- **THEN** it can validate and publish the returned ordinary drafts using the same publication contracts as Console without a separate triage lifecycle API

#### Scenario: A dependency changes after resolution
- **WHEN** a selected Profile, grant, workflow or effect scope is no longer valid before publication or activation
- **THEN** the command fails with the applicable precondition/capability error and does not silently rebind or enable a partial setup

### Requirement: Configuration readiness distinguishes execution from reporting claims
An exact Configuration/Repository readiness query SHALL expose bounded visible observations for the required executable profiles, qualified Agent/enforcement, input/context dependencies and configured Source/Reporter scope, with observation freshness and actionable blocked/unknown reasons. Missing or stale required execution facts SHALL prevent executable-ready claims. Successful read authentication SHALL NOT establish native write permission; internally configured reporting and unverified external write permission SHALL be distinguishable. The query SHALL perform no provider/model request, secret-value access, publication, execution, budget or lease reservation and SHALL NOT confer future dispatch authority.

#### Scenario: Only metadata authoring is installed
- **WHEN** a triage configuration references a visible Runtime with no compatible executable Agent path
- **THEN** readiness reports that execution prerequisite as blocked or unknown rather than using server health or model suggestions as proof of readiness

#### Scenario: Read access works but native write permission is unverified
- **WHEN** current installed update scope and Project grants exist but the provider has not confirmed a write
- **THEN** the report preserves that unverified reporting fact independently from executable readiness and never labels delivery confirmed

### Requirement: Triage results use ordinary authorized execution reads
Management schemas and generated clients SHALL expose triage's retained schema-validated results, accepted outcome/evidence references and original Work/Run/optional canonical Issue correlation through ordinary execution reads. Decision facts, assessment text, reproduction evidence, local status and Reporter receipts SHALL remain distinguishable. Reads SHALL be bounded and visibility-filtered and SHALL NOT invoke models, refresh a provider or mutate planning/status fields.

#### Scenario: A user inspects a completed classification
- **WHEN** an authorized client opens the retained Run or associated Issue result
- **THEN** it can distinguish classified priority/component/size, verified versus unresolved reproduction and pending versus applied GitHub effects with exact originating stage references
