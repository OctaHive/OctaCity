# Factory Role Profiles Specification

## Purpose

Defines Project-owned Factory roles and reproducible execution profiles, including bounded manual, rule-based, and Decision API selection with durable stage-specific recovery and provenance.

## Requirements

### Requirement: Project-owned arbitrary roles and multiple profiles
The server SHALL allow each Project to publish arbitrary Factory Roles and multiple Role Profiles per role without a fixed business-role enumeration. A role SHALL define a display name, an uploaded bounded raster avatar, description, an exact bounded shared skill set, an ordered bounded set of shared instruction entries and an agent-wide positive concurrency limit. A profile SHALL bind one exact role version to its runtime, model, thinking level, additional ordered instructions, environment variables, CLI arguments, MCP servers, external integrations, tools, resource limits and installed execution capability. Existing single-text instruction content SHALL remain readable as one instruction entry, and older definitions missing presentation metadata SHALL use neutral defaults without rewriting retained versions. Both SHALL have stable identities, immutable positive versions, content digests, and editable display names. Replacement SHALL require the current-version precondition and SHALL preserve old versions. Console AI-agent cards SHALL use these same role identities and owned profiles without an additional AI-agent entity or duplicated instructions/skills. Profiles and roles SHALL NOT represent enrolled worker Agents, Agent Pools, or additional execution runtimes.

#### Scenario: Projects configure similarly named roles independently
- **WHEN** two Projects publish a role named `coder` with different model and skill configurations
- **THEN** each Project discovers and selects only its authorized definitions, and changes in one Project do not affect the other

#### Scenario: Project adds a specialized coder profile
- **WHEN** an operator publishes an additional frontend profile under a custom coding role
- **THEN** that profile is selectable without changing the Factory interpreter or introducing a new role enum value

#### Scenario: Role is replaced or renamed
- **WHEN** an operator publishes a new role version with changed instructions or display name
- **THEN** existing profiles continue to reference their original exact role version until explicitly replaced, and retained Runs preserve their selected versions

#### Scenario: Operator configures AI-agent identity
- **WHEN** an operator publishes a custom name, uploaded avatar, description and ordered shared instruction set
- **THEN** these belong to the existing role identity, and its profiles retain their explicitly pinned role versions

### Requirement: Starter templates are independent copies
The server SHALL provide release-versioned starter role templates for `coder`, `reviewer`, and `architect`. The coder template SHALL include `default` and `pro` profile templates, distinguishable as `coder` and `coder-pro`; the other starter roles SHALL include a default profile template. Creating definitions from templates SHALL copy their content and template provenance into new Project-owned versions. Operators SHALL be able to rename and modify the copies and create additional roles/profiles. Template replacement SHALL NOT update existing copies. Templates SHALL require resolution to compatible installed model, tool, and permission choices before publishing executable profiles; `pro` SHALL grant no implicit capability, credential, quality guarantee, or budget increase.

#### Scenario: Two coder configurations are initialized
- **WHEN** an operator creates the coder role from a starter template and resolves compatible installed choices
- **THEN** the Project receives independently versioned default and pro profiles whose model and reasoning settings can be configured separately

#### Scenario: Release changes a template
- **WHEN** a later release changes the coder-pro template
- **THEN** an existing Project's role/profile versions and admitted execution packages remain unchanged

#### Scenario: Template requires an unavailable model
- **WHEN** a template copy has not resolved a required model or skill capability available under Project policy
- **THEN** validation reports the unresolved dependency and the profile cannot be published as executable

