## Context

See `proposal.md` for motivation and scope. The existing `base-flow` parts already
describe eligibility and classification observations, declarative gates and
explicit handoffs. Generic Role/Profile execution, Project inputs, Issue status
actions and Source/Reporter journals are their owners; they are not new triage
subsystems.

The current normal stand publishes a deny-all Codex authoring declaration and an
empty `codex` executor list. `enable-local-stand-runtime-mcp` owns the supported
execution installation. Source/Reporter composition is qualified by
`complete-factory-source-runtime`, but its default preparation reads ticket
`## Acceptance`. Three empty label predicates match all labels, not just unlabeled
tickets. GitHub label choices are currently immutable installation snapshots.
The recently fixed startup path keeps advisory model discovery out of Factory
registration; readiness must not reintroduce that dependency.

The 2026-10-28 MCP decision separates server-owned synchronization from agent-side
investigation. The dependency baseline is historical evidence; its generic
"lookup attachment" wording is refined by decision 5 and tasks 4.2–4.4 below.
Transport authoring/pins already exist, but neither a configured transport nor
model discovery proves a scoped OctaCity MCP service or actual AI-client use.

## Goals / Non-Goals

**Goals:** Publish and run one complete, inspectable two-agent triage through the
normal Console and HTTP paths, with the same execution/authority/recovery contracts
as any configured Flow. Make the missing installation and context prerequisites
actionable. Retain exact reasoning results, verified facts, accepted decisions and
delivery receipts as separate facts.

**Non-Goals:** A new triage engine, lifecycle journal, credential store, autonomous
planner, global semantic index, unrestricted shell/MCP access, model-specific
prompts in Rust, paid-model dependency in default tests, GitHub label creation,
GitHub Projects column management, PR/merge/deploy, or execution of the recommended
downstream implementation pipeline. Priority assessment does not rewrite an
already admitted Work or Build's scheduling priority. A preset does not introduce
another owner for authoritative Issue planning fields.
There is no dedicated duplicate-search REST API, custom wrapper per external
MCP server, common provider query language or additional MCP client in the server
or Agent when the selected AI executor already supplies one.

## Decisions

### 1. Deliver a product preset over generic definitions

Add a release-versioned manifest under `config/factory/presets/triage/`, resolving
existing Flow template parts through their public instantiation boundary. Reuse
the base-flow contracts where their behavior fits; extend or compose their data
schemas without copying an entire development flow. The published graph contains:

```text
Source snapshot + reviewed Project context
  --> Project assessment --> eligibility gate
       --> reject / human review
       --> Systems analyst --> classification gate
            --> accepted triage result / human review
  --> explicit local status actions and configured Reporter effects
```

These are two AI stages; deterministic gates, optional bounded reproduction
commands, human review and trusted reporting remain ordinary technical nodes.
Role display names default to `Project` and `Systems analyst` (localized in UI),
but the operator selects existing or newly reviewed Roles and exact Profiles.
Names never dispatch code. Each stage has its own instructions, skills, model
selection, Context, budget and permission ceiling.

Resolution returns ordinary drafts and exact dependencies. Publication uses the
existing immutable Flow/Configuration commands, not a triage-specific command
journal. The initial result stops at triage; route recommendations are data for
later separately enabled inputs or an operator, not implicit execution authority.

Alternative: a new phase-specific executor or a monolithic all-development preset
would duplicate existing policy and obscure the two-stage acceptance boundary.

### 2. Keep dependency ownership explicit

Task 1.1 records the inspected implementation, current installation observations,
verification scope and shared API ownership in the
[dependency baseline and handoff](dependency-baseline.md). Task 1.2 records
the [baseline verification and TDD test correction](baseline-verification.md),
including current environment limitations.

