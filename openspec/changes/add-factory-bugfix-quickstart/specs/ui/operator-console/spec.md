## ADDED Requirements

### Requirement: Guided bug-fix configuration setup
For a visible Project on a prepared deployment, the Console SHALL offer a **Fix a bug** preset entry point and guide selection of compatible server-published execution profiles, pools, and budget/permission choices without requiring graph construction, schema editing, opaque identifiers, or raw JSON. It SHALL review the normalized exact preset and dependency versions, mandatory checks, effective permission narrowing, budgets, and accepted-candidate stopping point before ordinary immutable publication. Advanced authoring SHALL remain available, and a compatible already-published Configuration SHALL be reusable without creating another version for each task.

#### Scenario: Project has no bug-fix Configuration
- **WHEN** the operator selects Fix a bug in a Project with compatible installed dependencies
- **THEN** guided setup resolves the preset, collects its required choices, reviews authoritative validation, and publishes an ordinary Flow and Configuration before task submission

#### Scenario: Project already has a compatible Configuration
- **WHEN** the operator starts another bug fix in a configured Project
- **THEN** the Console offers its exact published Configuration and proceeds to text entry without rebuilding the graph or republishing unchanged definitions

#### Scenario: Required setup choice is absent
- **WHEN** the server cannot provide a compatible required execution or policy choice
- **THEN** setup reports a safe actionable unavailable state, keeps ordinary CI/CD usable, and does not fabricate defaults, request raw credentials, or auto-provision privileges

### Requirement: Direct text bug-fix submission
The Console SHALL accept a bug description and acceptance criteria directly, show the server-published UTF-8 byte bounds, and permit source selection only through the selected Repository policy. Project, Configuration, Repository, priority, and risk SHALL come from visible typed choices or reviewed policy defaults. The operator SHALL not need earlier Builds or uploaded Artifacts. Before submission the Console SHALL show the target Project, exact Configuration and Repository versions, selected source policy, budget, and accepted-candidate stopping point and use the existing immutable confirmed-intent helper. The browser SHALL treat text as plain data and keep unfinished text, request bodies, and replay identities only in component/session memory, never local persistence, URLs, analytics, logs, or error reports.

#### Scenario: Operator starts a task from text
- **WHEN** the operator enters valid description and acceptance text for a ready configured Project and confirms the displayed intent
- **THEN** the Console submits one typed text Work command and opens the returned ordinary Factory Run after admission commits

#### Scenario: Text exceeds the byte limit
- **WHEN** multi-byte description or acceptance text exceeds its published UTF-8 bound
- **THEN** the Console identifies the invalid field, preserves the in-memory text, and cannot confirm an oversized request; server validation remains authoritative

#### Scenario: Operator abandons an unsubmitted form
- **WHEN** the operator reloads or leaves text entry before confirmation
- **THEN** unfinished text is discarded without publication, admission, or browser-persisted task data

#### Scenario: Transport response is lost
- **WHEN** the confirmed submission response is lost while the intent remains active
- **THEN** retry reuses the unchanged frozen request and original idempotency key rather than resampling source, defaults, or Configuration versions

### Requirement: Actionable readiness in the bug-fix journey
The Console SHALL show the server's exact configuration-scoped readiness checks with non-color statuses, freshness, safe explanations, and visible remediation links. It SHALL distinguish missing capability, unknown or stale facts, and queue delay, and SHALL NOT derive compatibility from Agent facts. Changes to Configuration, Repository, or relevant selected versions SHALL invalidate the displayed readiness association. Confirmation SHALL be unavailable while a required check is blocked, unknown, stale, loading, or failed. A ready display SHALL not imply a reservation or bypass server and Agent checks.

#### Scenario: Required enforcement capability is missing
- **WHEN** readiness reports no qualified compatible capacity for a required node
- **THEN** the Console explains the blocker and links to permitted configuration or capacity guidance without offering weakened enforcement

#### Scenario: Readiness expires before confirmation
- **WHEN** a report expires or its bound subject changes
- **THEN** the Console marks it stale and obtains a new report before enabling confirmation

#### Scenario: Capability disappears after confirmation
- **WHEN** server admission revalidation rejects a requirement that was previously ready
- **THEN** the Console shows the safe blocker and correlation, preserves unsent or active in-memory intent as applicable, and never silently retries through another backend or with broader rights

### Requirement: Pending intake and advanced workflows remain explicit
The Console SHALL distinguish pending text-input publication from an admitted Factory Run, show the safe server receipt, and observe its terminal admission result with bounded polling and backoff only while the view is active. It SHALL never claim model execution or candidate success from a pending receipt. Ready Runs SHALL use existing diagnostics and preconditioned commands. The Artifact-based Work form, advanced Flow authoring, ordinary activities, English/Russian localization, keyboard access, trusted same-origin deployment, and REST-only business boundary SHALL remain available.

#### Scenario: Text publication is pending
- **WHEN** the command returns a recoverable pending receipt
- **THEN** the Console displays preparation state and follows that same receipt until admission or a safe terminal failure instead of submitting another task

#### Scenario: Quickstart is unavailable
- **WHEN** the deployment does not support the preset or text-intake capability
- **THEN** the Console presents an intentional optional-mode unavailable state and existing Factory and ordinary CI/CD views continue to work

#### Scenario: Candidate approval is stale
- **WHEN** an operator later acts using a superseded Run or Node Attempt precondition
- **THEN** existing guarded-command behavior reports the conflict and does not authorize another candidate through the quickstart view
