# Triage preset

## Versioned Flow and responsibilities

Load `v1.json` through `FlowDefinitionTemplate::from_manifest`, resolving its
`parts` relative to `config/factory`. The manifest uses the ordinary bounded
resolver; it needs no triage registry or phase-name dispatch. It composes exactly
two reasoning nodes (`eligibility`, `classification`), a reviewed eligibility
program and a reviewed classification gate, using the common declarative
evaluator. Its explicit execution ceiling is four attempts, 240 seconds,
40,000 tokens, 4,000,000 cost micro-units, 4 MiB of output and one active node.
These are editable draft limits, subject to normal publication/admission checks.
The release grants no execution permissions.

`project-role-v1.json` and `analyst-role-v1.json` are editable
`FactoryRoleTemplate` seeds. Review and instantiate each into an independent
Project Role and Profile with complete operator-selected execution settings.
They provide instructions, not a model, credentials, an Agent or tool authority.
Their display names (`Project`, `Systems analyst`) have no dispatch meaning.

Select each stage independently. A template node accepts either an installed
`build_profile` alias or an exact `profile_selection` containing the reviewed
Role reference, complete Profile candidates/build bindings, selection policy,
failure policy and selection budget. Clear the static `build_profile` when using
`profile_selection`; simultaneous bindings are rejected. Instantiation delegates
to the same immutable reasoning-node validator as other Flows. Publication still
checks the Project, installed capabilities, current Role availability, Context,
budgets and permission ceilings. This adds no authority beyond the chosen Profile.

The Project input preserves the original Work's task and process-completion
Artifact references and scheduling metadata. The analyst receives these original
references plus only the accepted `eligibility_gate/classify` record. The Work's
immutable repository/revision remains the code subject. Configure Project Context
as described below, and select analyst Context separately; the handoff does not
implicitly expose the Project's private Context.

Project observations use the shared verified native MCP conclusion envelope and
the bounded eligibility assessment described below. Classification uses the
versioned triage contracts in `classification-results-v1.json` and
`classification-gate-v1.json`. The default accepts feature and unresolved defect
assessments without a reproduction command. The manifest imports no spec, research
or development nodes or optional spec-result schema closure. The Project gate
terminates at rejection or review, or admits the analyst. Every analyst gate result terminates at `triaged` or
`human_review`; downstream routes remain recommendations in the result.

Published definitions pin schemas, policies, Context, selected Profiles and budgets.
Editing release files or Role seeds requires a reviewed new publication and cannot
modify an admitted Flow's retained closure. Keep the prior configuration version
available when reverting a draft or selecting a previous published version.

The versioned templates and reviewed assessment/command policies cover tasks 5.1–5.4.
The shared management preset resolver/readiness (7.1–7.2) and packaged execution
qualification (8.1) remain separate requirements. These manifests alone do not
establish operational readiness.

## Reviewed analyst results

The closed analyst contract includes advisory `priority_assessment` (integer
0–100), `work_kind`, `severity`, `size`, `component`, `risk`, exact `dependencies`,
`recommended_route` and preliminary `reproducibility`. Component and route
vocabularies live in the editable schema; all projections reuse that exact shape.
Changing a vocabulary or priority range requires a reviewed new publication.
Both dependency lists reuse the `dependency_reference` schema and are bounded to
64 exact Run/Work/digest/terminal references. A reference is advisory data, never
an implicit read grant or scheduling dependency.

Classification is an advisory analyst assessment. The default dependency scope
is empty; nonempty
unreviewed references go to review. A separately reviewed gate may bind an exact
allowed scope without granting read access or scheduling work.

Feature requests require advisory severity and `not_applicable` reproduction.
Defects preserve `unknown`, `needs_research` or `needs_input`; incompatible
combinations, substituted or stale evidence and unsupported reproduction claims
go to `human_review` with `accepted: false`. A supported unresolved defect may
finish at `triaged` with a research recommendation. High assessed or admitted
risk also requires review. Reproduced and not-reproduced claims require the
applicable completed command findings described below. Deep research stays a
separate Flow. No recommendation starts another execution or external action.