| Dependency | Required boundary and handoff |
| --- | --- |
| `complete-factory-source-runtime` | Completed shared Source/Reporter, import, admission, association and ordered delivery baseline. Re-run its relevant public contracts; completed tasks do not prove an AI triage deployment. |
| `enable-local-stand-runtime-mcp` | Hard prerequisite for real reasoning execution, qualified Agent policy, AI-plugin materialization of selected HTTP/stdio MCP connections for the executor's native client and non-destructive installation updates. Its 1.2/3.1 handoff must qualify per-connection credential/purpose isolation, actual read-only enforcement, cancellation and tool-result capture; these details are required acceptance evidence, not claims that its current tasks are complete. Consume the existing owner; do not implement another launcher/client or loosen deny-all defaults here. |
| OctaCity MCP and scoped Issue authority | Triage 4.2 delivers the server-side MCP search/read boundary over existing authorized Issue services/storage. Authenticate each connection with a server-verified principal and explicit Project/read scope under existing authorization/credential owners. Missing token verification, revocation or scoped identity support is a prerequisite to implement/qualify here, not permission to reuse an unrestricted trusted-network management principal. No separate MCP credential registry is introduced. |
| Ready-made external MCP servers | Select compatible servers through ordinary retained Runtime/Profile configuration; the official GitHub MCP is the initial external lookup candidate. GitLab/Jira can follow through configuration and qualification without a triage provider adapter. Each server's transport, auth method, tool schema and actual scope remain its own contract; compatibility and read-only access must be observed before readiness, not inferred from the protocol name. |
| `simplify-integration-setup` | Owns authenticated resource enrollment, private-repository setup and connection lifecycle. Coordinate an explicit state-preserving existing-label vocabulary refresh through that installation owner. A preinstalled compatible public connection remains a valid pilot path; arbitrary private-resource setup cannot be claimed before this dependency is qualified. |
| `add-factory-bugfix-quickstart` | Not an execution prerequisite. Its planned generic `factory-presets` resolution and exact Configuration/Repository readiness contracts overlap this setup. Implement/reuse one minimal shared boundary, document the handoff, and leave its bugfix preset, text-intake certification and candidate/hidden-test pilot in that change. |
| Generic Factory/Agent release qualification | Verify the exact installed backend/toolchain needed by the selected profiles. Existing generic contracts are reused, but an unqualified backend remains a blocker regardless of a preset or health response. |

Do not edit or mark another change's tasks complete as a side effect. During
implementation, update a shared-contract handoff with the delivered API/schema
owner and evidence so follow-up work does not recreate it. Full Repository
Knowledge, JEV, development and delivery changes are not prerequisites for this
bounded pilot. If an upstream installation boundary is unavailable, report that
specific blocker; configuration-only tests cannot satisfy the final journey.

### 3. Add strict unlabeled selection without changing empty-set semantics

Extend the provider-neutral predicate with optional `label_presence: any|none`,
defaulting to `any` for omitted fields. `none` requires an empty observed native
label set and is incompatible with nonempty `all_of` or `any_of`; exclusions remain
ordinary configuration. Omitted/default settings must preserve old canonical
serialization, predicate digests and retained pins; adding the field is not an
implicit migration of existing inputs. Advertise support explicitly so an older installation
cannot silently ignore it. Apply the same predicate in preview, pull admission
and exclusive-group selection, with retained predicate/version fences.

The preset defaults to open, strictly unlabeled GitHub Issues. Operators can choose
the existing exclusion-based policy instead; the review names that different
meaning. The native connector still excludes PRs. No marker label claims a ticket:
the stable input/native admission identity prevents concurrent or repeated Runs.
New labels from this Run stop later matches but do not stop independently enabled
link refresh or retarget retained Work. Ordinary input edits never create another
intake cycle; a deliberate new cycle uses the existing explicit stable-input rule.

Alternative: treating three empty sets as unlabeled would break current clients
and exclude already eligible labeled Issues.

### 4. Use real triage completion criteria, not invented ticket acceptance

