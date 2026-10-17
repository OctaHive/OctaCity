## ADDED Requirements

### Requirement: Pipeline Work Source input bindings are versioned and explicit
The server SHALL support Project-owned immutable versions of pipeline Work Source input bindings selecting a reusable connection and native task resource authorized for that OctaCity Project, an installed source capability, bounded provider-specific selector, exact target Factory Configuration/root Flow, Repository source policy, input mapping and optional reporting mapping. Activating or replacing an input SHALL require authorized versioned configuration; the binding's stable identity SHALL survive ordinary replacement. Admission SHALL retain the exact binding and target versions. Provider labels SHALL remain selection data outside the Flow interpreter and SHALL NOT introduce phase-name dispatch or bypass existing eligibility, business duplicate, budget, permission or acceptance gates.

#### Scenario: Operator changes the labels for a pipeline input
- **WHEN** an operator publishes a replacement input selector with the current-version precondition
- **THEN** subsequent discovery uses the replacement while existing Work and reporting intents retain their original immutable inputs and policies

#### Scenario: A manually labeled Issue matches implementation input
- **WHEN** an Issue acquires the labels required by a configured implementation input
- **THEN** it can enter ordinary admission under that input's policy but its labels do not substitute for accepted assessment evidence or required pipeline gates

### Requirement: Repeated Issue observations reuse admission within a stable input scope
Admission SHALL reuse its authoritative replay and atomic Work/Run publication for external sources. A scoped Work identity SHALL include the authorized installation/repository/item identity and stable input-binding identity, so one Issue can enter distinct configured pipelines without poll retries creating additional Runs for either input. Poll timestamps, Issue edits, label removal/readdition, credential rotation and ordinary binding/configuration version replacement SHALL NOT create a new Work identity. A new intentional automatic-intake processing cycle SHALL require an explicit authorized new input identity; changing a selector SHALL NOT silently reprocess previously admitted Issues. A separately specified explicit internal-Issue execution request SHALL retain its own reviewed request/admission identity rather than changing an automatic input's replay key. Conflicting reuse of an admitted identity SHALL preserve the original Work and return a safe conflict rather than replace its intent.

#### Scenario: Poll response is lost after assessment admission
- **WHEN** an assessment input observes the same Issue after a lost response, label change or restart
- **THEN** it obtains its original Work/Run identity without resolving the mutable repository base or re-running business duplicate classification

#### Scenario: Approved Issue enters another pipeline
- **WHEN** the same Issue previously admitted to an assessment input later matches a different implementation input
- **THEN** ordinary admission can create one separate Work/Run for that input, retain their common Issue correlation and apply the implementation pipeline's existing policy

#### Scenario: Input version is replaced
- **WHEN** an operator changes a target configuration or polling selector while preserving the input's identity
- **THEN** an already admitted Issue keeps its original result and does not receive an implicit new processing cycle

#### Scenario: Operator later requests execution from an internal task card
- **WHEN** an enabled internal-Issue capability accepts a new reviewed execution request for the same native task
- **THEN** its distinct explicit request uses ordinary admission without resetting the existing automatic input identity or treating an external edit as that request

### Requirement: Native task observations remain independent of admission and optional cards
Factory Source integration SHALL retain trusted native task correlation separately from input-scoped Work identity: owning Project/security partition, installation namespace, native container and item identity SHALL identify the external task without a pipeline input, display name, credential or mutable label. A bounded execution-neutral observation seam SHALL preserve native correlation, freshness evidence and separately authorized snapshot provenance for optional internal-Issue consumers. Observation or consumer availability SHALL NOT itself admit Work, change existing Work inputs or grant reporting authority. Existing automatic inputs SHALL operate without an internal Issue capability. Persistent Issue creation/revisions, association uniqueness, standalone import/link refresh and explicit card execution requests SHALL remain owned by `add-factory-work-card`.

#### Scenario: Assessment and implementation refer to the same external task
- **WHEN** their distinct configured inputs observe one native item in the same Project/security scope
- **THEN** they expose the same native correlation and distinct original Work/Run identities suitable for one optional internal Issue without changing either admission scope

#### Scenario: Task card consumer is absent
- **WHEN** the deployment has no enabled internal-Issue consumer
- **THEN** existing intake/reporting and manual Artifact admission continue without card creation, extra provider reads or a dependency on task-card storage

#### Scenario: Linked task needs refresh after it stops matching an execution selector
- **WHEN** a separately enabled and authorized card refresh consumer requests an observation of that native task
- **THEN** the Source observation boundary does not treat the execution selector or prior admission as a task-read prohibition, while that observation alone starts no Run