### Requirement: Profiles contain complete bounded execution settings
A Role Profile SHALL bind exact installed runtime/harness/plugin/executable and model references, supported thinking level and model parameters, ordered role/profile instruction content, immutable skill bundles and supporting-file dependencies, bounded explicit public environment values, logical secret-environment references, ordered CLI arguments, authorized MCP and external integration bindings, declared tools and context requirements, resource ceilings and compatible ordinary Build configuration. Runtime/model capabilities SHALL define supported settings; no provider-specific global thinking-level enum or parallel runtime/connection registry SHALL be introduced. Secret values, ambient environment inheritance, shell-interpreted arguments and overrides of protected runtime/security options SHALL NOT be accepted as profile content. External bindings SHALL reuse existing Project authorization, Integration Connection, credential-purpose and installed tool authority. Unsupported parameters, unresolved dependencies, duplicate or unsafe materialization paths, cross-Project references, unverified digests, and unenforceable capability requirements SHALL be rejected before execution. A model alias SHALL NOT be represented as an immutable provider snapshot unless the installed capability supports that claim.

#### Scenario: Ordinary and pro use the same model
- **WHEN** two coding profiles select the same compatible model with different supported reasoning efforts
- **THEN** they retain distinct profile and effective configuration digests and can be selected independently

#### Scenario: Requested setting cannot be enforced
- **WHEN** a profile requests a reasoning setting or tool constraint unsupported by its installed harness
- **THEN** validation rejects the profile rather than silently dropping the setting

#### Scenario: Skill declares an unsafe supporting file
- **WHEN** a skill bundle includes traversal, a conflicting destination, a missing dependency, or content that does not match its retained digest
- **THEN** profile validation or materialization rejects it before starting the model task

#### Scenario: Profile includes runtime arguments and external tools
- **WHEN** an operator selects a qualified runtime with public environment values, secret references, CLI arguments, MCP servers and external integration bindings
- **THEN** normalized validation retains the supported exact settings and authorized bindings, and rejects unsupported or authority-widening options without exposing secrets

### Requirement: Custom skill folders become retained immutable bundles
An authorized operator SHALL be able to import one local skill folder through Console and its narrow Project-scoped REST/application operation. The import SHALL retain a root `SKILL.md` and reviewed supporting files under safe relative paths as one immutable versioned bundle with verified content digests. Imported and installed bundles SHALL use the same authorized profile-choice and retained-content mechanisms; no parallel file store or automatic dependency downloader SHALL be added. Imports SHALL use existing management audit, scoped idempotency and staged-content cleanup, and SHALL publish no partial bundle/catalog entry. Import SHALL enforce the shared path/file/byte limits, reject unsafe links or mismatched content, and SHALL NOT execute setup hooks, follow external URLs, inherit unrelated host files or retain absolute client paths. A successful import SHALL make the exact bundle selectable without granting execution authority.

#### Scenario: Imported folder is later changed locally
- **WHEN** an operator imports a folder with `SKILL.md` and supporting files, then edits or deletes the local folder
- **THEN** the saved bundle remains available by its exact version/digest, and only an explicit reimport creates a new version without updating existing profile or Run pins

#### Scenario: Import response is lost or a file fails verification
- **WHEN** a client retries the same accepted import after response loss, or one declared file fails validation before publication
- **THEN** an accepted operation replays its original bundle receipt, while a rejected operation exposes no partial bundle and staged bytes follow existing cleanup

### Requirement: AI-agent concurrency is enforced across profiles
The role concurrency limit SHALL bound active ordinary execution Builds for the same Project/role identity across profiles and definition versions. The server SHALL apply the current ceiling through existing Build capacity/reservation and fenced idempotent dispatch authority, extending that authority with a role scope rather than adding another scheduler. Reservations SHALL survive response loss/restart and SHALL be released once on authoritative terminal completion/cancellation. Reducing the ceiling SHALL block new starts without killing active Builds. Earlier definitions without this field SHALL retain inherited capacity behavior until explicitly replaced. Existing enclosing ceilings SHALL still apply; exhausted agent capacity SHALL wait with the retained profile rather than reselect, switch reserve or duplicate execution.

#### Scenario: Two profiles compete for one remaining agent slot
- **WHEN** concurrent starts for different profiles of the same AI agent compete for its last slot
- **THEN** at most one execution starts and the other waits for capacity with its original selected profile, including after owner restart