The gate summary retains the assessment, safe command-evidence header (or explicit
`absent`) and the original admission priority/risk separately. It never changes immutable Work/Build queue priority, Issue planning
fields, labels, ownership or business status. `accepted` denotes only this gate's
assessment acceptance, not overall implementation or delivery success.

Generic outcomes now support explicit `public_result: true`. Omitted/false
preserves historical serialized bytes and keeps the body private. The triage
classification gate opts in only its bounded summaries. Authorized ordinary Run
completion diagnostics (also reused by associated Issue result reads) return
the original value beside its exact schema and record digest, using the admitted
outcome policy. Inputs, private Context, Build evidence and worker ownership are
not expanded. Reading performs no dispatch, retry, classification or mutation.
The analyst Profile seed includes a complete executable example; the compiler
test validates the actual protected prompt and result schema.

## Optional reviewed result effects

`FactoryResultReportingDraft` composes operator-selected effects into ordinary
Flow gates, `set_status` actions and a reusable reporting subflow. It performs no
provider, credential or storage I/O. The released manifest still contains no
implicit write actions or writer permissions. Select an exact Source, Reporter,
managed connection/access revision and the installed reporting policy separately;
the policy's advertised labels/states do not constitute a write grant.

Give every source gate outcome an explicit literal effect or `None` (preserve).
Optional component/size dimensions select a required finite enum field in the
accepted summary. Priority selects a bounded integer field using complete,
nonoverlapping inclusive bands or individual values. Each dimension declares the
source outcomes on which it applies and covers its complete field vocabulary.
The caller selects field paths as object keys, never model-authored JSON pointers
or label names. Only labels and optional states already present in the installed
editing policy are accepted; unsupported combinations, missing labels and
incomplete partitions block composition. No label is created or scope refreshed.

For each accepted source outcome, simultaneously applicable groups must own
different labels and at most one group may own native state. Alternative cases
within a dimension and mutually exclusive source outcomes can share these fields.
This prevents delivery order from deciding the intended result. Dimensions compile
as a linear chain, with one gate and at most one action per selected case; they
never expand into a Cartesian product. Existing native node/transition bounds and
node, child, parent and admission budget/permission ceilings remain authoritative.

Use `compile(result_schema, source_outcomes, id, version)` with the accepted gate's
exact schema and outcome domain. Select the installed status action's outcome and
receipt schema, the ordinary outbox retry policy and explicit technical limits.
Then `bind(template, source_node, reporting, prefix)` inserts exact outcome-bound
calls and preserves each original continuation. It refuses a substituted child
definition, conflicting aliases, unsupported fan-out/repeat routes or wider
authority. Include the returned child definition and all exact schemas in the
normal complete-closure configuration publication. That publication checks the
current Project update grant, selected writer connection and installed editing
ceiling again; preview or composition cannot bypass authorization.

Each call reads only its exact accepted source record. Child inputs retain that
call's digest, so each ordinary action has a provenance chain to the original
classification. The subflow returns the unchanged result; advisory priority does
not rewrite Work/Build scheduling. The existing Work-backed action owner supplies
the original native item and enqueues each effect through the existing ordered
outbox. No new provider client or journal is introduced.

The publication review in both Console locales explains that effects are
delivered independently. A `reported` subflow result means its actions completed
their ordinary queueing steps; it does not confirm provider delivery. Some effects
may already be delivered while others remain pending or fail. Canonical Issue
workflow execution and completed delivery/recovery qualification (6.3) are described
below. Shared management resolution (7.1) and packaged execution (8.1) remain
separate tasks.

## Independent canonical Issue and local workflow effects

Source intake runs without a canonical Issue. Its Reporter always uses the
original settled Source preparation, native item, accepted effect and writer
pins. Canonical import/attachment is a separate explicit operation; a visible card
does not establish a reviewed execution or local-transition authority. Import
does not start triage, and Source reporting does not create a card.