### Requirement: Staged label-selected pipelines use declarative routing
Operators SHALL be able to configure an unrestricted or filtered Issue input for one pipeline, project its declared outcomes to arbitrary configured labels, and configure separate inputs matching those labels and any additional configured predicates for distinct target pipelines. No particular label, assessment step, business classification or number of pipelines SHALL be mandatory. The server SHALL support explicit mutually exclusive input groups: a candidate matching more than one member in the same group SHALL receive an observable ambiguous disposition and SHALL NOT be admitted to either target. Inputs outside that group SHALL remain independent. A published group revision SHALL freeze its member selectors for each selection pass. No connector SHALL infer a target from the spelling of a label or implement another duplicate-analysis pipeline.

#### Scenario: Assessment feeds defect or feature implementation
- **WHEN** accepted configured assessment actions project `approved + bug` or `approved + feature-request` and the corresponding downstream input observes that snapshot
- **THEN** the configured defect or feature pipeline respectively receives ordinary Work once, while `rejected` Issues are excluded by the supplied downstream selectors

#### Scenario: Classification labels overlap
- **WHEN** an Issue contains both required defect and feature label sets and matches both members of one exclusive implementation group
- **THEN** neither implementation pipeline starts and diagnostics identify the ambiguous input selection for operator correction

#### Scenario: Operator uses unrelated label names and pipeline stages
- **WHEN** inputs select operator-defined labels such as `ready-for-review` and `documentation`, with no approval or defect labels configured
- **THEN** those selectors and their target pipelines work through the same contracts without adding a recognized label name or changing connector code

#### Scenario: Two independent input groups select the same Issue
- **WHEN** distinct authorized inputs intentionally outside the same exclusive group match an Issue
- **THEN** their admission scopes remain independent and each retains at most one Run for its stable input identity

### Requirement: Every configured stage outcome can project source label changes
A Flow SHALL allow an operator to configure source-update actions after any selected declared stage outcome, including intermediate stages, nested stages and explicitly configured failure or escalation branches. Each action SHALL select a pinned source mapping with explicit bounded label additions/removals for the original Issue. No terminal pipeline result, fixed business phase or recognized label spelling SHALL be required. The action SHALL execute through existing trusted-action and durable reporting contracts, with the accepted originating stage/outcome and stable operation identity retained for audit. Model prose and an unhandled execution error SHALL NOT independently select label effects; only the configured authoritative branch can enqueue them.

#### Scenario: Intermediate research changes Issue labels
- **WHEN** an operator configures an accepted research outcome to remove `investigating` and add `ready-for-review`
- **THEN** its source-update action queues those exact effects for the originating Issue before pipeline completion, while unrelated labels and existing Flow authority remain intact

#### Scenario: Different outcomes of one stage have different effects
- **WHEN** a stage's declared success and escalation routes select different configured label-update actions
- **THEN** only the action on the accepted route is enqueued, and retry of that same action preserves its original operation identity

#### Scenario: A stage has no configured source action
- **WHEN** an intermediate or terminal stage completes without a selected source-update action
- **THEN** its completion does not implicitly add, remove or infer any Issue label

### Requirement: Reusable connections remain independent of pipeline routing and code repositories
Factory SHALL let distinct pipeline inputs reuse the same authorized provider connection without copying credentials or changing their independent admission scopes. The connection, its external task container, the owning OctaCity Project and the code Repository SHALL remain distinct selections. Common connection/input contracts SHALL support open provider identities and opaque native resource references; provider-specific authentication, ticket predicates and native effects SHALL remain in installed adapters. Native support for a provider SHALL be advertised before it is offered as a usable input.

#### Scenario: Several pipelines use one connection
- **WHEN** assessment and implementation inputs select the same permitted connection and external container with different stable input identities
- **THEN** they reuse connection credentials while preserving their own predicates, exact targets and ordinary one-admission-per-input replay

#### Scenario: Task source and code host differ
- **WHEN** compatible installed adapters and Project grants provide a task source from one provider and a code Repository from another
- **THEN** pipeline configuration validates those resources independently without requiring a shared host, provider or external project name

#### Scenario: A connection is renamed or its token is rotated
- **WHEN** an administrator changes its display name or rotates a credential without changing its stable installation/resource scope
- **THEN** existing inputs and admitted Work keep their identities and repeated polling does not create a new processing cycle

#### Scenario: Manual input is selected
- **WHEN** an operator creates manually entered Factory Work without configuring an integration
- **THEN** ordinary admission remains available and does not attempt credential lookup, connection checks or provider calls