#### Scenario: Agent limit is reduced while executions are active
- **WHEN** the published agent concurrency ceiling is reduced below current usage
- **THEN** active executions continue, new starts wait, and terminal completion frees reservations exactly once

### Requirement: Execution packages preserve Project rules and explicit context
Before dispatch, the server SHALL retain a reproducible package combining pinned Project rules, the exact role/profile, complete selected skill content and dependencies, the stage task, and explicitly projected Context. Project-required rules SHALL remain applicable across profiles; role/profile instructions SHALL NOT waive them. Context requirements SHALL be satisfied only through existing authorized projections. Profiles SHALL NOT expose protected tests, hidden transcripts, sibling state, or credentials through skill materialization, instructions, or additional mounts. Effective permissions and budgets SHALL remain the intersection with the stage, Factory, Project, and enforceable runtime ceilings.

#### Scenario: Project requires test-first implementation
- **WHEN** a selected coding profile contains instructions that attempt to waive the Project's mandatory TDD rule
- **THEN** the compiled package preserves the Project rule as controlling instruction and does not grant the requested waiver

#### Scenario: Selected reviewer is installed
- **WHEN** a reviewer profile is materialized for a read-only evaluation stage
- **THEN** its exact review skills and instructions are present, candidate access remains read-only, and coding/delivery credentials are absent

#### Scenario: Profile requests hidden test context
- **WHEN** an implementation profile declares a context requirement for protected acceptance-test source
- **THEN** validation or dispatch rejects the unsatisfied requirement without adding that source to its package

#### Scenario: Execution resumes after source files change
- **WHEN** Project instructions, template files, or skill sources change after the invocation package is retained
- **THEN** recovery uses the original package bytes and digests rather than loading current files

### Requirement: Stages pin bounded selection policies and candidates
A model-backed stage SHALL be able to select an exact role version and a finite set of compatible profile versions from that role, with one policy in `manual`, `rules`, or `decision_api` mode. Policy inputs, criteria, candidate mappings, and failure behavior SHALL be included in the immutable Flow/configuration closure. Candidate validation SHALL check the stage's task/result schemas, required context, capture/deliverable policy, credential purpose, permissions, installed capabilities, and worst-case execution plus selection budgets. Selection SHALL choose a complete profile and SHALL NOT patch arbitrary model or skill fields. Existing statically bound stages SHALL remain supported without roles.

#### Scenario: Stage allows more than two profiles
- **WHEN** a stage pins default, pro, and a specialized profile from its selected role
- **THEN** every configured selection mode can choose among those exact compatible candidates without hardcoded default/pro behavior

#### Scenario: New profile is published after admission
- **WHEN** a Project publishes a newer profile after a Run pins its Flow closure
- **THEN** the Run continues to select only its original candidates, and the new version requires a new published configuration and admission

#### Scenario: Reserve cannot fit the stage budget
- **WHEN** a stage's configured reserve or primary candidate exceeds its enclosing resource or execution ceiling
- **THEN** publication rejects that binding rather than raising the stage budget during selection

### Requirement: Explicit manual selection and overrides
Manual mode SHALL support an exact configured profile and, when none is fixed, a durable request for operator selection. An authorized operator SHALL be able to supply an explicit stage-scoped profile override at Work admission or before that stage commits its Build dispatch intent. A valid override SHALL take precedence over automatic selection and SHALL select only from the stage's pinned compatible candidates. Admission overrides SHALL bind an exact declared stage address in the admitted Flow closure; live commands SHALL bind the exact Run/Flow/cycle/node occurrence or pending selection, current version, actor, and idempotency intent. Invalid explicit requests SHALL return a safe rejection without silently selecting another profile. Override acceptance and dispatch SHALL be serialized so that an override cannot mutate an already committed invocation.

#### Scenario: Operator overrides automatic selection at admission
- **WHEN** an authorized operator admits Work with an explicit eligible pro profile for one stage
- **THEN** that stage uses the exact pro profile without invoking the automatic selector, while other stages keep their own policies