Keep the existing Work task/acceptance input contract and shared Project-input
publication, retention, fencing and recovery owners. Extend explicit Source
preparation to bind a reviewed immutable document containing **triage completion
criteria**, under the current Project and installation scope. Resolution supplies
the versioned preset document as a reviewable draft, or selects an existing
same-Project managed document. Ordinary Source input publication freezes its
content/reference, digest and purpose in the reviewed preparation settings.
Before admission, the existing preparation worker publishes the selected bytes
through the shared Project input service and its existing recovery journal. First
setup therefore needs neither an earlier Build nor a separate document-upload
workflow. The resolver remains read-only. The criteria specify required
assessments, evidence and finite decisions; exact provenance is retained with
the Source mapping and Work inputs.

This document describes success of triage, not requirements for implementing the
ticket. It is visibly distinct from source-authored acceptance and never fills
the canonical Issue's missing acceptance field or certifies a later bugfix. The
original title/body and absence of body/acceptance remain visible provenance.
Title-only tickets can enter triage; insufficient details lead to `needs_input`
or review rather than fabricated facts. Oversized content fails without truncation.

The current bundled criteria are concise `triage-completion` v2. The v1 file and
already retained inputs keep their original bytes and digest. Console shows a
localized result summary by default and reveals the exact reviewed document on
explicit expansion. Authentication and MCP/Reporter guidance is shown with
execution readiness rather than the completion criteria.

Existing field/body/heading mappings remain strict. There is no fallback from a
missing required ticket acceptance to the triage document. Exact preparation
policy and authorized document references select the mode. Replacement after
freeze cannot alter pending/admitted inputs; retention covers the document for
the Run. This avoids optionalizing acceptance across all Work/Agent contracts or
building a second input journal.

Task 3.1 delivers the reviewed draft/reference contracts and bundled completion
criteria described in [completion contract verification](completion-verification.md).
Task 3.2 installs the reader, freezes settings in the existing preparation
record and qualifies managed publication/admission and signed input transfer;
see [managed completion verification](managed-completion-verification.md).
Task 3.3 qualifies recovery faults and late authorization checks; see
[completion recovery and section review](completion-recovery-verification.md).

### 5. Provide bounded, provenance-bearing context

The supported `tools/factory-stand` default installs this triage pipeline through
the ordinary preset resolver and publication APIs. It installs the Project and
analyst roles with the operator-selected GPT-6.1 Sol model, publishes an initial
OctaCity goals document through the shared Project input owner, and leaves intake
disabled until the operator reviews and enables the configuration. Installation
does not manufacture demonstration Work or start model calls. The Console can
read the selected goals and publish revised immutable bytes; selecting the new
version is explicit, and already accepted Runs keep their original references.

The launcher accepts an explicit owner-private GitHub token file rather than a
token in source, command-line values, receipts or generated public configuration.
It creates the native GitHub connection and Project grant for the installed
resource's declared read/update capabilities. Provider token restrictions remain
authoritative. OctaCity MCP gets a separate Project-scoped read credential; that
credential grants no Reporter mutation or AI-provider authority.

The selected native Codex client owns ChatGPT login handling. A typed entry in
the existing Agent credential-profile installation selects only its private
JSON login file and relative per-Job destination. Re-read the current private
source for each Job, copy only that file into private execution scratch, redact
nested credential values at the supervisor output boundary, and remove the copy
with Job cleanup. Never import the workstation home, config, MCP settings or
sessions as model context. The trusted compiler pins native-file authentication
in its task digest, sets the isolated `CODEX_HOME`, and omits API-key fallback.
Existing API-key installations retain their task bytes and identity. No new
client, authentication broker, Factory state owner or public credential-bearing
JobSpec field is introduced. Native login recognition is distinct from a
successful model invocation and does not by itself prove model entitlement.

Sensitive JSON subtrees are selected by installed JSON pointers, without
provider-specific parsing in the Agent. Native updates may be retained only
when the operator explicitly selects an Agent-owned private installation.
Serialize Jobs using that installation and atomically retain the native
client's updated file after backend destruction, including failed execution.
Seed a stand-owned copy once; do not write back into the workstation's personal
cache or overwrite a refreshed installation with its original seed. Reseeding
requires the installation to be stopped. Authentication and refresh remain
native-client operations, not an Agent OAuth implementation.

