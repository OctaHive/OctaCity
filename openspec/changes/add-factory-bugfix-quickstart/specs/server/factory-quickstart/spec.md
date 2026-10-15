## Purpose

Defines a supported first bug-fix journey using immutable Factory presets, direct text Work intake, and actionable readiness without weakening the existing Dark Factory execution guarantees.

## ADDED Requirements

### Requirement: Release-versioned bug-fix preset
The server SHALL expose one supported `bugfix` preset with an immutable version and content digest. It SHALL resolve a complete bounded Flow closure and Factory Configuration from selected visible Project-owned profiles, pools, permission ceilings, and budgets through ordinary Factory validation and publication. A preset SHALL be configuration over existing technical primitives, not a special execution mode or business-stage dispatcher. Replacement presets SHALL NOT change published definitions or admitted Runs.

#### Scenario: First bug-fix configuration is prepared
- **WHEN** an operator selects the supported bug-fix preset and compatible installed Project choices
- **THEN** the server returns a normalized reviewable Flow and Configuration draft, its preset identity, exact dependency identities, effective permissions, budgets, and unresolved diagnostics without publishing versions or starting Work

#### Scenario: A preset changes after publication
- **WHEN** the installed preset is replaced after a Configuration or Run was created
- **THEN** existing versions and Runs retain their original preset provenance, Flow closure, profiles, policies, and budgets

### Requirement: Bug-fix preset retains mandatory execution gates
The preset SHALL retain defect eligibility and reproduction, an exact Accepted Work Contract, protected test authoring, verified regression failure on the original subject before writable implementation, exact ChangeSet capture, protected candidate validation, independently scoped review, and every verification and acceptance gate required by the selected Project policy. Environment or execution failure SHALL NOT establish the regression Red observation. Bounded correction and human-input routes SHALL remain explicit. The preset SHALL terminate with a retained accepted candidate for review and SHALL NOT grant delivery, merge, production, or forge-write authority. Optional Decision Signals SHALL NOT be required by the preset.

#### Scenario: Original subject exhibits the specified regression
- **WHEN** trusted baseline validation records the required protected regression failure for the exact original subject and accepted contract
- **THEN** the declared gate permits implementation while its Context omits protected test sources, assertions, author transcripts, and private grants

#### Scenario: Baseline checks fail because the environment is broken
- **WHEN** regression execution cannot run or fails for an infrastructure reason instead of the specified defect
- **THEN** the configured retry or human-input route is selected and writable implementation is not authorized by that observation

#### Scenario: Required evidence is missing or failed
- **WHEN** Project policy requires a verification class or independent assessment that is absent, failed, stale, or indeterminate
- **THEN** neither the preset nor model prose can accept the candidate or skip that required gate

#### Scenario: A simplified preset requests broader authority
- **WHEN** preset settings would widen Project permissions, weaken a required gate, exceed a budget, or select an unqualified backend
- **THEN** authoritative validation rejects publication without silently granting rights or falling back to Host execution

### Requirement: Bounded direct text Work input
Manual text intake SHALL accept a nonblank bug description and nonblank acceptance criteria, each at most 16,384 UTF-8 bytes, together with the existing bounded Work identity, exact Configuration and Repository versions, allowed source selection, and classification. The server SHALL retain exactly the submitted text in two immutable content-addressed documents with explicit manual-intake provenance and encoded lengths. It SHALL reject oversized, malformed, empty, or unsupported input before object writes or Work admission rather than truncate or silently rewrite it. Text SHALL remain untrusted task data and SHALL NOT choose profiles, policies, executable commands, context grants, or Flow edges.

#### Scenario: Operator submits text without earlier Builds
- **WHEN** valid description and acceptance text are submitted for an enabled visible Configuration and Repository
- **THEN** the server creates the required immutable Work input identities and can admit ordinary Work without a prior Build or caller-created Artifact

#### Scenario: Multi-byte text exceeds the bound
- **WHEN** submitted text contains fewer than 16,384 characters but more than 16,384 UTF-8 bytes
- **THEN** validation returns a field-specific bounded error and creates no input publication or Run

#### Scenario: Task text contains policy instructions
- **WHEN** description or acceptance text requests hidden tests, broader credentials, skipped checks, or an undeclared route
- **THEN** it remains untrusted scoped context and cannot alter runtime authority, deterministic evidence, or the admitted Flow

### Requirement: Recoverable text publication and admission
Text intake SHALL reserve one scoped immutable submission intent before byte-store writes, bind one exact source revision and Configuration, publish verified complete documents, and admit at most one ordinary Work Envelope and Factory Run. The original request fingerprint SHALL include both text fields, selected immutable definitions, source selection, Work identity, and classification. Exact replay SHALL observe the original result or recover the same pending intent before mutable source or catalogue resolution; changed reuse SHALL conflict. Pending publications SHALL be unavailable to execution, and incomplete publication SHALL NOT admit a Run. Concurrent recovery SHALL use fenced ownership and durable cleanup identities.

#### Scenario: Admission response is lost after the branch advances
- **WHEN** an exact submission is retried after admission committed and the selected branch or preset changed
- **THEN** the original Run, Work input identities, document digests, Configuration, and exact base revision are returned without another logical admission or source resolution

#### Scenario: Server restarts between document writes
- **WHEN** a durable submission has written one document and the server loses memory
- **THEN** recovery verifies or republishes the same reserved byte identities and admits no Run until both exact documents are published

#### Scenario: Concurrent intake and recovery race
- **WHEN** multiple requests or owners process the same scoped submission
- **THEN** they converge on one frozen subject, one complete document pair, and one Run, and stale owners cannot publish substituted inputs