For an Issue-associated execution, use the ordinary reviewed Issue execution
request and exact Work/Run association. An explicitly configured
`transition_issue` action selects a pinned workflow/version/direct edge and one
declared outcome. The existing action and shared transition command recheck
current move authority, requirements applicability, workflow, current status and
field ownership. Model output cannot choose an Issue, workflow or principal.
Evidence-requiring edges retain their existing independently verified evidence
requirements; a triage summary is not implementation acceptance.

Select the appropriate execution path explicitly. Automatic Source intake rejects
an enabled target containing `transition_issue`, because it cannot supply that
reviewed Issue association. Disabled drafts remain repairable. An action without
its exact reviewed association fails closed with `capability_unavailable` and no business move; it does
not find another card or silently change workflows. Linking a raw Source ticket
alone does not enable this local-action path.

One producer owns each outgoing native field. Before Work reporting contacts a
provider, the existing Reporter reads any canonical Issue push policy in the
original Work's Project/native/security partition and applies the common producer
conflict validator. All configured push choices participate in field ownership.
Overlapping labels/state block the Work delivery; an opaque legacy Work mapping
cannot establish disjoint ownership. Disabled or disjoint Issue push permits the
original Work effect. The ordinary per-write material check repeats this review
before every Apply, including nested result-reporting actions. Lookup uses existing
retained preparation/admission/link/mapping records and indexed keys, without a new
association or reporting journal.

For triage's Work-backed external producer, explicitly disable overlapping Issue
push. Local Issue status, accepted classification, Run state and provider delivery
remain distinct. A policy conflict leaves the accepted result readable. A later
link or mapping never supplies a replacement native target or changes accepted
Work effects. A previously accepted Issue-push intent also keeps its original
mapping and effects after push is disabled; existing ordered delivery settles
these historical operations without reopening or retargeting them.

## Delivery receipts and recovery

The ordinary ordered outbox retains each configured effect's operation identity,
accepted record digest, exact input/caller chain, original target and retry/deadline
bounds. An intermediate action may queue before the reporting subflow returns;
the final subflow result and completed Run still retain the original accepted
decision. Pending, forbidden, exhausted or superseded delivery never rewrites that
result or Work scheduling metadata and never starts another Run.

Recovery recreates the ordinary store/projector/worker against retained state and
inspects the same original intent before deciding whether another conditional
native step is needed. A lost reply after an applied write is confirmed through
observation, without replaying a completed addition/removal/state change. Current
writer authority is checked before provider calls and again before each Apply;
revocation stops confirmation/delivery without undoing an already applied effect.
Temporary outages may exhaust only the original finite policy. A malformed Inspect
response is a protocol failure, not permission to write or a successful empty read.

GitHub's installed `managed_label_groups` remain native ownership units. Give
independent reporting dimensions separate groups; state has its own scope. Groups
do not imply clearing other labels. Existing item-wide ordering preserves disjoint
fields, detects supersession of overlapping older intents and waits for uncertain
older writes under the original fences. No triage-specific ordering journal or
retry loop is added. The selected installation still requires qualification; the
generic effect compiler does not change native group definitions.

An installed Reporter's editable policy declares `ownership.labels` (each literal
label's complete native scope set) and `ownership.state` (the common native state
scope set). These are installed provider identities, stable across writer profiles
and resource aliases for the same native item. They must match the qualified native
adapter configuration; assigning a new name to a label does not create a new scope.
For example, two GitHub labels in one `managed_label_group` name the same scope,
including when they are selected by different dimensions. State and genuinely
separate groups use disjoint scopes.

Composition rejects overlapping coactive scopes. Missing ownership metadata means
unknown: legacy single-producer effects remain supported, but multiple coactive
producers require complete installed declarations. Compiled actions freeze their
scope claims and normal publication/dispatch checks them against the exact installed
policy. Canonical Issue mapping publication freezes the same installed declaration
under its existing content digest; Source delivery compares it with its selected
Reporter's actual scopes before provider I/O. Neither declaration grants write
authority.

Publication and preview do not enqueue Reporter writes. Completing a Run with an
explicit preservation selection (`None`) also emits no effect, even when a writer
is installed. Delivery requires an accepted, explicitly configured action.
Contract/process tests qualify these reporting boundaries; the complete packaged
two-agent journey remains task 8.1.