The per-Job credential guard retains original and refreshed sensitive values.
Supervision rechecks the current execution copy before releasing messages.
Before entering Uploading, the existing output lifecycle checks frozen file,
report and archive bytes and refuses credential-bearing snapshots. It does not
rewrite traces, result records or provenance digests. Unsafe or uninspectable
credential files prevent publication. This verbatim-value exclusion does not
claim protection against deliberately transformed exfiltration.

Accounting follows the selected billing mode through the existing usage owner.
API execution keeps token and monetary budgets and requires a verified cost
counter or operator-supplied rates. An explicit subscription policy accepts
native token/time counters without inventing a monetary price. It cannot be
installed for API-key authentication. Token accounting remains available in
subscription mode, while technical execution time, attempt and resource ceilings
remain bounded. The existing finite budget contract is retained: its monetary
ceiling applies to known expenses, and cannot bound an unknown subscription
price. This change adds no nullable-budget API or optional-cap editor.

Measured monetary totals preserve their historical numeric representation.
An unpriced aggregate uses `{"status":"unknown","known_micro_units":N}`:
known expenses still count toward monetary ceilings, and an unpriced component
cannot turn into measured zero during aggregation or persistence. Observations
with unknown cost cannot be used as declared admission reservations. Native
login recognition and accounting support alone do not establish entitlement or
complete the packaged deployment qualification.

The reviewed setup selects required Project goals, optional Project rules and
supporting documents with their immutable references, the ticket source scope
and independently selected code Repository. The stand publishes only initial
goals; it does not manufacture a triage-rules document. Omitted rules add no
Context policy entry. Explicitly selected rules retain their controlling-policy
meaning and normal ownership, integrity and retention checks. The stand's stage
Profiles require task, completion and goals; a custom Profile's explicit rules
requirement remains enforced. Existing published Profiles and accepted Runs are
not rewritten when these defaults change.
Project assessment receives the frozen ticket and declared Project context, then
uses selected read-only MCP tools to investigate duplicates. The model chooses
and refines queries and reads candidate details across the permitted sources.
Candidate discovery is not a server-planned list fixed before the invocation.

Keep the two execution locations separate:

| Location | Responsibility |
| --- | --- |
| OctaCity server | Existing Source connectors poll/import/admit tickets; existing Reporter/outbox applies accepted effects. The OctaCity MCP server exposes authorized Issue search and exact candidate reads for agent investigation. |
| Agent and selected AI plugin | The ordinary execution package selects exact reviewed MCP connections. The plugin supplies only those settings to the selected Codex/Claude/other qualified executor in its supported native format. It does not inherit workstation MCP configuration. |
| AI executor and its native MCP client | Discover and call the selected servers' native tools. Ready-made GitHub/GitLab/Jira MCP servers retain their own names and parameter/result schemas; adding a compatible server requires configuration and qualification rather than an OctaCity wrapper for that provider. |

The OctaCity MCP service reuses existing Issue read authorization and storage,
with bounded query, pagination and detail retrieval. It must authenticate a scoped
principal and enforce Project/Issue read visibility, including a current grant
recheck after asynchronous reads. Reject foreign entities without revealing their
existence. It exposes no mutation tool for this use case. This is an MCP interface
to our tasks, not a duplicate-decision engine or a new search REST service.

Each MCP connection has its own authorized credential binding and access scope,
independent of the AI-provider credential and server Source/Reporter connection.
Secret references/purpose and exact transport/tool choices are reviewed settings;
values are resolved only through existing secret owners at authorized execution.
Pass them only to the selected client/server process or approved transport, never
into prompts, skill/context files, generic logs or public retained evidence.
Read-only mode must be enforced by actual server/tool and credential permissions;
a model instruction or tool annotation alone is insufficient. A prompt repository
filter likewise does not replace credential/server-enforced resource scope.
External-server revocation follows that server's enforcement; local connection
revocation blocks newly unauthorized execution under existing dispatch rules.
Do not promise a cross-provider atomic authorization snapshot or erasure of data
already delivered before revocation.

