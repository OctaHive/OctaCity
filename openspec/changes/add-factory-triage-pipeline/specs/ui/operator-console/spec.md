## ADDED Requirements

### Requirement: Operators configure a complete two-agent triage through guided setup
The Console SHALL provide a Project-scoped triage setup using visible preset, Role/Profile, Project-context, repository, Source, workflow and reporting choices with existing validation/publication controls. It SHALL default to Project and Systems analyst stage labels while permitting reviewed Role/Profile selection; role names SHALL NOT imply installed tools or permission. Strictly unlabeled selection SHALL be distinguishable from exclusion-based filtering. The review SHALL identify process-completion criteria separately from implementation acceptance, finite outcomes/effects and missing installation or label-scope prerequisites. It SHALL require no raw JSON, opaque-ID entry or credential values in pipeline settings.

Publication, source preview, canonical Issue import and execution intake SHALL remain separately controlled; new intake SHALL remain disabled until its normal explicit confirmation. Stale parents or partial setup SHALL preserve useful drafts and block invalid continuation without automatic repinning. English/Russian, narrow-screen and keyboard use SHALL expose the same decisions, and unconfirmed text, previews and mutation keys SHALL stay out of browser persistence.

Setup SHALL expose safe selected MCP connection/tool, credential-reference and read-only/resource-scope metadata through existing Runtime/Profile choices and upstream installation guidance. It SHALL distinguish OctaCity/external MCP read access, AI-provider authentication and server Source/Reporter authority. Missing endpoint, scoped authentication, plugin transport or evidence-capture support SHALL have actionable readiness reasons. Triage setup SHALL NOT collect token values, add a provider-specific MCP editor or imply that saved configuration proves an actual permitted call.

#### Scenario: An operator configures the preset for the first time
- **WHEN** compatible execution, read/update, context and label choices exist
- **THEN** the operator can review and publish both stage bindings and exact input/reporting settings, preview raw unlabeled tickets and separately enable intake using the normal controls

#### Scenario: A requested label is unavailable
- **WHEN** the reviewed effect vocabulary includes a label absent from the current permitted GitHub choices
- **THEN** setup explains the missing existing-label scope and exposes the integration remediation path without creating a label, accepting arbitrary text or claiming a successful configuration

#### Scenario: Runtime setup is incomplete
- **WHEN** a profile can be edited but cannot execute under the installed Agent policy
- **THEN** the UI shows the specific installation/readiness blocker and the existing setup handoff instead of offering an apparently executable triage

#### Scenario: A role needs another MCP source
- **WHEN** an operator configures a compatible ready-made server through existing Runtime/Profile settings and returns to triage setup
- **THEN** setup can select its exact authorized connection and show separate safe credential/scope metadata without requiring an OctaCity provider wrapper or token entry in the pipeline form

### Requirement: Triage diagnostics explain decisions and delivery independently
The Console SHALL render retained Project and analyst results with their exact stage/profile/context provenance, accepted eligibility/classification outcome, bounded classification fields, reproduction evidence or unresolved reason, optional local workflow status and separate GitHub delivery receipts. It SHALL provide ordinary original Run/Issue navigation and current authorized human-review actions without exposing private reasoning or hidden candidates. A failed or delayed provider write SHALL NOT be presented as a failed accepted AI decision or a successful delivery.

Duplicate evidence SHALL link to retained authorized MCP observations and show consumed source scope and incomplete, unavailable or unknown coverage distinctly. Model assertions alone SHALL NOT be displayed as verified tool execution, and connection secrets SHALL NOT appear in diagnostics.

#### Scenario: Classification succeeded but GitHub update failed
- **WHEN** the accepted analyst result exists and its Reporter receipt is pending, forbidden or exhausted
- **THEN** the result remains readable with its correct local status and a separate actionable reporting condition

#### Scenario: Eligibility requires human input
- **WHEN** Project assessment cannot satisfy the reviewed fit/duplicate rule
- **THEN** the UI explains the declared review/needs-input state without displaying a nonexistent analyst result or starting that stage from a page read