#### Scenario: Future stage repeats
- **WHEN** an admission override targets a stage that executes in more than one cycle or bounded repeat
- **THEN** the override applies to that declared stage address across its bounded executions, and each execution retains an independently bound selection record

#### Scenario: Operator changes an undispatched selection
- **WHEN** an authorized operator chooses another pinned candidate before Build dispatch intent is committed
- **THEN** the server appends the superseding selection with its precondition and preserves the earlier selection history

#### Scenario: Dispatch wins a concurrent override race
- **WHEN** dispatch commits its immutable intent before a competing manual override
- **THEN** the override conflicts and cannot change the profile of that invocation

#### Scenario: Operator submits a foreign or unsupported profile
- **WHEN** a manual choice names another Project's profile, a newer unpinned version, or an incompatible profile
- **THEN** the request is rejected without dispatching a reserve or exposing hidden definition content

### Requirement: Deterministic criteria-based selection
Rules mode SHALL evaluate an immutable bounded ordered rule set over explicitly projected schema-validated stage inputs and accepted observations. Predicates SHALL use the existing bounded declarative comparison, existence, boolean, and collection operations without arbitrary executable code. The first matching rule SHALL select its exact pinned candidate. Missing required inputs, invalid types, evaluation-bound exhaustion, and no matching rule SHALL produce a typed selection failure; missing data SHALL NOT be treated as an ordinary negative comparison. An explicit catch-all rule SHALL be supported. Reasoning-generated observations SHALL remain observations and SHALL NOT become permission grants or accepted evidence by being used in a criterion.

#### Scenario: Criteria select pro
- **WHEN** an accepted complexity observation is `high` and the first matching configured rule maps it to the pro profile
- **THEN** selection retains the rule identity, observation/input digest, policy version, and exact pro profile

#### Scenario: Multiple rules match
- **WHEN** two ordered rules match the same complete input
- **THEN** the first matching rule deterministically selects the profile and replay yields the same result

#### Scenario: Required criterion is unknown
- **WHEN** the stage's required complexity observation is absent
- **THEN** the selector records a missing-input failure and invokes the stage's configured failure policy rather than assuming ordinary complexity

### Requirement: Decision API selection uses a separate purpose-specific consumer
Decision API mode SHALL submit a bounded redacted context, versioned semantic criteria, and a finite mapping to the stage's exact allowed profiles through the provider-neutral Decision bridge. JEV SHALL be the initial compatible adapter; another installed decision system SHALL be replaceable through versioned configuration. The profile-selection consumer SHALL independently validate input binding, provider capability, answer domain, calibrated confidence/margin when applicable, compatible profile membership, remaining budget, and current stage eligibility. The bridge SHALL NOT be a universal decision engine: Factory routing and profile selection SHALL retain distinct contracts, criteria, calibration, consuming policies, and failure behavior.

The selected Decision Signal policy SHALL retain existing rollout semantics: `shadow` and `advisory` SHALL use an explicitly pinned deterministic baseline profile, while only qualified `bounded_control` SHALL allow the accepted provider recommendation to change the choice. Provider error, timeout, malformed/undeclared answer, inadequate qualified confidence, or unobservable outcome SHALL be selection failures under the stage policy when a provider decision is required. Provider output SHALL NOT create a profile, alter its configuration, change a Flow edge, or complete an execution gate.

#### Scenario: JEV selects an allowed coding profile
- **WHEN** the configured provider returns a valid finite pro choice meeting the profile-selection policy in bounded-control mode
- **THEN** the profile consumer records the provider receipt and chooses the pinned pro profile, subject to current stage checks

#### Scenario: Another provider is installed
- **WHEN** a later configuration selects another adapter qualified for profile selection
- **THEN** later Runs use that adapter without changes to profile selection semantics, while earlier Runs retain their original provider and receipts

