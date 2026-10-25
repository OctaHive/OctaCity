# Operating optional Dark Factory

Ordinary CI/CD is the default deployment mode. Factory adds Work admission and
an immutable Flow owner over ordinary Builds; it does not install a second Job
executor. Use this guide alongside [server operations](server.md),
[Agent operations](agent.md), and the [local stand](local-stand.md).

The separate [OpenAI Decisions shadow guide](openai-decisions-shadow.md)
describes the optional adapter, private installation files and deferred live
experiment. Installing it does not enable Factory or qualify a model.

## Choose and observe the mode

| Observation | Meaning | Operator action |
| --- | --- | --- |
| `/health/ready` succeeds and `/health/factory` reports `disabled` | Ordinary CI/CD is available; no Factory owner is installed | Use ordinary Builds, or deliberately prepare an opt-in installation |
| Both readiness scopes succeed | Shared dependencies and installed Factory capabilities are healthy | Check the exact Configuration and qualified Agent inventory before admitting Work |
| Ordinary readiness succeeds; Factory reports `unavailable` | An optional Factory worker or installed capability is degraded | Investigate that capability; keep ordinary CI/CD available |
| Shared readiness fails | A required shared dependency is unsafe for mutations | Follow the server incident runbook before admitting either mode |

Health is an observation, not a permission grant or capacity reservation. A
healthy model endpoint does not qualify an Agent/backend, validate a Flow, or
prove that a candidate passed required verification.

## Prepare an explicit installation

The server requires an operator-owned `factory_profiles_file` or an explicit
trusted runtime installer, together with bounded `[factory]` worker settings.
Install only exact profiles and capabilities needed by the published Flow.
Unknown configuration, invalid bindings and missing required authority fail
validation. An installation timeout can leave Factory unavailable while ordinary
server startup continues; recovering missing installation material in that
initial state requires a server restart.