## Optional bounded reproduction command

`with-reproduction-v1.json` composes the same two reasoning stages and gates with
one ordinary `build_command` between accepted eligibility and classification.
It requires a separately selected installed `reproduction` profile. The template
intentionally has no invented `reproduction_report` schema: bind that alias to
the selected command's `observation_contract.report_schema()` before ordinary
instantiation/publication. Root limits cover five nodes plus one explicit command
retry: six attempts, 360 seconds and 6 MiB output, with the same token/cost ceilings
and one active node. These are editable limits, not added execution permission.

An installed command profile can opt into `observation_contract`, containing exact
`environment` and `criterion` references, a closed bounded `observations` schema,
a `completion` predicate and a `symptom` predicate. The existing bounded gate
interpreter evaluates coverage first and then the symptom. A completed applicable
sample yields `matched` or `not_matched`; unknown coverage yields `indeterminate`.
The contract and report schema are validated before dispatch and included in the
frozen command task digest. Omitted contracts preserve legacy command digests and
plain-report behavior. No model output or generic exit-code parser implements
this verification.

The qualified installed program emits the canonical `factory.command-observation`
report envelope: original command `input_digest`, exact `subject`, `command`,
`environment`, `criterion`, `status`, optional structured `observations`, and
`finding`. The report must agree with the independently evaluated finding.
`environment_unavailable` and `needs_input` may omit observations, but must remain
`indeterminate`. A completed report requires observations. Environment/criterion
identifiers prove consistency with reviewed configuration; the qualified program
must actually observe readiness and the expected symptom. Unit fixtures alone do
not qualify a real environment or probe.

Select a canonical primary Report output using `observation_bindings()`, which
returns only safe immutable command/tool/plugin/task/configuration/schema and
criterion metadata. Put the exact selected binding in the analyst mapping's
zero-or-one `command_policy` array. The supplied projection uses the explicit
`reproduction-report` output name; select that installed logical name, or edit the
reviewed projection paths and command binding together for another output name.
This is ordinary draft authoring, not runtime name dispatch. Until the schema,
policy, output binding, Context, permissions and installed capabilities agree,
publication/compilation/gate checks fail closed.

The ordinary output owner checks the actual Build/Attempt/latest Job, immutable
compiled program and input, output publication/content/schema and producer pins.
The command must use the exact Work code subject; a candidate checkout cannot
claim evidence about the admitted base revision. The analyst receives only an
explicit safe header from this accepted command record: finding/status, exact
policy and subject/input scope, freshness, record digest and actual producer IDs.
Raw probe observations and private command Context remain retained under their
ordinary owners. The gate reads that frozen analyst input, never model-authored
verification metadata, and checks exact selected policy, command-input scope and
both command/model freshness deadlines before accepting either finding.

A feature may use `not_applicable`; a defect without applicable proof can remain
`unknown`, `needs_research` or `needs_input`. A negative finding concerns only its
reviewed criterion; it does not prove already-fixed, verification-only or global
absence. Unsupported claims go to review. For defects, command status
`environment_unavailable` or `needs_input` requires `human_review` before any
model-recommended route, including rejection. A feature with `not_applicable`
reproduction does not require a defect reproduction environment. No route starts
deep research or another pipeline implicitly.

A wrapper that completes its qualified preflight can publish an indeterminate
unavailable/input report. A killed/timed-out command, checkout/dependency error or
generic unsuccessful execution instead retains an ordinary technical stop and
usage; it supplies no semantic finding even if report bytes were published.
The Flow waits for operator review, using the existing exact retry command with
`max_execution_retries: 1` and original overall limits. There is no automatic retry,
synthetic completion or provider effect. Contradictory successful Job terminal
facts (`timed_out` or a failure class) are rejected.

## Reviewed eligibility policy

