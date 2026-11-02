## Why

Factory Configuration currently asks an operator to select installation aliases such as `triage-admission` and `triage-ceiling` without a Project editor for their contents. The admission alias has no editable rule body, while the access ceiling is supplied by installation metadata. Operators need real, inspectable Project settings with usable defaults instead of unexplained mandatory selections.

## What Changes

- Add Project-owned, immutable versioned **launch policies** and **access profiles**, with separate editors under Project settings → Factory.
- Limit launch rules initially to allowing manual admission and allowing automatic Source admission. The server determines the origin and enforces these rules; policy content cannot replace authorization or activate a Source.
- Reuse the existing typed Factory permission declarations for access profiles, constrained by installed capabilities and independent Server/Agent enforcement. Do not introduce another permission language or a second Runtime registry.
- Provide a named **Standard** launch policy and access profile through idempotent Project provisioning. New configuration drafts select the Project's exact defaults and show their contents and edit links. Access defaults require explicit installed authority; absence never becomes unrestricted access.
- Pin exact policy versions at Configuration publication and Work admission. Changing a policy or Project default does not silently repin published Configurations or Runs.
- Retain existing installations, published Configurations, immutable digests and accepted execution history. Resolve legacy aliases through an explicit, durable compatibility mapping with inspectable content; unresolved mappings block new admission with an actionable reason.
- Update `tools/factory-stand` to provision these ordinary Project settings without hardcoded business-specific policy identities or overwriting operator edits on restart.

## Capabilities

### New Capabilities

None. Extend the existing Factory lifecycle and Console specifications.

### Modified Capabilities

- `server/dark-factory`: Durable Project policy contents/defaults, exact bindings, admission-origin enforcement, permission narrowing and compatibility.
- `ui/operator-console`: Project policy editors and defaults, visible selectors, effective access summaries and draft-preserving navigation.

## Impact

Factory core types and validation; application authorization/admission/configuration handlers; backend-neutral store ports and PostgreSQL migrations; REST/OpenAPI and generated Console contracts; Project settings and Configuration authoring; local-stand provisioning and operation documentation.

No new worker execution path, integration credential owner, business-stage switch, custom rule scripting, queue scheduler, or duplicate Run-budget editor is introduced. Ordinary CI/CD and agent-step input/result isolation remain unchanged.