The [runtime installation contract](../architecture/server.md#optional-factory-runtime-installation)
defines the profile catalogue and composition boundaries. The
[local-stand procedure](local-stand.md#optional-factory-installation) shows the
supported private-file inputs. Server-side Signal connectors use a separate
private catalogue and credential file. Never put a provider key, private URL,
raw provider payload or mutable model alias into a Flow or management command.

Keep authoring, implementation, protected validation, review, verification,
delivery and promotion profiles independently scoped. Install the pinned Codex
and task/source plugins, exact question/model identities, read-only inputs,
permission ceilings, original deadlines and resource/output limits. A role name
does not create authority: the installed compiler, profile, local Agent policy
and backend must all agree. Delivery and promotion credentials must not enter
implementation, protected-test authoring or ordinary source reads.

### Project AI-agent compatibility declarations

An optional `role_profiles` map inside each `factory_profiles_file` Project entry
attaches authoring compatibility to existing `node_capability` aliases. Each alias
must already name the exact runtime executable, and its ordinary Build configuration
must belong to that same Project. Existing files omit this map and keep their
previous behavior. Programmatic installers use the same
`FactoryConfigurationChoices::with_role_profile_capability` contract.

Each declaration contains:

| Field | Content |
| --- | --- |
| `ceiling` | An ordinary complete Role Profile definition declaring the exact toolchain, approved argv, skills, MCP/integration bindings, credential purpose, permission and budget ceilings |
| `parameters` | An existing content-addressed `FlowDataSchema` declaring supported model parameters and thinking values |
| `public_environment` | A bounded map from exact environment names to `FlowDataSchema` value contracts |
| `enforcement` | The existing backend permission categories that this installation can enforce |

The optional `ceiling.thinking_parameter` names the one entry in
`model_parameters` whose scalar value is safe reasoning metadata. A profile may
select that same key through its optional `thinking_parameter`; it must reference
an existing parameter and pass the installed capability check. The value remains
in `model_parameters` and participates in the ordinary profile digest. No provider
parameter name is inferred. Profiles omitting the annotation keep their existing
wire representation, digest and compatibility, and publish no thinking metadata.

Public environment values must match their declared contracts; they cannot replace
approved secret entries. Secret entries select only exact installed logical
references. The argv vector must match the installed approved vector, and requested
permissions must survive the existing local permission intersection unchanged.
These declarations install no executor and confer no execution or credential grant.
Names and instructions remain editable Project content rather than installed
compiler instructions.

In Console, the profile editor selects the installed MCP servers, Project integration
bindings and permission categories from this filtered ceiling. MCP entries identify
the integration they require; select that integration as well. Review validates the
exact selected pins through the ordinary profile contract. These checkboxes do not
register a new MCP transport or authorize a connection. When a category has no
available entries, the editor explains the missing configuration instead of showing
an empty selection frame. A deny-all Runtime displays one permission explanation
instead of zero-only controls. CLI arguments remain the installed fixed vector;
public environment fields and logical secret selections appear only when declared.

Application catalogs require visible runtime/model and Build dependencies before
visible counts and cursors. Unavailable optional skills, MCP servers, integrations
and permission selections are removed from the available ceiling, without hiding
the runtime. An MCP selection tied to an unavailable integration is removed too.
Catalogs and dedicated details share this filtered ceiling. Validation checks the
unchanged candidate against it: omitting an unavailable option is valid, while
explicitly requesting it is rejected rather than silently narrowed.
Managed resources remain in the existing Integration
Connection and Project-grant owner: active connection metadata, exact connection and
access revisions, granted resource/purpose and configured credentials are rechecked
without contacting the provider or resolving secret values. They need no duplicate
resource entries in the Factory capability catalog. General collections expose safe
metadata and tool identities; approved argv, command patterns, logical secret
locators and instructions require dedicated authorized content reads.

Role/profile and starter-template catalogs remain available without an enrolled
worker or enabled Factory execution. A normalized compatibility preview retains the
exact role and candidate digest without publishing a version, reserving a mutation
key, or starting a Build. Executable admission and dispatch still require their
ordinary installed compiler, Project and backend authority.

### Reviewed Project Runtime settings

`PublishFactoryRuntimeSettingsCommand` and `GetFactoryRuntimeSettingsQuery` use
existing Project administration/content authorization and the ordinary role/profile
handlers. Their operation-shaped store port has memory and PostgreSQL adapters.
REST authoring and the separate Console AI settings view are available. Publishing these
records does not change installation inventory or start execution.

Settings belong to an existing Project/Runtime alias and retain exact toolchain
pins, named structured MCP entries, existing integration selections and a finite
permission set. They do not copy a whole profile or the model-discovery cache.
Programmatic installers attach `FactoryRuntimeSettingsSupport` to the existing
choice catalog, declaring supported MCP transport families and qualified stdio
paths/executable pins. These private paths are not generic discovery metadata.
Without that declaration, new MCP transports cannot be configured. Installation
file and local-stand wiring remain separate from these application contracts.

Qualification reuses the existing permission intersection and managed integration
checks. HTTP endpoints require an explicitly permitted canonical host; embedded
URL userinfo and undeclared bearer environment mappings are rejected. Stdio
requires an installed exact path, permitted executable/argv and declared environment
names. Configuration does not resolve credential values, launch commands or contact
MCP/model/provider endpoints. The common MCP transport/pinning contract and server-name
validation are also used by the existing Codex adapter, preserving its wire format.

Publication retains immutable positive revisions and verified content digests, with
current-version replacement, scoped idempotency and shared audit/outbox evidence.
An exact retry returns its original receipt before mutable capability lookup while
still requiring current Project authorization. Historical reads retain original
transport content after replacements; no latest defaults are substituted. The
shared memory/PostgreSQL journal test contract covers these persistence semantics.

PostgreSQL migration 0104 adds one append-only `factory_runtime_settings` table,
keyed by Project, Runtime alias and revision. Publication uses the existing Project
row lock and scoped mutation transaction, committing the revision, original receipt,
audit and outbox together. Reads verify the core content digest and its row identity.
Audit/outbox payloads contain only Project/Runtime/version/digest metadata, not MCP
transport content. An interrupted transaction leaves no settings or replay reservation;
a replacement retry through a new connection returns the original historical result,
even after later settings changes.

The common capability read overlays reviewed settings before authorization filtering.
Profile discovery, details, validation and admission therefore use the same configured
MCP/integration pins and narrowed permission choices. Runtime administration reads the
independent installation ceiling, allowing a later reviewed update to restore choices
within that ceiling. Publication checks the complete derived catalog under the same
Project lock, so competing Runtime saves cannot overflow its shared choice limit.

Changed toolchain pins require a new review before that Runtime is selectable again.
Withdrawing transport support or the authority required by its reviewed settings removes
configured MCP choices from new selection; ordinary profile reads also filter current
integration access. These checks do not delete the original settings or change retained
profiles. Dedicated authorized settings reads continue to expose their historical content.

Changing an integration alias removes installed MCP choices pinned to its previous exact
binding; it never rebinds those transports implicitly. Configured replacements retain their
own reviewed pins. Shared profile qualification requires every environment name declared
by a selected configured stdio transport or HTTP bearer mapping. Unselected transports add
no environment requirements, and missing mappings are rejected before execution rather
than inherited from the host.

The native materializer and output verifier share one batched exact MCP lookup alongside installed
transports. PostgreSQL reads candidate settings once per batch, filtered by the selected
revisions and server names. The lookup checks Project, revision and complete immutable reference, including
the content digest; it never substitutes the current settings. Replacing or removing an
entry leaves prior transport content available for retained invocation recovery. Only
profile-selected servers and explicitly mapped environment names reach the plugin;
dispatch still rechecks managed integration access and independent execution grants.
Static profiles and manual Factory intake keep their existing path when no settings apply.

Runtime HTTP authoring uses the existing optional role/profile facade:

- `GET /api/v1/projects/{project_id}/factory-runtime-choices` requires Project
  Administer authority and accepts the ordinary `limit`/`after` alias pagination.
  It exposes qualified transports/executable choices, independent permission ceilings
  and environment names, never environment values or logical secret locators.
  Existing Integration Connection, resource and Project-access endpoints remain the
  integration selection catalog; Runtime configuration creates no connections or grants.
- `GET /api/v1/projects/{project_id}/factory-runtimes/{alias}` requires Project View
  and returns `{ "settings": null }` when unconfigured. The `/versions/{version}`
  read returns exact historical content, or 404 when that revision is absent.
- `POST .../factory-runtimes/{alias}/validate` requires Project Administer and accepts
  `{ "expected_version": null, "definition": { ... } }` for creation, or the displayed
  positive revision for replacement. It returns normalized candidate `settings`
  through the same installed-limit/integration qualification as save, without writing
  a revision, reserving a replay key, refreshing models or contacting an MCP service.
- `POST .../factory-runtimes/{alias}` saves that same reviewed body with
  `Idempotency-Key`. A stale body revision returns 412; different intent under an
  accepted key returns 409. Retrying the original body/key returns its original
  receipt after later updates and still requires current authorization. Final version
  and aggregate catalog-capacity checks remain atomic at publication, so review is
  not a reservation. Every successful content/setup response uses `Cache-Control: no-store`.

OpenAPI reuses the existing toolchain, Integration binding and permission schemas,
and describes bounded HTTP/stdio MCP rows. The Console Runtime API client uses generated
types from these contracts and adds no provider rules, fixed model list or implicit retry.

Console **AI settings / Настройки AI** has **AI agents** and **Runtimes** views under
an explicit selected Project. Its Project chooser supports hierarchy and pagination;
the Project name links back to its ordinary workspace. The Factory AI-agent shortcut
opens this same Role-backed catalog and profile editor. Worker Agent/Pool management
remains in the Agents section.

In Runtimes, select an installed alias and configure named HTTP/stdio MCP rows.
Stdio selects an installed executable, separate argv entries and approved environment
names; HTTP bearer authentication selects a logical secret environment name, never a
credential value. Optional tool names remain a finite explicit list. Integration
bindings select existing connection metadata and currently granted Project resource/
capability pins, with a link to existing integration access administration. Removing a
binding does not silently detach its dependent MCP: the server must qualify the edited
configuration again. Permission controls reuse the common editor, display installed
numeric maxima and prevent new selections in unenforceable backend categories; existing
unsupported selections may be removed. Unsupported resource limits explain the backend
restriction while a positive retained value remains; clearing it removes the zero-only
control. Saving cannot widen independent execution grants.

If installation pins change, explicitly adopt the current Runtime installation before
review. This updates the settings draft, without rewriting existing profiles or executions.
Review first calls the read-only validation contract, then the ordinary confirmed-command
dialog publishes that frozen request. On conflict, explicitly refresh the current version
and review again; the MCP, integration and permission draft remains in component memory.
Transport retries use the same scoped key through the common command helper. Draft contents
are not written to URLs or browser storage. Local-stand installation wiring remains task 4.1.

In the profile editor, **Configure Runtime / Настроить Runtime** opens the Runtimes
view under the same Project. Return through **AI agents** (or browser Back) to refresh
the available capability choices. The mounted profile keeps its unsaved name, model,
instructions and selections; newly configured bindings are not automatically adopted.
An MCP dependency names the integration that must be selected separately. Checkboxes
select exact configuration/integration pins, not whichever revision now has the same
alias. Removed or changed selections remain in the draft with an explanation and an
explicit removal action; selecting an available replacement replaces only that binding.
The shared permission editor also retains removed tool/plugin/executable pins, logical
profiles, network hosts, mounts, command permissions and output kinds as checked values
marked unavailable. They can be removed individually; removed values cannot be newly
selected without becoming available again. This does not reset the model, parameters or
instructions. The ordinary profile validation/publication contract still checks current authority.
Empty MCP/integration categories and deny-all permissions show guidance rather than
empty frames or editable zero-only limits. The setup view explains installed support;
capability/authorization failures use the shared error states rather than an empty-list
message. Integration access/resource read failures do not also claim that the grant list
is empty; that guidance requires successful reads. When browser Back hides a profile
confirmation, its keyboard capture is suspended. Returning to AI agents restores the
same reviewed confirmation; it is not published or discarded by switching views. Setup
neither publishes the profile draft nor changes historical profiles.

### Reproducible AI-agent execution packages

`FactoryRoleExecutionPackage` assembles an explicit core snapshot from the existing
Task Envelope, Context Manifest, exact role/profile content, ordered Project rules
and selected skill inventories. Project rules reference `policy` inputs in that
same Context Manifest and must match their exact UTF-8 digest and size. Rule,
shared-role and profile instruction layers remain separate, with Project rules
controlling; this records precedence rather than proving that a model obeys text.
All runtime/model parameters, environment/CLI, MCP/integration bindings, technical
pins and stage task/output contracts participate in the versioned package digest.

`FactoryRoleSkillBundle` pins a canonical relative-path inventory using existing
Artifact references, including root `SKILL.md` and supporting files. It reuses
Context file/byte bounds and rejects file/directory conflicts. The package requires
exactly the profile's selected bundle references, the original role and Context
pins, required Context entries and matching task toolchain. Task permissions and
budget may only narrow the selected profile. Shared instruction bounds and the
common 1 MiB document bound apply; restoration repeats validation and rejects
unknown contract versions or substituted content.

Assembly performs no catalog, filesystem, credential or provider lookup and grants
no execution authority. Skill/task file bytes remain under their existing Artifact
owner. Project skill-folder import and protected-input preparation are implemented
below; the ordinary compiler integration is described next, while profile/Run
retention remains task 3.5. Callers still qualify
current installed capabilities and authorized inputs before executable admission.

### Project skill-folder imports

`ProjectSkillImport` accepts an explicitly reviewed regular-file set and expected
SHA-256 values under Project administration authority, without Factory execution
permission. It requires root `SKILL.md`, retains every declared supporting file,
and rejects links, duplicate/unsafe paths, file/directory conflicts and changed
reviewed bytes before reserving objects. Existing Artifact/Context bounds apply:
nonempty files up to 16 MiB each, at most 256 files and 64 MiB in total, with
portable relative paths. No workstation directory or setup hook is opened.

`ProjectSkillImportStore` retains metadata and receipts in memory contracts and
PostgreSQL. Bytes use `ArtifactBytePublisher`/`ArtifactByteReader` and the configured
object backend. Reservation records the complete cleanup inventory before I/O;
only the complete verified tree becomes visible in the bounded Project catalog.
Project and accepted management security scope partition replay. The same key and
reviewed tree recover the original receipt; changed intent conflicts. Explicit
reimport with a new key creates another immutable version and preserves earlier
references. Pending retries resubmit the exact reviewed bytes.

Staging uses the shared five-minute Project input preparation deadline. Expired
imports cannot publish or renew their deadline. The existing private-input cleanup
worker claims their objects with a durable fence before deletion and retries
failed deletions. Its shared recheck interval also removes late writes from an
interrupted publisher. Published bundles are excluded from abandoned staging
cleanup; profile/Run/Build retention and holds are integrated in task 3.5.

The trusted compiler can prepare selected complete bundles through `materialize`.
It checks Project ownership, exact manifests, aggregate Context bounds and all
protected destinations before reading files, then verifies every retained byte
identity. Missing or substituted content fails without an ambient/global fallback.
The resulting unchanged `ProtectedInputManifestV3` uses the existing Artifact input
transfer and Agent materializer, with no provider URL or new file backend. Import
creates no Work, Build or executable grant. REST uses this same import owner
(see the Project skill routes below); Console offers reviewed folder import
inside the open profile draft.

### Role packages in ordinary Build compilation

The version-1 `factory_profiles_file` separates executable installations from
Project authoring choices. Within `projects`, optional `role_profiles` entries
are keyed by an existing node-capability alias and contain `ceiling`, `parameters`,
`public_environment` and `enforcement`. `ceiling` is the complete ordinary profile
definition, with exact toolchain and Build Configuration pins; the alias resolves
its installed executable. The selected Build Configuration must belong to that
Project. Parameter/environment values use the existing closed Flow schemas.
`enforcement` declares the supported local permission categories.

These entries constrain authoring and validation; they do not install Codex,
resolve credentials, start a worker, or grant a permission. Exact runtime/model and
Build availability are required. Unavailable optional skills, tools, MCP or
integration choices are removed from the available ceiling; selecting an unavailable
entry is rejected. External connections reuse existing Project integration access
and credential purposes. User-authored instructions never extend that ceiling.

Install each actual compiler/runtime independently and qualify its supported
settings. The current pinned Codex adapter supports its model/reasoning fields,
explicit public environment and installed logical secret mappings. Octa 0.7.0
accepts the additive flags `--ephemeral`, `--skip-git-repo-check` and
`--color=never|always|auto`. Arguments must be unique and select at most one
`--color=` value; conflicting flags are rejected during compilation, before
execution inputs or an ordinary Build are published. Installed MCP transports
live in the Codex entry's `mcp_configurations` and are selected by content-pinned
profile references. MCP aliases use 1–64 ASCII letters, digits, `_` or `-`. Each transport
allows up to 64 distinct `enabled_tools`, whose names contain 1–256 UTF-8 bytes
without surrounding whitespace or control characters. An empty tool list remains
valid. Stdio `env_vars` contains at most 64 distinct names; every name must also
be explicitly mapped by the selected profile. These list constraints are checked
when constructing or validating installed transport metadata. Compilation also
checks aliases and explicit profile environment mappings. A transport can bind
one exact existing Project integration grant; the grant and connection
administration are rechecked before dispatch, including retained-package replay.
MCP receives only environment names explicitly declared
by that profile; the implicit model credential is not inherited. A declaration
for another adapter does not install its runtime support.
Keep the installation's exact identities and retained old bindings available for
existing Runs. Roll back by preventing new role-selected admissions and draining or
cancelling them with a compatible binary; preserve retained packages/history and
do not feed live new-format Runs to an older decoder.

An installed materializer can opt into `FactoryRoleInvocationSource` to obtain one
selected immutable package. An unselected exact installed static task bypasses the
source, so its storage availability cannot block static Builds. Selected profiles
always require their retained package, even when their runtime matches a static
installation; a missing package cannot authorize a fallback.
The source must use existing capability qualification, authorize dependencies and
read exact retained content; it cannot choose current catalog defaults. `ProjectSkillImport.prepare_invocation` composes the existing
skill reader with the common invocation contract, including every supporting file
under `/octacity/protected/skills/<alias>/`. Missing, extra or substituted file
identities are rejected. Credential mappings contain selected logical references
and installed variable names, never secret values or a new credential resolver.

Native `FactoryRuntimeResources::codex_executor` supplies
`FactoryStoredCodexRoleInvocationSource` for the first readonly observation package
and retains it through the common input journal. It reads exact historical Role,
Profile and imported-skill versions plus the explicitly selected Context. Replay
uses the retained package rather than current catalog heads. This stock source
supports readonly review and tool reasoning, and supplies no additional secret
variable mappings; unsupported authoring or secret selections fail closed.
Released-process qualification is tracked separately in task 9.1.

`factory_role_task_digest` binds the installed task policy to exact role/profile
references. Matching includes the ordinary Build configuration and full
plugin/executable/model pins, so sharing a runtime/model cannot reuse another
profile's invocation. This configuration identity avoids a digest cycle with the
immutable Flow. The separate package digest covers exact per-node rules, task,
Context and supporting content. Compilation verifies the Node Attempt, Context,
ordinary binding and the exact effective task permissions/budget before producing
an executable program. Selection must supply an intent with those effective limits.

Package construction and restoration require the complete selected Context
`Policy` inputs in the controlling rule layer, with exact text digests and sizes;
omitting one or all selected policies is rejected. Other Context kinds remain data,
and no ambient Project policy is discovered at compilation time.

The existing Codex compiler preserves ordered controlling Project rules, shared
role instructions, profile additions and installed stage instructions. It emits
native model/reasoning settings, explicit public variables and qualified secret
variable mappings and qualified additive CLI/MCP settings. Unknown model settings,
protected environment overrides and unresolved credential mappings fail before
publication. Explicit runtime settings map child `HOME` through a compiler-owned
variable to `/workspace/scratch`, so Octa's ambient `HOME` overlay cannot replace
it. Existing static task documents keep their retained representation. The qualified
Agent/Microsandbox installation must exclude ambient Codex configuration;
`--ignore-user-config` alone does not remove every configuration source.
These restrictions belong to the adapter, rather than a provider enum in core.

The canonical package is another protected input at
`/octacity/protected/role-execution-package.json`; its digest is retained in the
internal Task Envelope. Imported files keep their existing Artifact identities and
are merged into the ordinary protected-input manifest without copying their bytes.
The compiler reserves room for generated documents under the shared transfer
limit before publication. Existing fenced publication, ordinary Build assembly,
permission intersection and JobSpec v3 validation remain authoritative. Static
invocations bypass the role source and keep their previous documents and identities.

Internal Task Envelopes additionally support `observe` with `flow_v1` for ordinary
Flow observations; the immutable Flow outcome supplies the actual result schema.
This does not claim an independent candidate evaluation or relax existing
implementation/evaluation contracts. JobSpec v3 and Agent wire schemas are
unchanged. Stage selection and dispatch use the retained selection contracts below;
released-process qualification remains tracked separately in task 9.1.

### Stage profile selection contracts

A model-backed `reasoning` node may publish `profile_selection` instead of its
static `build` binding. The two bindings are mutually exclusive. Selection pins
one exact role version and 1–64 distinct complete profile snapshots, each with
its ordinary Build binding. Candidate names are arbitrary; there are no built-in
tiers or provider-specific fields. Profiles, execution references, policy inputs,
criteria, candidate mappings, selection allowance and failure behavior are covered
by the existing immutable Flow definition digest and retained admission closure.

All candidates use the node's task and outcome schemas. They must preserve the
same result output, category, outcome, verifier, evidence and capture/deliverable
contract. Their toolchain and Build configuration must match their complete
profile; required context must have unconditional explicit source selections.
Credential purpose must agree across candidates, and each profile's permissions
must fit the stage ceiling. Flow and Factory Configuration publication reuse the
installed role-profile capability validator and the common authorized Build
reference checks. Admission also checks Project ownership. Existing static Flows
require no role/profile catalog and keep their original serialized content.

The declaration supports `manual`, ordered `rules` with an optional explicit
`otherwise`, and `decision_api` with an exact policy and bounded input projection. Rules reuse
the common typed Flow predicates and their structural/parameter bounds. Failure
is either `manual_selection` or one declared `reserve_profile` candidate. For
every candidate, selection allowance plus its full execution ceiling must fit all
five stage budget dimensions, including the reserve. Candidates are alternatives,
so their execution budgets are not summed together.

Manual policy accepts an optional fixed `candidate`, for example
`{"mode":"manual","candidate":"specialized"}`. Omitting it records a pending
manual choice; an undeclared key is rejected at publication. The owner atomically
retains the prepared input and `FlowProfileSelectionRecord` in the common fenced
node journal before allocating an execution attempt. The input digest binds the
exact Run, nested Flow, cycle, node and input generation to the pinned candidate
closure. Repeated polling and owner takeover read this record and the original
Context without another preparation, execution attempt, budget charge, Build or
provider call. The original start time and bounded stage deadline are retained.

PostgreSQL migration 0102 stores these facts in `factory_flow_profile_selections`
with an exact frozen-input foreign key. The existing bounded metadata cleanup
removes selection rows before their inputs, without cascading history deletion. No separate worker, scheduler or byte store is introduced.
The same journal validates restored selections against the original policy.
Manual records omit the optional rule provenance field, preserving their existing
serialized identities.

Rules evaluate the frozen, schema-validated node payload through the common typed
Flow evaluator, with one shared 16,384-step allowance across the ordered rules
and their nested collection scans. Known false conditions advance to the next
rule; the first true condition selects its declared candidate. Only when every
condition is known false does the explicit `otherwise` candidate apply. A missing
operand, incompatible value type or exhausted allowance produces a typed
`missing_input`, `invalid_type` or `evaluation_bound_exceeded` failure. All-false
rules without `otherwise` produce `no_match`. These failures invoke the configured
stage failure policy; they never silently take a catch-all. Schema-invalid input
is rejected by the existing payload contract before selection, without coercion.
An earlier explicit `absent` rule can handle an optional missing field before a
later comparison requires it. Temporal predicates use the retained original
selection start time, including during restoration.

The optional record field `rules` distinguishes `matched` (with the zero-based
rule position), `catch_all`, `no_match`, and `failed` (with its safe typed reason). The input
digest and admitted definition bind the exact projected observations, schema,
ordered criteria and complete candidate profiles. Restoration recomputes the
same outcome from that frozen input and original time. Polling reuses the saved
choice without evaluating rules over a new input; its current capability
eligibility is checked separately against the live catalog as described below.

Automatic failures apply exactly one pinned failure policy before the initial
selection fact is committed. `manual_selection` keeps the candidate empty;
`reserve_profile` selects only the configured exact candidate if the current
Project capability catalog qualifies that complete profile. The same capability
validator is used during publication. A missing catalog, disabled Factory mode
or incompatible reserve leads directly to manual waiting, without trying another
candidate or provider. Store read failures use the ordinary error/retry path and
never authorize a reserve. The optional `recovery` field records `reserve_profile`,
`manual_selection`, or `reserve_unavailable`, alongside the original rule failure.
Recovery keeps the original input, start/deadline and admitted budgets; the
journal atomically retains the input and final disposition without execution
attempts or budget replenishment. Polling/restoration reuse that fact even if the
catalog later changes. Earlier records without `recovery` remain readable.

A retained choice is not a current capability grant. While the selected stage
waits before dispatch, the owner checks its exact Build configuration, runtime,
model, verifier, evidence dependencies, selected skills, MCP configurations, tools
and profile enforcement against the current Project catalog. Unavailable optional
entries are pruned from the installed ceiling, while the retained profile stays
unchanged; an unused option does not block a profile. Publication and current
eligibility checks use the same dependency filter as the authoring catalog.
Revocation, disabled Factory mode or missing enforcement returns
`CapabilityUnavailable`; it never relaxes permissions, selects the reserve or
changes the retained input, choice, deadline or budgets. Restoring the capability
allows the same choice to proceed. Journal restoration itself needs no live
catalog. Ordinary queued/running Builds wait for worker capacity through the
existing Build/Placement mechanism and retain their profile and operation;
an absence of idle agents is not capability incompatibility.

The common Flow/node owner accepts the existing `FactoryConfigurationStore`
catalog through `with_profile_catalog`; server assembly supplies it. Static
execution and unselected manual waits need no catalog dependency. No additional registry,
scheduler, table or provider fallback is introduced.

Fixed-choice retention, pending state, deterministic rules, their failure
recovery, current capability validation, live application selection commands and
selected Build dispatch are implemented. Decision provider consumption uses the
purpose-specific existing Signal bridge; a recorded reserve alone
is not dispatched execution or stage completion.
Manual task creation and static Build execution keep their existing mechanisms.

### Work admission profile overrides

The application Work admission intent accepts optional `profile_overrides`: a
bounded array of exact stage/profile pairs. A stage contains the immutable root
Flow reference (`root`), an ordered path of subflow-call node keys (`calls`), and
the final node key (`node`). An empty call path addresses a root stage. The profile
is a complete Project-owned ID/version/digest reference from that stage's pinned
candidate map. Two calls of the same nested definition have different addresses.
There are no wildcard targets, mutable aliases or per-setting patches.

Admission validates these references before resolving the repository and again
at the atomic store boundary. Duplicate stages, foreign or unpinned profiles,
unknown calls/nodes and static stages are rejected without partial Work.
The collection uses the existing Flow node-count, nesting and data-byte bounds.
Canonical stage ordering makes equivalent request ordering replay-identical.
Changing a choice under an accepted idempotency identity conflicts; an identical
request returns the original Work/Run before another VCS resolution.

The ordinary immutable Work snapshot retains the choices. Each prepared input
binds its applicable choice through the actual parent-call ancestry, and the
ordinary journal verifies that association. A matching override precedes manual,
rules and Decision API policies without evaluating rules or calling a provider.
It applies to each permitted occurrence of the declared stage, including retries
and later cycles; unrelated stages keep their configured policy. Selection
records retain the exact input and profile with no automatic rule/recovery result
or added budget. Current capability validation still applies before execution.
Existing Work and input bytes omit the optional fields when no override exists.

REST and Console admission accept optional exact `profile_overrides`.
Admission cannot replace an
already dispatched invocation or authorize
an otherwise forbidden restart, cycle or retry.

### Decision API profile selection

The three stage policies use the same pinned candidate map. For example, a stage
with `default` and `pro` candidates can declare:

| Mode | Explicit choice | Result before dispatch |
| --- | --- | --- |
| `manual` | `{"mode":"manual","candidate":null}` | Wait for an operator; `candidate:"default"` fixes that pinned candidate instead. |
| `rules` | `{"mode":"rules","rules":[{"when":{"operation":"compare","comparison":"equal","left":{"source":"path","path":"/complexity"},"right":{"source":"literal","value":"high"}},"candidate":"pro"}],"otherwise":"default"}` | High complexity chooses `pro`; a valid different value chooses `default`; missing required input invokes the declared failure policy. |
| `decision_api` | Choose an exact installed profile-purpose policy and its declared input mapping; explicitly bind its finite answer keys to the pinned candidates. | The qualified provider recommendation is consumed under its own rollout/thresholds; failure uses the stage's pinned reserve/manual policy. |

All modes require one explicit `failure`: `{"kind":"manual_selection"}` or,
for these candidate aliases, `{"kind":"reserve_profile","candidate":"default"}`.
Names such as `pro` carry no privilege. Admission overrides and live manual controls
may select only the already pinned eligible profiles and never replace a dispatched
Build. Reusing a routing policy or inventing answer keys does not qualify a profile
selector. The exact installed input projection and call allowance still apply.

The private deployment catalog accepts `profile_selectors` entries with
`selection_policy`, `project_id`, `plan`, `baseline`, `credential_profile`,
`project_permissions` and `local_permissions`. They resolve the exact capability and credential through
the existing Signal connector catalog; metadata cannot construct a transport.
The optional consumer is installed on the common Flow owner. Static, manual and
rules stages do not require it. Project, Flow, node and deployment permissions
must permit the selected transport and its dedicated credential.

The loader content-addresses the complete installed plan and narrowed authority
under `selection_policy`, using the same pinning approach as installed actions.
The Flow references that exact compiled identity; Rust installers use
`pin_policy` before registration. Changing rollout, questions, model, projection,
budget or authority creates another reference. Earlier Runs keep their original
policy or reach its configured unavailable recovery if that version is retired.
This installation identity is distinct from `plan.profile.policy`, the consuming
policy retained in the Signal request/receipt.

The capability and request explicitly declare `profile_selection`. The JEV
adapter can advertise this purpose after its finite-choice conformance checks;
the current OpenAI adapter rejects it. Profile keys are separate from routing
dispositions and cannot select Flow edges or complete execution gates. The
request must enumerate exactly the stage's pinned candidates and use its explicit
input projection. Role instructions, skills, transcripts and ambient context are not
added to that projection.

The installed threshold uses `qualify_profile_selection` to retain an
operator-owned qualification fingerprint over the exact provider, adapter, model,
question set, consuming policy, question/criteria content, probability semantics,
calibration and threshold/margin. A purpose label or
routing calibration alone cannot qualify profile control. This records trusted
installation metadata; it does not measure model quality or authorize automatic
promotion. Shadow and advisory modes retain the explicit baseline, including
when an external outcome is unobservable. Bounded control accepts only a declared
answer meeting the qualified probability and margin.

The existing private `factory_flow_profile_selections` journal retains prepared
requests, one dispatch reservation, receipts and recovery as fenced generations.
Request identity, projected input and deadlines survive restart. A reservation
permits observation, never another evaluation. If the original outcome cannot be
observed, the journal retains uncertainty and charges the full reserved ceiling
without fabricating a measured receipt. A required unusable signal reaches the
stage's existing reserve/manual policy once, retaining its request and original
receipt or uncertainty. A late result cannot replace an accepted manual choice;
cancellation, exhausted hard budgets and the original deadline remain controlling.

Calls must fit `selection_budget`; measured or unresolved provider resources are
charged once to the ordinary Run budget without allocating execution attempts or
agent quota. Unsettled ceilings remain reserved across all Run transitions and
cannot be spent by another intent or budget observation. Both settled usage and
unsettled ceilings also count against the budget of their owning Flow. Only the
latest selection revision for each frozen input contributes to these totals.
Build admission excludes
the already charged selection resources from the stage reservation and retains
the exact selection digest. Candidate
capability checks and ordinary Build compilation remain the shared execution path.

### Live profile choices before dispatch

`SelectFactoryProfileCommand` uses the existing authorized Run control handler.
It addresses one exact frozen `input_digest` (including nested Flow, cycle,
node and execution generation), an admitted Project/profile ID/version/digest
and the current Run version. The atomic control boundary accepts pending or
already selected occurrences before Build intent commitment, within the original
deadline and without cancellation or terminal state. Unknown/unpinned choices,
stale versions and a duplicate of the latest accepted intent conflict without
changing history. Acceptance retains the actor and request through the existing
audit, records `select-factory-profile`, advances the Run version and fences the
previous owner. It does not create an execution attempt or Build.

The next fenced owner revalidates current capability availability and consumes
only the latest accepted control for the occurrence. An operator may replace
an unconsumed intent after reviewing the new Run version; all earlier intents
remain retained. Consumption appends a selection generation linked to its exact
predecessor and control. Initial automatic choices, reserve/failure provenance,
manual choices and superseded facts remain immutable. The frozen input,
original start/deadline and measured budget are preserved. An unavailable
capability blocks consumption without silently choosing another profile.
Polling after consumption adds no rows.

The common journal validates the complete predecessor chain, unique generations
and canonical pinned candidates. Run version and worker ownership checks serialize
competing publication. A renewed worker fence cannot authorize an older control,
a stale generation or an automatic result after manual acceptance. The same
publication checks cover the existing one-shot reserve/manual recovery over a
retained raw selector failure; they do not restart the selector or renew its
limits. Earlier first-generation manual records retain their original wire
representation and content identities.

A selected ordinary `FlowBuildIntent` retains the exact selection digest and
Project/profile ID/version/digest alongside its immutable execution binding. The journal checks
that it references the current generation. Commitment and manual acceptance use
the same Run lock/version, so only one wins; a pending newer control must be
consumed before commitment. A committed intent, including an unknown dispatch
response, cannot be replaced by another choice. Current selection/control lookup
is derived from the retained journals without another mutable projection.
Memory and PostgreSQL share these checks and the existing tables and recovery
paths.

An identical idempotent request returns its original acceptance receipt even
after supersession, consumption, later progress or expiry, without another
version increment. Changing the intent under that identity conflicts. A receipt
confirms recorded intent, not execution. The Flow owner commits the selected
Node Attempt and ordinary Build intent together, using the frozen input and
original selection deadline. Execution permissions and budget narrow to the pinned
profile, including its elapsed-time watchdog; the Node Attempt keeps the enclosing
stage reservation. Configured phase pools retain their normal ready queue and
capacity admission before attempt allocation. Before publishing a choice or selected
intent, the owner rechecks its exclusive deadline with a fresh clock reading,
including after asynchronous catalog qualification.

Before selection, owner polling, live choice acceptance and phase-pool admission
share the ordinary stage reservation check against the remaining Run and Flow
budgets. A retained selection is actionable strictly before its original deadline;
polling and takeover cannot renew it or spend new attempts/tokens. Expiry or
exhausted hard budgets reject progress without introducing another Run state.
An undispatched expired selection cannot reenter the ready queue or keep a
phase-pool reservation. Once execution exists, its reservation remains until the
ordinary completion or confirmed stop receipt.

A manual profile wait consumes its enclosing Run WIP slot, but allocates no
Node Attempt, Build or execution slot. Cancellation follows the existing Run owner,
closes the Run and releases its WIP; a late profile control cannot revive it.
Reserve/manual selection-failure policies apply only before dispatch. An execution
failure retains its selected binding and measured cost in the normal stop/retry
history, without another selector call or an automatic upgrade.

Selected invocations require the complete exact role package, even if a technical
task could match an installed static profile. The existing compiler/materializer
qualifies that package and publishes it through the ordinary retained-input owner;
a missing or substituted package cannot authorize a static fallback. Installed
invocation sources supply original pins and authorized content, never current
defaults. Output verification reads the same private retained package through an
optional publication-store dependency and reconstructs the qualified ordinary Job,
including capture contracts. Static tasks do not consult that role dependency.

After intent commitment, takeover observes or replays the same ordinary Build
operation without consulting the selection catalog. A lost dispatch acknowledgement
cannot allocate another Build. Ordinary Build retries keep that operation, profile
and package while retaining the actual Attempt/Job provenance; they do not invoke
selection or its reserve policy.

An explicitly confirmed `RetryNode` after technical failure also retains the
preceding committed binding and original selection provenance. It creates the next
frozen input generation and a new ordinary operation under the existing retry
permission, budget and execution window; it never evaluates selection or recovery
again. Current capability qualification still applies, and a configured phase pool
requires a fresh capacity selection. Result provenance must match both the exact
input node and the committed intent profile in the common journal; another
compatible node or candidate cannot substitute its task/model contract.
REST and Console expose the application contracts in sections 7 and 8.

### Role package recovery and lifetime

Before writing compiler objects, the existing input publisher retains the complete
private invocation under the persisted node operation and current owner fence.
The snapshot includes canonical package content, exact skill transfer metadata and
qualified logical credential mappings. It must match the existing owned role/profile
versions and immutable task intent. An identical replay recovers the original snapshot;
a different package cannot replace it. A restarted materializer reads this snapshot
before consulting the optional source, so catalog replacement or local folder removal
cannot change the invocation. Foreign keys retain the exact definition versions.

Imported skills remain immutable Project catalog content, like retained role/profile
versions; reimport does not overwrite an earlier bundle. Their bytes keep the existing
Artifact object owner, and abandoned staging uses the existing private-input cleanup.
Published role compiler inputs follow Run retention, which is already scheduled after
the linked ordinary Build retention deadlines and respects Build Result holds.
The shared cleanup worker deletes their objects after the Run becomes hidden, then
bounded Run metadata cleanup releases the snapshot. A hold accepted before hiding
preserves the package; a late hold cannot revive released package bytes. Failed,
unpublished object writes still use the original node cleanup deadline. Storage
retention never extends the signed input download deadline. Generic package Debug
output is redacted, and audit records contain safe operation/content identities only.

### AI-agent execution capacity

A role-backed ordinary Build admission checks the current Project role concurrency
limit in its existing fenced acceptance transaction. PostgreSQL uses the same Project
lock as role publication. Active queued/running Build rows are the reservation across
all profiles and role versions; there is no additional counter or release worker.
Authoritative completion/cancellation releases capacity through ordinary Build state.
Lowering the current ceiling blocks new admissions without changing active Builds or
their retained packages. Definitions without a limit keep inherited capacity behavior.

An exhausted role returns the ordinary transient capacity wait, preserving the selected
package and operation. Retry does not select another profile. Accepted-operation replay
precedes current capacity checks, including after response loss or owner restart.
Management Build Retry rejects Factory-owned Builds in both memory and PostgreSQL;
Factory retries require the owning Run's RetryNode control and current node authority.
Existing Factory/Project/worker ceilings and manual CI/CD creation remain authoritative.

### Review role isolation

The common role invocation boundary qualifies the complete selected review profile,
even when its current Task Envelope is narrower. Independent readonly review uses
only the existing evaluator credential purpose, declares no tools, and permits
writes only to the ordinary output/scratch roots. A profile carrying source-write,
coding/delivery credential or additional tool authority is rejected during compilation,
before compiler inputs are published. Runtime-specific setting checks remain in
the installed adapter; role names confer no authority.

Skill references do not grant access to arbitrary Artifacts. Project skill preparation
resolves only complete imports from their existing Project publication owner; even
same-Project private test inventory/source publications from other nodes cannot be
relabelled as imported skill files. Missing Context requirements are rejected by the
common package contract, and existing candidate/protected-test readers and signed
input selections keep raw private test sources, hidden transcripts and unselected sibling
outputs outside model access. This is explicit selection/ownership enforcement,
not inspection of filenames or instruction prose. The retained prompt preserves
Project rules before profile additions, including attempted waivers; deterministic
validation gates remain responsible for observable compliance.

Agent registration must advertise the exact v3 semantic enforcement route
needed by each Factory Job. Host, Apple VF and Windows WHP preview do not gain
Factory qualification from their ordinary CI/CD support. Missing v3, a blocking
tool hook, workspace anchoring or another required capability leaves that Job
unscheduled or rejected; it must never select a weaker route. See
[JobSpec v3](../reference/protocols/signed-job-spec-v3.md).

## Admit and follow one Work item

1. Select the owning Project, exact enabled Factory Configuration and Repository
   version. Confirm that the immutable Flow closure, budgets, permissions,
   profiles, input retention and mandatory verification are the intended ones.
2. Prepare task and acceptance Artifacts through an ordinary Build and select
   their exact retained references. The current manual intake requires those
   inputs. Direct-text quickstart and a unified Work card are separate proposals,
   not prerequisites already supplied by this release.
3. Admit Work through [management REST](../reference/management-rest-v1.md#optional-factory-management)
   or **Admit Work** in the [Factory workbench](operator-console.md#manual-work-and-run-investigation).
   Keep the confirmed request and idempotency key for exact transport replay;
   never silently resample a branch, input or Configuration after an unknown
   response. The server returns the original Work/Run and frozen base revision.
4. Follow the Run's current Node Attempts and linked Builds. The pinned graph
   explains permitted control and Context/data routes; the execution journal
   records what happened. An absent row on a bounded page does not prove absent
   work. A model recommendation or completed implementation Build does not mean
   the candidate is accepted.
5. Answer a declared HumanGate or bound operation with its exact typed value,
   target and current Run precondition. A recorded command is intent awaiting
   owner processing, not a successful node, delivery or deployment. A stale
   response requires explicit refresh and review.
6. Inspect deterministic evidence, independent Assessments and the Decision for
   the exact candidate. Retain its ChangeSet and immutable result references.
   Delivery for review and policy-qualified promotion remain separate actions;
   no screen or model summary grants merge authority.

Every step is available headlessly. The Console has no separate lifecycle
authority, and leaving an unconfirmed form publishes no Work or configuration.

## Provider failure, cancellation and recovery

In bounded control, treat timeout, invalid result, unsupported capability, low
confidence and unknown provider outcome as classified failures under the frozen
fail-closed fallback. Shadow/advisory retain the deterministic baseline and
record the unusable observation without granting the signal authority. Keep exact
provider/model/policy and logical request identities. Observe or replay the
original operation; do not substitute a model or dispatch the same logical
decision again merely because its response was lost. Server-side JEV credentials
remain outside Agent workloads and protected tool operations stay blocked until
their broker returns a valid recorded disposition.

Disabling a Configuration prevents fresh admission; already admitted Runs retain
their pinned versions. To stop existing Work, use the exact Run cancellation
command. Draining capacity stops new leases and is not a replacement for cancelling
an active Run. Confirm terminal stop and retained measured usage through the
journal before treating effects as stopped. Never fabricate a completion from a
killed process or delete claim rows manually.

On restart, restore the same immutable profile/connector identities and let fenced
recovery use durable inputs, receipts, original deadlines and budgets. A missing
provider session is not missing authoritative state. Reporter retry/failure affects
only its independent outbox, not an accepted Flow result. For disaster recovery,
use the complete [backup/restore procedure](backup-restore.md); preserve all
Factory metadata, compiler-input objects, private configuration references and
retained candidate/evidence objects in the same consistency unit.

## Signal rollout and release qualification

The [calibration seed and comparison guide](../../tools/fixtures/factory-signal-calibration/README.md)
documents the offline routing/tool-risk corpus and comparison command. Use it
to prepare and inspect measurements; its reports cannot authorize promotion.

Routing and tool risk have independent `shadow`, `advisory` and `bounded_control`
policies. Shadow records comparison only; advisory exposes a recommendation while
the deterministic baseline acts. Do not treat either as permission to enable
bounded control. Require labeled purpose-specific corpora, exact provider/model
and probability semantics, comparison and drift/failure/latency limits, an audited
promotion, and demonstrated immediate demotion/fallback. A fixture endpoint tests
the adapter; it cannot calibrate or approve a real model. If any of those gates is
unqualified, retain shadow/advisory and the existing deterministic policy.

Retain release evidence for two independent modes:

- Ordinary CI/CD: Linux/macOS/Windows portable checks, old v1/v2 canonical wire
  contracts, packaged Host and qualified ordinary backends, headless REST,
  non-Factory Console activities, migrations, retention and local-stand lifecycle.
- Opt-in Factory: signed v3 positives/negatives on qualified real backends,
  protected input/tool boundaries, exact nested Flow/candidate/evidence lineage,
  independent agents, server/Agent restart, unknown responses, cancellation,
  audit/retention and required delivery/promotion qualification.

Use the pinned release tools and locked dependencies. Run formatting, architecture,
strict Clippy/rustdoc, tests/coverage, security scans/fuzzing, locked Console quality
checks and package verification. Record the source revision, dependency/plugin
identities, commands, outcomes and retained artifacts. A skip is not a pass; an
old successful revision cannot attest to a later correction. See
[backend qualification](../testing/backend-contracts.md) and
[security release gates](../testing/security.md).

The backend workflow intentionally runs packaged Host contracts on push. Its full
hosted matrix runs on the nightly schedule or manual `hosted` suite; self-hosted
platform qualification needs the documented provisioned runners. Do not infer
full Linux performance or Factory qualification from the push workflow alone.

External ticket discovery/reporting and an Octa-local JEV task retain the
[separate extension boundaries](../architecture/factory-extension-boundaries.md).

### Project role/profile HTTP authoring and safe selection history

All role/template/profile and skill routes are Project-scoped under
`/api/v1/projects/{project_id}`. `factory-roles`, `factory-roles/{role_id}/profiles`,
`factory-role-templates`, `factory-profile-capabilities` and `factory-skills` are
bounded current catalogs using `limit` and opaque `after` continuation. Role and
profile details have current and `/versions/{version}` reads. Templates use
`factory-role-templates/{identity}/versions/{version}?digest=...`; their version
is a release key such as `v1`, rather than an owned numeric revision.

`POST factory-role-publications` atomically publishes an optional role and a
bounded profile batch. Every supplied item carries its own
`expected_current_version`: omit/null for creation, exact numeric head for
replacement. A stale item returns 412 and publishes nothing. The request requires
`Idempotency-Key`; identical retries return the original receipt before checking
current heads. This permits independent profile replacement without copying role
instructions. `POST factory-role-template-copies` uses the same publication owner,
with explicit role/profile identities and complete settings, pinned to an exact
release template. `POST factory-roles/validate` and
`factory-role-profiles/validate` normalize content without publishing or invoking
a provider. Profile validation also requires the exact role and installed
`capability` alias. Dedicated details, validation and publication responses use
`Cache-Control: no-store`; ordinary role/profile catalogs omit instructions and runtime values.
Flow detail, validation, publication and current-page responses also use `no-store`,
because their definitions may embed retained profile instructions.

Installed profile selectors also project safe authoring facts into the existing
Factory choice `metadata.profile_selection`: exact provider/adapter/model,
question-set/calibration/probability-semantics references, rollout, baseline, finite
answer keys and probability/margin thresholds in millionths. Native installation
derives these facts from its validated selector plan; there is no second policy
definition or provider call. Restricted choice reads omit this projection when any required
provider/adapter/model/question-set identity is hidden. Instructions, question
text, credentials and provider payloads are not included. Authoring hints do not
replace publication/admission qualification.

Console explicitly maps each pinned profile to one installed answer key; profile
IDs and names are not selector answers. Flow/configuration validation checks the
complete answer domain and baseline when the exact installed policy declares them.
The runtime still checks the retained plan, input projection, call budget and
current authority before a provider call. Older catalogs without selector metadata
do not gain qualification from its absence; static/manual selection is unchanged.

`POST factory-skills` receives one logical identity and explicit regular files,
each with a relative `path`, SHA-256 and a JSON byte array. It opens no server-side
path and accepts no links or archives. The shared reviewed-tree/import owner
validates the complete tree and publishes through retained inputs/object storage.
The transport's existing 8 MiB JSON limit includes encoding overhead; common file
and aggregate byte limits still apply. Exact retries return the original bundle
reference and publication time, without object locations or file content.
The receipt's `bundle` contains `identity`, `version` and `digest`; it has the same
shape as a skill catalog item. The separate `published_at` preserves the original time.
File inventories stay in retained storage and are not included in the receipt.

Manual Work admission optionally accepts `profile_overrides` containing an exact
`stage` (`root`, ordered nested `calls`, `node`) and `profile` reference. Omitting
the array preserves the earlier request. Existing admission validates candidate
membership and immutable stage binding; no selection provider is required.

`POST /api/v1/factory-runs/{run}/profile-selections/{input_digest}` takes only
`profile`, `Idempotency-Key` and a strong numeric `If-Match`. Missing preconditions
return 428; stale versions, competing intents and post-commitment choices return
409 through the ordinary Run-control contract. A 200 receipt records accepted
intent, not execution. The fenced owner consumes that exact intent separately.
An exact replay returns the original receipt after dispatch, but current Run and
owning Project authorization are always rechecked. Execute authority is required;
this endpoint cannot replace a committed package or create a profile.

The existing paged Run diagnostic route adds `profile-selections`. It exposes
exact input/revision/predecessor/control/role/profile identities, configured mode,
rule disposition or original safe provider receipt state, original recovery and
deadline, supersession, and ordinary operation/Build links. An accepted, unconsumed `pending_control` is
reported separately from the executed choice. Retained packages are
represented only by their content digest, when published. Eligible candidate
references are computed using common current capability and control eligibility
checks; expired, cancelled, superseded and committed occurrences expose no choices.
Reads grant no reservation. These projections omit instructions, skill content,
model parameters, input facts, provider questions/results, secrets and worker
ownership. Dedicated authorized content reads remain the authoring boundary.
`thinking_level` is the display text of the selected profile's explicitly declared
`thinking_parameter`: strings remain text, numbers and booleans use their JSON
scalar spelling. It is null without the annotation. The projection uses the
retained exact profile, so catalog replacement or unavailability cannot change
historical reasoning metadata; other model parameters are never traversed.

### Shared AI-agent skills and avatar authoring

The AI-agent Role owns its uploaded raster avatar, name, description, shared
instructions and exact shared skill selections. Every profile pinned to that Role
version uses the shared skill set through the existing execution-package builder,
verified protected-input transfer and prompt renderer. New profile editors do not
copy or separately edit these selections. Retained legacy profile additions remain
readable; conflicting shared/legacy aliases are rejected. As with instructions,
changes publish a new Role version and profiles adopt it explicitly; retained Runs
keep their original content.

AI-agent deletion marks the existing role as unavailable for new use rather than
removing immutable content. Current catalogs/current-alias reads exclude the role
and its profiles; exact versions, admitted Flow pins and retained execution packages
remain readable under their existing authorization and retention rules. The common
candidate qualification rejects deleted roles for automatic/reserve selection and
new Flow publication. Live manual selection checks the role from the admitted
binding under the deletion Project fence; new automatic-selection journal appends use the
same fence. Deletion does not cancel accepted Builds
or rewrite Run history. The Console and DELETE endpoint share the ordinary reviewed,
version-fenced, audited and idempotent management mechanism.

Import a folder with root `SKILL.md`, or a collection of such folders, in the
agent editor. Each outermost skill folder is reviewed and imported separately;
its supporting subtree is retained unchanged. Console defaults identifiers to
lowercase and validates edited identifiers before import. Successful import adds
its exact version to the draft’s selected skills; saving the agent still requires
a separate reviewed publication. Use View skill to inspect retained files without
discarding the draft. Dedicated Project-authorized inventory/file reads reuse the import
record and object reader, verify file integrity, and require Artifact download
authority for content. Only bounded UTF-8 is shown inline as inert text.

The avatar is a small PNG snapshot in existing bounded Role metadata. Console
normalizes PNG/JPEG/WebP uploads; publication decodes the complete raster through
the shared safe-preview mechanism. No remote image URL, extra file store,
provider call or execution grant is introduced.

The demonstration `tools/factory-stand` installs the compatible real Codex CLI
and a Project role-profile authoring capability. Its plugin-owned advisory catalog
provides model names and parameters, explicit refresh and manual entry; new
profiles require an explicit model instead of using the technical contract alias.
`default` and `pro` remain arbitrary profile names. Setup snapshots only existing
Codex authentication into the private Runtime home, retaining no workstation
instructions, MCP settings or skills. Missing login permits advisory/manual
catalog use. This metadata installation creates no Role, profile, task or Build
and grants no execution authority. An executable reasoning stage still requires
its separately installed Codex executor, Agent credentials and policy through the
[ordinary Factory installation](local-stand.md#optional-factory-installation).
An empty Runtime list therefore still indicates missing or unavailable installed
compatibility for that Project.

An installed role-profile declaration may attach `model_discovery` with an absolute
`executable`, its `executable_sha256`, and an installation-owned `environment` map.
The verified native plugin runs `--describe-models` through the existing bounded
process host. The caller supplies one JSON frame without a line terminator; the
shared host appends the single newline and closes standard input. This executable
has its own digest: a native Server discovery binary
can differ from the Linux plugin retained for Agent/Microsandbox execution. Neither
path nor environment is supplied by a Role or exposed as credential metadata.
Without this attachment, existing static capability declarations remain valid.
The [pinned Octa release](../../deployment/local-stand/inputs.json) includes the
Codex discovery timer correction: executable qualification and app-server queries
share one 10-second deadline, with runtime and stderr-drain cleanup on expiry.

Octa 0.7.1 adds the optional `model-catalog.v1` contract. Catalogs are bounded to
256 suggestions and 256 KiB; model identifiers are opaque UTF-8 up to 256 bytes.
OctaCity accepts the existing Flow scalar parameter grammar: closed object schemas
with string, integer, boolean or finite scalar enum properties. Flow bounds strings
in UTF-8 bytes: a plugin's `maxLength` becomes a conservative byte ceiling;
positive character-based `minLength` is rejected because it cannot be preserved
by the shared byte grammar. Unsupported schema constructs fail discovery
explicitly rather than weakening validation.
Initial unavailability does not fabricate models; a failed explicit refresh preserves
the previous usable catalog. Refreshes for different installed sources are
independent. An overlapping refresh for the same source fails immediately,
retaining its snapshot instead of joining an unbounded queue. Cached reads and
profile validation never discover.

Profiles retain `model_identifier` and the exact parameter values in their immutable
content and execution package. Omitted identifiers preserve legacy installed-model
behavior. The toolchain model reference still pins the installed execution/accounting
contract; the explicit identifier is passed unchanged to the runtime and checked
against observations. Exact token-rate pricing cannot be reused for a different
identifier; the installed accounting policy must support the selected model.
Refresh does not rewrite profiles, pending manual choices or retained runs.