The Project result declares fit (`fits`, `out_of_scope`, `inconclusive`),
uncertainty (`clear`, `needs_input`, `review`), a nonempty reason of at most
2,048 bytes and a boolean `duplicate` assessment. The selected candidate appears
once, in its verified native detail claim. The final conclusion allows at most
two claims: one chosen candidate detail and one bounded coverage value, in either
order. The required coverage claim leaves only one possible candidate claim;
additional claims are rejected rather than selecting an ambiguous candidate.
Multiple native queries/reads and their evidence remain under the shared 64-call
capture ceiling. Each claim's JSON value retains the shared 64 KiB bound.
The common verifier retains exact native observations and never turns model
assertions into `duplicate_proven` or `global_absence_proven` facts.

The release policy selects the `issues` connection's native `read_issue` response
at `/issue` and `search_issues` response at `/coverage/has_more`, decoded from text
content item zero. The detail claim must resolve to the actual observed entity;
the latter claim must be the observed JSON value `false`. JSON whitespace and
object member order do not affect the decision: the shared verifier checks the
native value structurally and the gate selects its claim without a second JSON
copy in assessment. These are editable reviewed tool
contract literals, not Rust dispatch rules or a common external-provider query
format. Native identities/revisions, original arguments, scope and observation
time remain under the retained trace/response references. A completed bounded page
does not establish a global absence of duplicates or a consistent Project snapshot.

Select `factory_mcp_evidence_verifier()` as the Project Build binding's exact
`result_tool`, with the qualified native executor and `codex-run-result` output.
The gate projects its actual producer and exclusive freshness deadline from the
accepted predecessor's owner metadata, independently of its model-authored payload.
It requires the exact verifier pin, matching Build/Attempt/Job, fresh output,
completed capture/calls and the declared bounded coverage claim. Missing owner
metadata blocks input preparation; unknown or unsatisfied policy follows review.
No separate Project `facts` report or duplicate lookup owner is required.

| Assessment under the reviewed prerequisites                  | Gate outcome   | Result                                  |
| ------------------------------------------------------------ | -------------- | --------------------------------------- |
| Clear fit with an exact observed candidate                   | `duplicate`    | `rejected`, no analyst                  |
| Clear fit without a proposed duplicate                       | `classify`     | `decision: eligible`, analyst may start |
| Clear out-of-scope assessment without a proposed duplicate   | `out_of_scope` | `rejected`, no analyst                  |
| Explicit missing input                                       | `needs_input`  | `project_review`, no analyst            |
| Inconclusive, conflicting, incomplete or unresolved evidence | `review`       | `project_review`, no analyst            |

The default Project Profile seed supplies these evidence-format instructions and
a complete model-facing example to the compiled invocation. The example includes
the native Issue's content/status fields as well as identity/version, and is only
illustrative; actual arguments and returned values must come from execution.
Models return `assessment` and `claims`, without server-generated references or
verification metadata. A different reviewed native tool contract requires updating
both the selected Profile instructions and policy before a new publication.

Fit remains a Project assessment under the required goals, any selected rules and
reviewed acceptance rule, rather than an independently proven semantic fact.
Every gate result retains its finite decision, eligibility flag, bounded reason
and exact evidence references. The analyst handoff projects only these declared
fields alongside the original task/completion; it excludes raw claims, lookup
responses, private Project Context and verification metadata. No eligibility
outcome itself invokes an Issue or Reporter mutation.

For another compatible MCP server, review the native detail/coverage selectors
and tool literals against its qualified read-only contract before publication.
A candidate selector must identify an actual authorized entity, not query echo,
prose or an unqualified summary. Select every required coverage/truncation signal;
a server with missing, ambiguous or unverifiable required coverage must remain in
review. Keep credential/resource restrictions in the selected Runtime/Profile and
server authority. Editing this policy cannot qualify an unverified external server,
provide credentials or enable a write tool.

## Completion document

`completion-v2.json` is the concise process-completion draft for Project → Systems
analyst triage. It describes retained decisions, classification and unresolved
facts; execution instructions remain in Roles/Profiles and enforced contracts.
`completion-v1.json` remains unchanged for previously reviewed selections.
Each immutable reference hashes the exact UTF-8
`content`, including line endings. `SourceCompletionDocument::review_preset`
binds it to a Project without publishing bytes or creating a Build. A later
explicit Source preparation selection publishes ordinary private Work inputs.

