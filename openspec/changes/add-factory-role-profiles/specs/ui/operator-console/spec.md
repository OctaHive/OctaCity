## ADDED Requirements

### Requirement: Project AI agent and profile authoring
The Console SHALL provide Project-scoped discovery, creation, validation, and immutable-version replacement of arbitrary Factory Roles and multiple Role Profiles per role through an **AI agents** catalog (`AI-агенты` in Russian). One AI-agent card SHALL represent one Factory Role using its stable identity and SHALL contain its Role Profiles. Name, uploaded raster avatar, description, shared versioned skill folders, ordered shared instruction entries and agent-wide concurrency limit SHALL be edited as role content. Runtime, Model, Thinking level, additional ordered instructions, environment variables, CLI argument lists, MCP servers, external integration bindings, tools and resource ceilings SHALL be edited as profile content through the existing role/profile REST contracts and authorized choices. Secret variables SHALL use logical references without revealing stored credentials; runtime-specific unsupported settings SHALL be identified rather than silently ignored. For a selected exact profile, the card SHALL show its pinned shared and additional instruction layers and assigned skills from authorized role/profile details and normalized validation data. Unresolved or incompatible settings SHALL be clearly identified. The AI-agent catalog SHALL be separate from enrolled CI/CD Agent/Agent Pool management and SHALL NOT introduce another agent registry, storage model, or worker enrollment flow. It SHALL offer copy-on-create coder/default/pro, reviewer/default, and architect/default starter templates and allow independent naming and configuration of their copies. Authoring SHALL preserve the existing in-memory-only draft, reviewed immutable intent, idempotency, current-version precondition, localization, accessibility, and safe error contracts.

#### Scenario: Operator distinguishes AI agents from CI/CD workers
- **WHEN** a Project contains a Factory Role with two profiles and the deployment has enrolled worker Agents
- **THEN** its AI-agent catalog shows one role-backed agent card with two profiles, while worker Agent/Pool capacity and enrollment remain in their existing Console area

#### Scenario: Operator configures default and pro coding profiles
- **WHEN** an operator copies the coder template into a Project
- **THEN** the editor presents two independently configurable profiles and does not imply that pro grants extra permissions or that updating the template will update the copies

#### Scenario: Operator creates a custom specialist
- **WHEN** an operator creates a custom AI agent, enters shared instructions, and configures profile instructions using visible installed choices and shared skills on the AI agent
- **THEN** the Console validates and publishes ordinary role/profile definitions through REST without a second AI-agent identity, fixed role category, or hand-entered opaque reference

#### Scenario: Profile changes while being edited
- **WHEN** another operator publishes a replacement before the current draft is confirmed
- **THEN** the Console preserves the in-memory draft, reports the conflict, and requires explicit review against the new version instead of silently resubmitting

### Requirement: Skill folder import preserves the AI-agent editor draft
The Console SHALL offer import of a local skill folder beside the shared skill list without closing the AI-agent editor. The operator SHALL select the folder, review its `SKILL.md` and supporting relative-path file set, and explicitly import it through the narrow authorized Project skill-import contract. The Console SHALL add the resulting exact bundle/version to the selected skills of the current AI-agent draft and SHALL preserve other in-memory draft fields on import success, cancellation, failure or a lost-response retry. Updating a local folder SHALL require explicit reimport, which selects the new version only in the current draft; it SHALL NOT silently modify a saved role or profile. Skill bytes, environment values, instruction content and private connection data SHALL NOT be copied to URLs, browser persistence or analytics. The import control SHALL follow existing localization, keyboard/focus and narrow-layout conventions.

#### Scenario: Operator imports a folder while editing an AI agent
- **WHEN** an operator has entered agent instructions and selected other skills and imports a folder containing `SKILL.md` and supporting files
- **THEN** the saved immutable bundle is selected in the same open draft, other selected skills and entered fields remain intact, and publishing the agent explicitly pins the shared skill version

#### Scenario: Folder import fails or is cancelled
- **WHEN** the operator cancels the folder picker or the server rejects an unsafe/oversized/incomplete folder
- **THEN** the existing AI-agent draft and selected skills remain intact and no successful import or profile publication is claimed

### Requirement: Stage selection and recovery policy editing
The Factory stage editor SHALL support `manual`, criteria-based `rules`, and `decision_api` selection over the selected AI agent's role-bound pinned compatible profiles. Selectors SHALL identify the AI agent and profile using the same exact role/profile references. It SHALL expose ordered typed criteria for rules mode and semantic criteria, installed provider choice, qualification/rollout status, and applicable thresholds for Decision API mode. Every stage SHALL explicitly choose either an exact reserve profile or manual selection on failure. The editor SHALL show candidate compatibility and the total bounded selection/execution requirements from server validation and SHALL NOT infer authority, invent unsupported provider/model parameters, or evaluate business rules in the browser.

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

### Requirement: Discoverable provider/runtime setup and independent model choices
Console SHALL provide a discoverable Project settings workflow for supported provider/runtime configuration and the available-model catalog. Credentials SHALL use existing managed-secret operations and SHALL not persist in browser storage or Role/Profile content. An operator SHALL choose Runtime, model and supported thinking separately for each arbitrary named profile, including default/pro, using the same authorized server choices as validation. Missing execution configuration SHALL have an actionable explanation while ordinary agent identity/instructions/shared-skill authoring remains usable. Skill browsing SHALL show the retained relative file inventory and explicitly selected inert text without discarding the agent draft.

#### Scenario: Operator binds two profiles to different models
- **WHEN** the operator configures a supported runtime/provider and edits default and pro
- **THEN** each profile can save its own exact available model/thinking choice while both use the agent's shared skills and instructions

### Requirement: AI-agent deletion from its card
The Console SHALL offer Delete AI agent in the AI-agent card through the existing confirmed-command and generated REST mechanisms. Confirmation SHALL explain that profiles become unavailable for new selection while Run history remains retained. Cancellation and definitive failure SHALL leave the card intact; transport retry SHALL retain the original key and displayed version. Success SHALL refresh current catalogs, close the removed card and restore keyboard focus to Create AI agent. The action SHALL be localized and independent of CI/CD enrollment and provider configuration.

#### Scenario: Review and delete an AI agent
- **WHEN** an operator confirms deletion of the displayed AI agent
- **THEN** the original version/key is submitted, the removed agent no longer appears in current choices, retained history is unchanged and keyboard focus returns to the catalog control