Consume generic connection installation/materialization from
`enable-local-stand-runtime-mcp`; record any missing per-plugin transport/auth,
secret isolation, cancellation or result-capture support in its handoff. Triage
4.3 qualifies the selected executor's composition, not a second generic MCP host.
Do not require every executor to support every authentication method. Unsupported
required bindings are readiness blockers; adding another AI executor requires its
plugin support, while adding another compatible MCP server requires no new
triage dispatch branch. Supporting Codex does not by itself certify Claude.

Use a small configured tool set, bounded result sizes/pages, execution deadline
and query/candidate budget. Prefer summaries before selected detailed reads;
allow independent lookups to overlap where supported and reuse already read facts
within the same authorized invocation. Never put the complete ticket database in
model context. New indexing/caching infrastructure requires measured need.

Retain actual tool calls/results through existing execution/result/Artifact and
Context provenance, bound to the exact invocation, connection/tool, query/scope,
observation time and content digest. Keep native candidate identity and revision
when supplied; otherwise retain the observed bytes/digest/time without inventing
an immutable provider version. Preserve coverage, pagination, truncation and
unavailable/failed/cancelled outcomes. The common contract is the bounded stage
conclusion with evidence references to those retained observations, not a uniform
search signature or parser for every external server. Unverifiable references or
unknown required coverage follow review/needs-input policy, never guessed proof.
No new duplicate index, credential broker or tool-result journal is introduced.
If a required MCP scope/client/capture capability is unavailable, readiness or the
stage reports that blocker rather than returning an empty successful search.

The analyst receives the original ticket, explicitly projected accepted handoff
and its own permitted repository/tool context. It does not inherit private stage
Context or hidden reasoning. Ticket bodies, repository files, tool results and
candidate descriptions are untrusted data, never role instructions or authority.
Goals missing from required Context, unresolved candidates and incomplete required
coverage follow declared review/needs-input policy. No bounded search claims a
global proof that duplicates do not exist.

Task 4.1 supplies the reviewed Project-context draft and ordinary ticket input
projection, with safe named-input diagnostics at the existing Artifact collectors;
see [Project context compilation and verification](project-context-verification.md).
OctaCity MCP, actual isolated AI/MCP execution and consumed lookup provenance remain
tasks 4.2, 4.3 and 4.4 respectively. Their unchecked status is unchanged by this
planning revision; task 4.1's context work is preserved.

### 6. Separate assessment from accepted facts and reproduction evidence

Eligibility data contains fit assessment, optional exact duplicate candidate,
bounded reason/evidence references and uncertainty. A duplicate candidate must
resolve to a visible observed entity; a model-invented ID or boolean cannot close
another ticket. Reviewed gate policy chooses only eligible, duplicate,
out-of-scope, needs-input or human-review outcomes. Defaults route unresolved
judgments to review; any automatic rejection policy is explicitly reviewed.

Classification extends the existing bounded schema with priority assessment and
retains kind, severity, size, component, risk, dependencies, preliminary
reproducibility and a finite recommended route. Component/priority/label vocabularies
are operator-owned configuration. The gate validates enum/range/references and
cross-field consistency; model prose cannot select a node or arbitrary effect.
Store this as an accepted generic result with exact profile, Context, schema and
record provenance, not as a phase-specific database entity. Display it on Run and
associated Issue results. Optional authoritative Issue metadata editing is not
inferred from classification; business status uses its existing guarded action.

For a defect, the analyst can consume configured ordinary reproduction commands
on the exact code revision and environment under the qualified Agent. Only
independently verified reports bind a reproduced/not-reproduced claim to the
subject, command, expected symptom, observations and applicable attempt. Model
claims alone, missing tools, timeout, checkout failure, unavailable dependencies
or a generic nonzero exit are not negative reproduction evidence. Preserve
`unknown/needs_research/needs_input`; feature requests use `not_applicable`.
The default triage can finish with an explicit unresolved reproduction result and
research recommendation; deep defect research remains a separate Flow.

### 7. Publish exact effects through one selected producer

