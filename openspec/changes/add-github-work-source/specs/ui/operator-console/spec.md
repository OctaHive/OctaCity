## ADDED Requirements

### Requirement: Pipeline inputs expose guided GitHub Issue selection
The console SHALL allow an operator to configure GitHub Issues as a source at the selected Factory pipeline input, choosing an existing Project-authorized connection and external task container independently of the code Repository, visible installed capabilities, target configuration, Issue state, all-label, any-label and excluded-label sets, input mapping, optional exclusive group and enabled state. It SHALL explain empty label selection as unrestricted, provide bounded read-only preview and authoritative validation, and publish reviewed immutable replacements through the existing confirmed-intent workflow. The operator SHALL NOT need to write provider JSON, enter opaque resource identities or supply credentials in the pipeline editor. A missing connection or grant SHALL link to the separate Integrations administration/access flow without enabling unsupported choices. Selector drafts and preview payloads SHALL remain outside browser persistence.

#### Scenario: Operator prepares assessment and implementation inputs
- **WHEN** an operator configures an assessment input with no required labels and separate `approved + bug` and `approved + feature-request` inputs
- **THEN** the console shows each input next to its exact target pipeline and makes exclusions and exclusive grouping reviewable before activation

#### Scenario: Operator selects no required labels
- **WHEN** all three label sets are empty
- **THEN** the console explains that labeled and unlabeled Issues can match and does not silently add a label restriction

#### Scenario: Server rejects an incompatible input
- **WHEN** validation reports a hidden dependency, contradictory selector, unsupported mapping or stale version
- **THEN** the console identifies the safe error in the appropriate section, preserves the in-memory draft and prevents activation until a new valid intent is confirmed

### Requirement: GitHub reporting mappings and source progress are reviewable
The console SHALL expose guided configuration of arbitrary Work status values mapped to permitted managed-label additions/removals and optional Issue open/close state. It SHALL present safe Source/Reporter status and Work/Run correlations from REST, distinguish matching from admitted and pending projection from applied effects, and identify ambiguous grouped inputs without deriving lifecycle authority from GitHub labels. Reporting and source choices SHALL be filtered by server-published capabilities; no control SHALL grant itself write authority or expose protected provider material.

#### Scenario: Operator configures assessment reporting
- **WHEN** an operator maps accepted `approved` and `rejected` outcomes to mutually exclusive managed labels
- **THEN** review shows exact label effects, unrelated-label preservation and whether Issue state will change before publication

#### Scenario: Status projection is pending
- **WHEN** a Run completed assessment but its GitHub update is retrying or exhausted
- **THEN** the console shows those distinct facts and does not display the downstream pipeline as admitted without an authoritative admission receipt

### Requirement: Stage authoring exposes source label-update actions
The console SHALL let an operator configure a source-update action for any declared stage outcome in the existing Flow editor, including intermediate and nested stages. The editor SHALL expose the original Issue source, permitted labels to add, labels to remove and the exact resulting mapping/effects, and SHALL persist them as ordinary immutable trusted-action configuration rather than a special stage implementation. Different outcomes SHALL support different effects, and stages without an action SHALL leave the Issue unchanged. No approval, rejection or classification vocabulary SHALL be mandatory.

#### Scenario: Operator adds an update after a nonterminal stage
- **WHEN** an operator selects an intermediate stage outcome and configures removal of one arbitrary label and addition of another
- **THEN** the editor shows the source-update action on that route, validates its authority through REST and allows publishing without moving it to the pipeline's terminal stage

#### Scenario: Operator inspects stage update progress
- **WHEN** an intermediate stage committed and its source update is pending, applied or failed
- **THEN** the console associates that reporting state with the exact stage/action and keeps it distinct from the overall pipeline result

### Requirement: Integrations has dedicated reusable connection administration
The console SHALL provide a dedicated provider-neutral Integrations section for authorized operators to list, create, inspect, replace, check and disable named connections, manage credentials and inspect permitted dependents. It SHALL present installed providers, allowed endpoints, supported authentication methods, permitted external resources and separate capability/readiness states through typed server choices. GitHub SHALL expose its repository owner/name context and token setup without making those fields mandatory for other providers. The section SHALL distinguish connection configuration from access granted to an OctaCity Project and from pipeline filtering/routing. Only installed supported capabilities SHALL be offered. Forms SHALL use existing review/version-conflict patterns, keyboard access and English/Russian messages.

#### Scenario: Operator creates one connection for multiple pipelines
- **WHEN** an administrator configures a named GitHub connection and grants selected resources to a Project
- **THEN** several pipeline editors can select that connection without copying its credentials or its per-pipeline predicates

#### Scenario: Provider is not installed
- **WHEN** a provider has no compatible installed adapter
- **THEN** the console identifies it as unavailable and cannot present metadata alone as a working source

### Requirement: Credential entry stays isolated and transient
The console SHALL collect raw credentials only in a separately authorized write-only integration setup/rotation flow, never in pipeline or ordinary connection metadata forms. Existing credentials SHALL be represented by configured/missing state and safe metadata, with replacement rather than value read-back. Secret input SHALL remain transient, SHALL NOT enter browser persistence, URLs, query/mutation caches, analytics, confirmation summaries or errors, and SHALL be cleared after submission or leaving the flow. Server errors SHALL be rendered as safe classifications without echoing the submitted value. Selecting an existing permitted server-side handle SHALL NOT require retrieving its contents.

#### Scenario: Administrator submits or rotates a token
- **WHEN** the protected credential form is submitted
- **THEN** the console shows a safe outcome, clears the secret field and exposes no token value in detail/review screens or cached state

#### Scenario: Administrator leaves an unfinished credential form
- **WHEN** the credential flow is closed or navigation leaves it
- **THEN** returning does not restore the secret from browser storage or client caches

### Requirement: Project access and capability readiness are explicit
The console SHALL let authorized administrators grant/revoke selected connection resources and purposes for an OctaCity Project, showing task-read, task-update, repository-read and Delivery access independently. Pipeline users SHALL see only their authorized choices and SHALL NOT gain connection-administration rights by creating a pipeline. Connection checks SHALL be visibly read-only and distinguish configuration, authentication, resource access and capability readiness from pipeline activation. Disablement/revocation SHALL show safe dependency effects without claiming to cancel already accepted remote requests or undo Work.

#### Scenario: Read access is available without Reporter access
- **WHEN** a Project has task-read access and no task-update grant
- **THEN** the console permits read intake, explains unavailable reporting and does not require a write credential to use the read source

#### Scenario: Administrator revokes access
- **WHEN** an administrator reviews revocation for a connection used by visible inputs and pending reporting
- **THEN** the console shows those dependents, explains blocked subsequent provider operations and preserves the distinction from accepted pipeline results

#### Scenario: Manual Factory intake remains selected
- **WHEN** the operator chooses manual intake with no provider connection
- **THEN** the console allows ordinary manual Work creation without opening integration setup or requiring tokens