#### Scenario: Routing qualification is reused incorrectly
- **WHEN** an installed model is qualified only for routing and lacks profile-selection capability or calibration
- **THEN** validation prevents its use for controlling profile selection rather than inheriting routing qualification

#### Scenario: Selection is observed in shadow mode
- **WHEN** the stage has a pinned baseline profile and a shadow Decision API policy
- **THEN** the baseline is selected and the recorded provider recommendation does not change execution

### Requirement: Selection failure behavior belongs to the stage
Every profile-selecting stage SHALL declare exactly one selection failure policy: `reserve_profile`, identifying one pinned compatible candidate, or `manual_selection`, requiring a durable wait for an operator. The same policy SHALL cover all typed automatic-selection failures. A usable reserve SHALL be selected once with the original failure retained; an absent or unusable reserve SHALL result in manual selection without another automatic fallback or implicit model substitution. Failure policy SHALL NOT bypass Project policy, unavailable enforcement, required gates, cancellation, expired deadlines, or exhausted hard budgets. Terminal eligibility failures SHALL preserve the existing terminal behavior rather than create an actionable wait.

#### Scenario: JEV times out and reserve is valid
- **WHEN** a Decision API selection times out and the stage declares an eligible reserve profile
- **THEN** the stage retains the timeout reason and uses that exact reserve without increasing its budget

#### Scenario: Rules cannot choose and manual recovery is configured
- **WHEN** no rule matches and the stage declares manual selection on failure
- **THEN** the stage persists a pending selection and dispatches no coding/review Build until a valid operator choice is accepted

#### Scenario: Reserve becomes unavailable
- **WHEN** the pinned reserve cannot pass current installed-capability or permission checks before dispatch
- **THEN** the stage waits for manual selection among still-eligible pinned candidates without a recursive fallback chain

#### Scenario: Run is cancelled while waiting
- **WHEN** the Run is cancelled or its hard deadline expires during a pending manual selection
- **THEN** normal cancellation/deadline behavior applies and a later choice cannot revive the stage or replenish budgets

### Requirement: Manual selection waits are durable and bounded
A pending profile selection SHALL expose the exact target, safe reason, pinned eligible candidates, observed Run version, and applicable deadline. A choice SHALL be an authorized idempotent control intent, consumed only by the current fenced Flow owner. Competing requests SHALL yield at most one accepted current intent; exact replay SHALL return the original receipt after response loss or later progress. Waiting SHALL NOT start an execution Build, run a model repeatedly, reset deadlines, or consume new execution attempts merely because the server polls or restarts.

#### Scenario: Server restarts while awaiting a choice
- **WHEN** a new owner restores a Run with a pending manual selection
- **THEN** the same pending target and candidates remain available without another automatic provider query or Build

#### Scenario: Two operators choose different profiles
- **WHEN** two operators submit different choices against the same current selection precondition
- **THEN** at most one intent is accepted and the other conflicts without changing the selected invocation

#### Scenario: Manual response is lost
- **WHEN** an operator repeats the same accepted choice with the same idempotency key after losing the response
- **THEN** the original receipt is returned without a second selection or dispatch

### Requirement: Selection provenance is immutable and replayable
Every selection SHALL retain Project, role/profile versions and digests, Flow/Run/cycle/node attempt and exact subject, selection-policy/input/criteria digests, method, actor for manual input, matched rule or provider receipt when applicable, failure/reserve reason, and final compiled execution-package identity. Selection history SHALL distinguish recommendation, accepted intent, chosen profile, dispatch, and execution result. Restart and transport replay SHALL reuse retained inputs, decisions, and ordinary Build operation identities; they SHALL NOT reselect from the latest catalog, ask a different model, or duplicate dispatch. Sensitive instruction/skill/provider bytes SHALL remain in explicitly authorized retained content rather than generic audit or diagnostics.

#### Scenario: Owner loses the dispatch acknowledgement
- **WHEN** an ordinary Build was accepted with a chosen profile but its response was lost
- **THEN** takeover recovers the same selection and Build operation instead of choosing a profile or creating a Build again