Console shows a short localized summary and keeps the exact criteria under
**View completion criteria**, collapsed by default. AI/MCP/Reporter access guidance
belongs to execution readiness.

The resulting acceptance-role Artifact has `purpose: process_completion`; its
provenance identifies this document separately from the original ticket. A raw
ticket keeps its missing body and source acceptance. Existing field/body/heading
mappings retain their original required-content behavior.

An operator may instead select an exact already published private Project text
input. The common reader verifies its Project, reference, bytes and retention.
The complete two-stage preset resolver, setup and packaged execution are delivered
by the subsequent tasks of `add-factory-triage-pipeline`.

## Reviewed Project context

`TriageProjectContextDraft` compiles a reviewed same-Project selection to an
ordinary `FlowContextBinding`. Goals are required exact Artifact references;
controlling Project rules are optional and supporting documents use explicitly
selected aliases. Missing goals and a foreign draft Project have distinct safe diagnostics.
Omitted or null rules add no policy entry or placeholder document. Explicitly
selected rules retain their normal ownership, integrity and retention checks.
The common binding and Context Manifest enforce their existing size/count bounds.
Compilation performs no document I/O and grants no read or execution authority.
The common Artifact reader still checks ownership, exact content, publication
and retention when Context is prepared or consumed. An unavailable input reports
its logical name and original safe failure class without document/backend text.

Apply `bind(project_id)` to the ordinary Project-stage template's
`context_binding` before publication. The bundled `v1.json` already binds `task`,
`completion` and Work scheduling metadata to its `project_input` contract; retain
that input binding. For a custom template with just the two Artifact fields,
`ticket_binding()` loads `ticket-input-v1.json`. In both cases, `task`
and `completion` come only from the admitted Work, while the selected code
Repository/revision stays in the immutable Work/node subject. A source ticket
repository, ticket text or prior model output cannot replace that subject.
The latter fixes `project-goals` as specification data, any selected `project-rules`
as controlling policy and each supporting `document.<alias>` as specification data.
All selections are unconditional; a document alias cannot replace `task`.

The default stand Profiles require only `task`, `completion` and `project-goals` in their
ordinary `context_requirements`. A reviewed custom Profile may explicitly require
rules; it still cannot execute without that selected input. The role package retains exact inputs and rules;
Role display names and supporting text grant no tools, credentials or permission
to waive controlling rules. Changing a draft requires a newly reviewed ordinary
publication; retained bindings/packages keep their original identities.
Do not forward this whole Context implicitly to the analyst: the manifest selects
its accepted gate handoff separately, and its Profile declares its own Context.

## Retained operator diagnostics

Newly published eligibility gates expose their reviewed accepted/review/rejected
projection through the existing public-result diagnostic contract. Classification
keeps its separate accepted summary. Raw reasoning observations, claims, tool
arguments and Context bytes remain private. Eligibility's optional `lookup`
retains native observation references, consumed connection/resource pins and
capture/coverage metadata. The analyst input explicitly selects only `accepted`,
`decision`, `reason` and `evidence`; it receives none of this operator lookup view.
Previously admitted definitions keep their original disclosure policy.

Public completion diagnostics expose safe producer Call metadata from the actual
executing Build and explicitly selected predecessor records. This identifies the
original attempt, frozen Profile/tool/Build binding and Context identity/digest;
it does not expand Context or infer an agent from a logical stage name. Historical
results without retained producer metadata show that absence explicitly.

The Console shows Project and analyst assessments independently of technical Run
state. Reproduction assessments remain separate from retained command evidence,
its exact subject/environment/criterion and original validity limit. Dependency
links retain their original Run/Work identities. Original Issue navigation keeps
current local workflow status separate from historical assessments and Reporter
receipts. The `effect-delivery` collection reads the latest immutable receipt per
logical configured effect through the existing Run diagnostic endpoint and store
port. Pending/claimed receipts do not establish delivery; terminal failure reasons
remain visible without changing an accepted assessment. Reads never dispatch an
agent, retry an effect, or grant workflow/provider write authority.