Setup reviews finite logical outcomes mapped to existing authorized GitHub labels
and optional open/closed state. Priority, component and size can contribute only
declared effect combinations/branches; raw model strings are never label names or
mutation requests. Keep the vocabulary and number of combinations bounded and
validate complete outcome coverage instead of generating a combinatorial graph
silently. Compose optional priority/size/component effects as separate bounded
dimension gates and configured actions, with linear rather than Cartesian
expansion; validate total node/transition/budget limits before publication. Each
action keeps an exact finite effect and explicit classification-record binding.
These actions can be separately delivered, so diagnostics must not claim one
atomic GitHub mutation. Missing labels produce a setup blocker with the existing integration
remediation path. No label is created and no existing installation is rewritten
to make a draft pass.

Use Work-backed accepted status/source-update actions as the preset's external
producer, through the existing ordered outbox. Canonical Issue import is a
separate opt-in; when enabled, local workflow transitions pin its exact workflow
and have explicitly absent push authority for overlapping fields. Current
producer-conflict checks remain unconditional. A source-only Run works without
creating a card; a linked Run can expose the same decision/status with its
ordinary association and transition checks. Do not silently adopt a new workflow.

Accepted triage, local business status, Run state and external delivery are
independent. Mapping edits do not change frozen pending effects. Dispatch rechecks
current writer grants; lost replies require observation before retry, and exhausted
delivery does not roll back an accepted decision. Re-import, echo and completed-Run
replay cannot create another logical write or another input's execution.

### 8. Share narrow setup, resolution and readiness contracts

Use one generic Project-visible preset list/resolver with the already proposed
`GET /projects/{project_id}/factory-presets` and
`POST /projects/{project_id}/factory-presets/resolve` paths under `/api/v1`.
Return bounded ordinary Flow/Configuration drafts, parameter choices, exact
dependencies and missing-capability reasons. Do not create a separate triage
executor API, template registry or certification/provenance lifecycle. If a shared
implementation already exists when work starts, consume it rather than change
its meaning. Resolved drafts do not grant authority; ordinary validation and
versioned publication recheck current dependencies.

Reuse the exact Configuration/Repository readiness query planned by quickstart
(`POST /factory-configurations/{id}/versions/{version}/readiness`) with bounded
Source/context/reporting prerequisite observations. Distinguish executable
readiness, internally configured reporting and unverified native write access.
Required missing/stale execution facts block activation; actual GitHub write
permission remains visibly unverified until a real effect is observed and is
never guessed from a successful read. Readiness performs no provider/model call,
secret-value read, execution, publication, lease or budget reservation. Separate
explicit connection checks may collect current read observations.

Include the required OctaCity MCP endpoint/auth scope and selected AI plugin's
MCP transport, per-connection secret bindings, read-only enforcement and evidence
capture in those observations. Distinguish an MCP read token from AI-provider
authentication and server Reporter write authority. Unknown required MCP facts
block executable-ready claims; discovery/configuration success alone is not
proof of an actual permitted call. Reuse Runtime/Profile/secret-reference editors
and upstream setup guidance rather than adding a triage credential form.

Console composes existing Role/Profile, Source, workflow, effect and publication
editors into Project -> Triage setup -> review -> publish -> preview -> independently
enable intake. Existing input editing/disablement remains the lifecycle owner.
Multi-resource setup is resumable by ordinary receipts and exact versions; do not
promise one atomic transaction across publications or automatically enable a
partially prepared graph. Draft text and previews stay out of browser storage.

The packaged Console qualification exposed loss of precision in schema bounds:
JavaScript rounds signed 64-bit maxima during a normal JSON round trip. At the
shared management schema boundary, emit unsafe integer bounds as canonical decimal
strings and accept both legacy integers and exact strings. Keep internal numbers,
canonical hashing and retained definitions unchanged. Convert only declared schema
bounds, never arbitrary constant/result data; reject malformed or overflowing
decimal strings through ordinary validation. This avoids changing preset limits
or introducing a browser-specific hidden numeric protocol.

### 9. Qualify the real composition with finite external boundaries