#### Scenario: Model execution fails after successful selection
- **WHEN** a Build fails after a valid profile was selected and dispatched
- **THEN** normal execution retry/rework policy applies with the recorded binding and selection failure policy does not silently upgrade to pro

#### Scenario: Package identity is substituted
- **WHEN** materialization or recovery supplies model, skills, instructions, or runtime parameters inconsistent with the retained selection
- **THEN** the invocation is rejected before execution or result acceptance

### Requirement: Shared agent skill ownership and retained browsing
Shared skill references SHALL belong to the exact Role version used by every profile. Assembly and prompt rendering SHALL use the same resolved skill closure without copying shared selections into new profile definitions. Historical profile additions SHALL remain readable; an alias conflict with shared skills SHALL fail rather than silently substitute content. Dedicated Project-authorized reads SHALL expose the retained file inventory and an explicitly selected bounded UTF-8 preview through the existing import/object reader, checking exact bundle membership, byte integrity and current Artifact download authority. Text SHALL NOT execute as Markdown, HTML or scripts. Avatar publication SHALL validate complete bounded PNG content through the common raster mechanism; external URLs and active image formats SHALL be rejected.

#### Scenario: Default and pro use one shared skill set
- **WHEN** two profiles pin the same agent version with imported skills
- **THEN** both materialize the same retained shared files without separately editable profile copies, and restoring a retained invocation preserves that closure

#### Scenario: Operator reads an imported file
- **WHEN** an authorized operator selects an exact skill version and one retained relative file
- **THEN** the response contains that version's bounded verified text only after current content authority succeeds; foreign Projects, absent paths and substituted bytes are unavailable

### Requirement: Plugin-owned models and profile binding
Installed plugins SHALL supply model suggestions and supported parameter/thinking contracts through the existing runtime capability boundary. Suggestions SHALL be advisory, names SHALL NOT be hardcoded in OctaCity, and an operator SHALL be able to enter an unlisted model identifier manually. Where supported, explicit bounded refresh SHALL query the installed runtime without model execution; ordinary reads and validation SHALL use cached metadata. Unavailable discovery SHALL be explicit and failed refresh SHALL preserve the previous usable catalog. Profiles and execution packages SHALL retain exact selected model/parameters through refresh and restart. Runtime rejection SHALL NOT select another model automatically. Existing executable, credential, permission, and exact pricing restrictions SHALL remain enforced. AI-agent identity/instructions/shared skills and manual selection SHALL remain independent of discovery availability.

#### Scenario: A new model is absent from suggestions
- **WHEN** an operator publishes an unlisted model using a registered runtime with manual model support
- **THEN** the selected identifier and supported parameters are retained and passed to that runtime without replacing the model from current suggestions

#### Scenario: Discovery is refreshed independently
- **WHEN** an operator explicitly refreshes the registered runtime catalog
- **THEN** new authoring suggestions are available without changing published profiles or prior invocation identities; a failed refresh preserves the previous usable catalog

### Requirement: AI-agent deletion preserves retained history and prevents new use
The server SHALL delete an AI agent by marking its existing Role unavailable for new use. It SHALL preserve immutable Role/Profile versions, admitted Flow pins, execution packages and Run history under their existing authorization and retention contracts. Current catalogs and current-alias reads SHALL omit the agent and its profiles. New publication SHALL NOT resurrect the identity. The common candidate qualification SHALL reject its profiles for new automatic, configured, rule and reserve selection. Live manual selection SHALL reject a deleted role from the admitted binding, including an embedded profile without a separate catalog publication. Deletion SHALL NOT cancel already accepted Builds. Deletion SHALL use current-version fencing and atomic scoped replay/audit/outbox through the shared Project publication lock; successful historical replay SHALL retain its original result.

#### Scenario: Delete an agent referenced by a retained Run
- **WHEN** an authorized operator deletes the displayed current Role version
- **THEN** current choices exclude the agent, new profile choices are rejected, and the Run history and authorized exact-version reads retain their original content

