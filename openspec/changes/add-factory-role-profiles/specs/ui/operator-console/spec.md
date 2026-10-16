## ADDED Requirements

### Requirement: Project role and profile authoring
The Console SHALL provide Project-scoped discovery, creation, validation, and immutable-version replacement of arbitrary Factory Roles and multiple Role Profiles per role. It SHALL offer copy-on-create coder/default/pro, reviewer/default, and architect/default starter templates and allow independent naming and configuration of their copies. Typed editors SHALL expose supported model/reasoning settings, skills/dependencies, instructions, tools, and limits from authorized server choices, with unresolved or incompatible settings clearly identified. Authoring SHALL preserve the existing in-memory-only draft, reviewed immutable intent, idempotency, current-version precondition, localization, accessibility, and safe error contracts.

#### Scenario: Operator configures default and pro coding profiles
- **WHEN** an operator copies the coder template into a Project
- **THEN** the editor presents two independently configurable profiles and does not imply that pro grants extra permissions or that updating the template will update the copies

#### Scenario: Operator creates a custom specialist
- **WHEN** an operator creates a custom role with another profile name using visible installed choices
- **THEN** the Console validates and publishes it through REST without requiring a fixed role category or hand-entered opaque reference

#### Scenario: Profile changes while being edited
- **WHEN** another operator publishes a replacement before the current draft is confirmed
- **THEN** the Console preserves the in-memory draft, reports the conflict, and requires explicit review against the new version instead of silently resubmitting

### Requirement: Stage selection and recovery policy editing
The Factory stage editor SHALL support `manual`, criteria-based `rules`, and `decision_api` selection over the selected role's pinned compatible profiles. It SHALL expose ordered typed criteria for rules mode and semantic criteria, installed provider choice, qualification/rollout status, and applicable thresholds for Decision API mode. Every stage SHALL explicitly choose either an exact reserve profile or manual selection on failure. The editor SHALL show candidate compatibility and the total bounded selection/execution requirements from server validation and SHALL NOT infer authority, invent unsupported provider/model parameters, or evaluate business rules in the browser.

#### Scenario: Stage uses rules and a reserve
- **WHEN** an operator configures high complexity to select coder-pro and chooses coder as reserve
- **THEN** the reviewed configuration includes exact candidate, criterion, order, and reserve versions before publication

#### Scenario: Stage delegates profile choice to JEV
- **WHEN** an operator selects a qualified JEV profile-selection capability
- **THEN** the editor shows the bounded candidates, supplied criteria, rollout semantics, and stage-specific failure behavior while distinguishing profile selection from routing configuration

### Requirement: Manual profile choices and selection history
The Console SHALL support optional explicit stage profile overrides during Work admission and eligible pre-dispatch selection through REST. A stage awaiting manual selection SHALL show its exact target, safe failure reason if present, eligible pinned profiles, current version, and deadline; submission SHALL require a target-specific reviewed intent with duplicate prevention. History SHALL distinguish automatic recommendations, manual/reserve choices, accepted intents, committed dispatch, and execution results. A selection SHALL NOT be displayed as stage success or candidate acceptance. Existing static Run views SHALL remain usable without role setup.

#### Scenario: Operator overrides automatic choice for one stage
- **WHEN** an operator selects coder-pro for a stage in the admission form
- **THEN** confirmation names that stage and exact profile, while other stages retain their configured selection policies

#### Scenario: Operator responds after selection failure
- **WHEN** a stage reports a current pending manual choice
- **THEN** the Console offers its authorized pinned candidates, submits one preconditioned selection intent, and waits for authoritative owner/dispatch state before showing execution

#### Scenario: Choice becomes stale while the form is open
- **WHEN** dispatch, cancellation, expiry, or another accepted choice invalidates the displayed target
- **THEN** confirmation is blocked or returns the server's conflict, and the Console requires refresh and review rather than silently choosing a new target/profile

#### Scenario: Selection history contains sensitive context
- **WHEN** the stage's criteria or skill package reference protected content
- **THEN** the Console displays safe policy, rule, profile, and receipt metadata without copying raw content into history, URLs, browser storage, analytics, or errors