Extend the packaged Source runtime journey with an actual configured verified
reasoning plugin and ordinary qualified Agent execution. Hermetic model/provider
endpoints can supply finite protocol-correct outputs, but cannot bypass the
compiler, materializer, permissions, Context selection, result verification or
outbox. Retain two distinct stage invocations/profile identities and actual
reproduction-command evidence. Run the same setup through HTTP and Console,
including narrow keyboard navigation and English/Russian.

Exercise actual native MCP client calls from the selected AI executor to the
OctaCity MCP server and a separately configured protocol-correct external MCP
peer. Observe independent credentials and resource scope, refusal of write tools,
tool-result retention and omission of unselected connections. The finite external
peer qualifies the generic transport/auth boundary, not all GitHub/GitLab/Jira
versions; separately qualify the exact ready-made server selected for deployment.
Readiness and setup must not imply every AI plugin or MCP server is certified.

Required cases include accepted defect/feature, duplicate, out-of-scope,
uncertainty, missing goals, title-only/no-acceptance input, labeled/PR exclusion,
failed reproduction environment, forged facts, malformed model output, hidden
candidates, label-scope/credential revocation, lost responses and controlled
restart barriers around admission, stage result acceptance and reporting. Keep
manual CI/CD, import-only and no-Source modes qualified. Default tests make no
paid LLM request and no live repository mutation. A separately authorized optional
live pilot records exact versions and evidence; it is not replaced by a health
check or a claim about semantic model accuracy.

## Risks / Trade-offs

- **Installation work is mistaken for a missing form** -> Treat executable Runtime,
  independent worker policy and label scope as visible deployment prerequisites;
  qualify their supported update path without resetting the operator's stand.
- **Triage completion criteria are confused with implementation requirements** ->
  Retain purpose and source provenance; keep the canonical Issue acceptance absent
  and require a separately reviewed implementation contract for later development.
- **Classification/reporting vocabulary grows exponentially** -> Use finite reviewed
  mappings and explicit bounds; no unbounded effect expansion or arbitrary model labels.
- **Search/reproduction coverage is incomplete** -> Retain coverage and exact evidence;
  route uncertainty explicitly rather than assert absence, duplication or reproduction.
- **MCP configuration is mistaken for enforced access** -> Qualify the actual
  AI-plugin/client/server chain with distinct credential bindings and server-enforced
  read/resource scope; prompts and tool annotations cannot supply authority.
- **Provider extensibility becomes a wrapper framework** -> Pass native MCP tools
  through the selected AI client; share execution limits and provenance, and keep
  the accepted stage conclusion independent of provider-specific query schemas.
- **A Source edit or another producer changes the same fields** -> Preserve frozen
  mapping/subject and use existing producer conflicts, ordering and version fences.
- **Current specs and overlapping changes drift** -> Record shared-contract ownership
  before implementing it; sync dependencies in order and never archive another change
  merely because this preset consumes its interfaces.

## Migration Plan

1. Verify upstream execution, AI-plugin MCP/auth/capture and server Source contracts
   on the exact current baseline. Deliver the scoped OctaCity MCP read service and
   qualify the selected external MCP server; do not replace Source/Reporter with
   agent tool calls or silently reuse their credentials.
   Add compatible optional predicate/preparation fields and the trusted preset;
   existing omitted settings retain their old behavior. Reject unsupported new
   settings explicitly. Any persistence change uses additive/reentrant migrations
   and existing input/definition/outbox owners; no new phase journal is required.
2. Apply approved installation updates through the upstream staging path, preserving
   PostgreSQL, objects, credentials, Agent identity, operator settings and accepted
   Work/Run history. Never edit retained bundles or expand grants silently.
3. Resolve and publish new disabled triage Flow/Configuration/input versions. Preview
   and review readiness/current parents before separately enabling intake. Existing
   inputs/Runs retain their exact snapshots, label scope and effects.
4. Roll back by disabling new triage intake and using prior published definitions.
   Keep accepted Runs, managed documents, result reads and pending reporting supported;
   ordinary restart does not redispatch old input identities or renew deadlines.