#### Scenario: Replay changes acceptance text
- **WHEN** the same scoped idempotency or external Work identity is reused with different acceptance text or another bound request field
- **THEN** the server returns a stable conflict and creates no replacement documents or second Run

### Requirement: Work inputs have truthful provenance and retention
Text documents SHALL be Project-owned managed Work inputs with verified content, submission identity, publisher kind, publication time, and retention. They SHALL NOT fabricate a Build, Job, Agent, Lease, output report, or test-evidence producer. Selected input reads and transfers SHALL verify owning Project, publication state, digest, size, explicit Context selection, current Job authority where applicable, and required retention window. Pending or failed reservations SHALL be cleaned up within bounded retention; active, held, or recoverable Runs SHALL retain their required inputs, and cleanup SHALL NOT replenish budgets or delete a concurrently retained document.

#### Scenario: Implementation consumes a manually authored description
- **WHEN** the admitted Flow explicitly projects the task document to an implementation call
- **THEN** exact verified bytes enter only that call's recorded Context Manifest and signed input grants with manual-intake provenance, not execution-evidence provenance

#### Scenario: Another Project requests a text input
- **WHEN** a request selects an input owned by another Project or outside its explicit permitted Context
- **THEN** the read or transfer is rejected before private bytes or capabilities are exposed

#### Scenario: Cleanup encounters admitted and abandoned submissions
- **WHEN** retention processes expired incomplete submissions and inputs required by active or held Runs
- **THEN** only eligible abandoned bytes lose visibility, admitted inputs remain retained, and interrupted deletion retries idempotently

### Requirement: Configuration-scoped readiness is read-only and actionable
The server SHALL provide a bounded readiness report for an exact published Factory Configuration and Repository version. It SHALL cover required reachable profiles, Flow validity and policy, Factory worker and required dependency health, signing and storage readiness, permitted pools, fresh verified Agent/plugin/executable/backend capabilities, and configured credential-profile availability using metadata rather than secret values. Each required check SHALL report `ready`, `blocked`, or `unknown`, a safe stable code, observation time, expiry, and a permitted remediation target. Overall `ready` SHALL require all required checks ready; unknown or stale required facts SHALL NOT be treated as pass. Optional capabilities absent from the closure SHALL NOT block readiness.

#### Scenario: A required execution capability is unavailable
- **WHEN** no permitted fresh Agent capability satisfies a required profile's enforcement or pinned executable contract
- **THEN** readiness reports a safe configuration-scoped blocker and remediation without exposing hidden resources or claiming executable compatibility

#### Scenario: Compatible capacity is temporarily busy
- **WHEN** verified permitted capacity is compatible but currently busy and no required capability is missing
- **THEN** the report distinguishes queue delay from incompatibility and does not promise an immediate lease or reserve capacity

#### Scenario: A required fact is stale or unobservable
- **WHEN** a required health or verified inventory observation expired or cannot be established
- **THEN** the report marks that check unknown or blocked, cannot report overall ready, and supplies safe refresh or operator guidance

#### Scenario: The initial preset has no delivery or signal capability
- **WHEN** the exact bug-fix Configuration ends at a retained accepted candidate and selects no Decision Signal or delivery action
- **THEN** absent forge, deployment, or signal adapters do not become readiness dependencies

#### Scenario: Readiness is queried repeatedly
- **WHEN** a client repeats the readiness query
- **THEN** it starts no Build, model or forge call, resolves no secret value, writes no lifecycle or mutation-audit fact, and reserves no Agent, budget, or storage object

### Requirement: Readiness never replaces execution admission
A readiness report SHALL be an observation bound to exact subjects, not an authorization token, successful test, or lease reservation. Fresh text intake SHALL repeat required admission and selected-configuration checks before accepting Work; dispatch and Agent execution SHALL retain their unconditional capability, signed intent, local grant, retention, and fencing checks. A pending intake SHALL retain frozen intent while recovery revalidates live enforcement conditions without changing its subject or accepting an unsafe fallback.

#### Scenario: Backend support disappears after a ready report
- **WHEN** the selected enforcement capability disappears before intake commits or before Job execution
- **THEN** fresh intake is blocked or pending recovery waits safely, and an admitted Job remains unscheduled or rejected by existing enforcement rather than executing through fallback

#### Scenario: The browser submits a fabricated ready result
- **WHEN** a client posts a readiness flag, edited check result, or readiness report as supposed execution authority
- **THEN** the server rejects unsupported command fields and independently enforces the admitted contract

### Requirement: Quickstart preserves ordinary CI/CD and Factory contracts
Quickstart SHALL use the existing Work, Run, Flow, Build, Job, Artifact-byte, signed JobSpec v3, and fenced execution boundaries. Its inputs SHALL not widen permissions, context, budgets, WIP, descendant, network, output, or credential authority. Required evidence, independent review, append-only history, bounded rework, cancellation, exact operator preconditions, audit redaction, and restart recovery SHALL remain authoritative. Existing Artifact-based Factory intake and ordinary CI/CD SHALL remain operational with quickstart disabled or unavailable.

#### Scenario: Quickstart capability is disabled
- **WHEN** the deployment lacks quickstart configuration or the bug-fix preset cannot be resolved
- **THEN** ordinary Builds and previously supported Factory APIs remain available under their existing readiness and authorization contracts

#### Scenario: Candidate changes before an operator action
- **WHEN** an operator command uses a precondition from an older candidate, cycle, or Run version
- **THEN** the existing conflict contract rejects it and quickstart cannot apply the action to a newer candidate