#### Scenario: An old picker submits a choice after deletion
- **WHEN** a profile choice for a deleted role reaches the ordinary live-control transaction
- **THEN** no control or Run-history change is committed, including for an embedded profile, while a previously accepted command can still replay its original receipt

### Requirement: Project administrators configure installed Runtime choices
The server SHALL provide Project-scoped authorized reads and reviewed publication of Runtime settings bound to an existing installed Runtime identity. Settings SHALL define named supported MCP transports, exact existing Project-authorized integration bindings and finite permission choices through the existing execution-setting and permission contracts. Installation-owned executable/plugin identities, supported settings and independent backend/Agent ceilings SHALL remain authoritative. Configuration SHALL NOT install arbitrary executables, create another Runtime or connection registry, grant integration access, accept credential values or silently widen execution authority. Supported MCP transports SHALL use structured endpoint or executable/argv fields and explicit environment names without shell interpretation or ambient inheritance. Invalid, unsupported, foreign or unauthorized selections SHALL fail safely before publication; configuration and reads SHALL perform no model execution or remote MCP/provider operation.

#### Scenario: Administrator configures a Runtime for profile authoring
- **WHEN** a Project administrator publishes a valid named MCP server, existing authorized integration and supported permission choices for an installed Runtime
- **THEN** that Project discovers those choices through its ordinary profile capability contract without changing another Project or installing a second Runtime

#### Scenario: Configuration exceeds installed authority
- **WHEN** a request replaces a protected executable, includes a credential value or requests unsupported settings or authority outside installation/backend constraints
- **THEN** the server rejects it without partially publishing the settings or silently weakening the requested constraint

### Requirement: Runtime settings retain reviewed revisions and mutation outcomes
Runtime settings and referenced MCP transport content SHALL have immutable retained revisions and verified exact identities. Replacement SHALL require the displayed current-version precondition. Publication SHALL atomically retain its original scoped mutation result, audit and outbox facts through the existing management mechanisms. Reads and publication SHALL require current Project authorization, including replay. Concurrent replacements SHALL have at most one winner for one precondition, and a response-loss retry SHALL recover its original result after restart or later updates. Private transport content SHALL remain in authorized content reads rather than generic audit or discovery summaries.

#### Scenario: Publication response is lost and another update follows
- **WHEN** an accepted publication is retried with its original body and scoped key after restart or a later settings revision
- **THEN** the original receipt is returned without another version or duplicate audit/outbox effects, and a different body under that key conflicts

#### Scenario: Administrators edit the same settings revision
- **WHEN** two different reviewed replacements compete for the same current version
- **THEN** at most one is applied and the other receives a safe conflict while prior revisions remain readable under authorization

### Requirement: Runtime configuration feeds shared profile and compilation contracts
Profile discovery, details, validation and compilation SHALL resolve Runtime configuration through the same authoritative capability mechanism. Profiles SHALL select exact MCP and integration bindings rather than embedding transport definitions or inheriting future defaults. The existing compiler/materializer SHALL resolve the selected verified retained transport content, enforce the current integration grants and independent execution ceilings, and pass only explicitly selected settings to the plugin. Replacing Runtime defaults SHALL NOT rewrite published profiles or retained execution packages. Revocation or removal SHALL block newly unauthorized use without substituting another server, integration, model or profile. Unconfigured Runtime settings SHALL preserve existing installation behavior, static profiles and manual Factory intake.

#### Scenario: Profile uses a configured MCP server
- **WHEN** an authorized operator selects configured MCP and required integration bindings and the qualified ordinary execution is compiled
- **THEN** the plugin receives the exact selected transport/settings with only approved environment mappings and no unselected MCP server

#### Scenario: Settings change after an invocation is retained
- **WHEN** a Runtime setting is replaced before recovery of an earlier retained invocation
- **THEN** recovery resolves its original verified content rather than current defaults, while current authorization and revocation checks still apply
