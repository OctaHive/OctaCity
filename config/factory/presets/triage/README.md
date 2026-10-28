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
`classification-gate-v1.json`. Its independent `facts` report remains an explicit
execution prerequisite. The manifest imports no spec, research or development
nodes or optional spec-result schema closure. The Project gate terminates at rejection or review,
or admits the analyst. Every analyst gate result terminates at `triaged` or
`human_review`; downstream routes remain recommendations in the result.

Published definitions pin schemas, policies, Context, selected Profiles and budgets.
Editing release files or Role seeds requires a reviewed new publication and cannot
modify an admitted Flow's retained closure. Keep the prior configuration version
available when reverting a draft or selecting a previous published version.

The versioned template and reviewed assessment policies cover tasks 5.1–5.3.
Reproduction verification
(5.4), the shared management preset resolver/readiness (7.1–7.2) and packaged
execution qualification (8.1) remain separate requirements. This manifest alone
does not establish operational readiness or provide a facts verifier.

## Reviewed analyst results

The closed analyst contract includes advisory `priority_assessment` (integer
0–100), `work_kind`, `severity`, `size`, `component`, `risk`, exact `dependencies`,
`recommended_route` and preliminary `reproducibility`. Component and route
vocabularies live in the editable schema; all projections reuse that exact shape.
Changing a vocabulary or priority range requires a reviewed new publication.
Dependencies are bounded to 64 exact Run/Work/digest/terminal references. A
reference is advisory data, never an implicit read grant or scheduling dependency.

The selected ordinary Build binding must declare the independently verified
`facts` report with the exact `classification_facts` schema and installed
tool/plugin pins. It contains `assessment`, the producing input digest, and
independent `reproduced`, `already_fixed`, `verification_only` facts. An arbitrary
second model response or copied JSON does not qualify this verifier. Dependency
references and other facts must be checked by that trusted producer; the gate
requires exact agreement with the analyst assessment. The common Build result
owner checks configured schema/tool/plugin and Build/Attempt applicability,
retaining actual Job provenance; independent verification may use another Job.
The gate takes provenance and both freshness deadlines from retained owner
metadata, and the expected input digest from the original reasoning record.

Feature requests require advisory severity and `not_applicable` reproduction.
Defects preserve `unknown`, `needs_research` or `needs_input`; incompatible
combinations, substituted or stale evidence and unsupported reproduction claims
go to `human_review` with `accepted: false`. A supported unresolved defect may
finish at `triaged` with a research recommendation. High assessed or admitted
risk also requires review. Negative reproduction verification remains task 5.4;
until that verifier exists, `not_reproduced` fails closed. Deep research stays a
separate Flow. No recommendation starts another execution or external action.

The gate summary retains the assessment and the original admission priority/risk
separately. It never changes immutable Work/Build queue priority, Issue planning
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

Fit remains a Project assessment under the selected required goals/rules and
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

`completion-v1.json` is the versioned, reviewable process-completion draft for
Project → Systems analyst triage. Its immutable reference hashes the exact UTF-8
`content`, including line endings. `SourceCompletionDocument::review_preset`
binds it to a Project without publishing bytes or creating a Build. A later
explicit Source preparation selection publishes ordinary private Work inputs.

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
ordinary `FlowContextBinding`. Goals and controlling rules are required exact
Artifact references; supporting documents use explicitly selected aliases.
Missing goals/rules and a foreign draft Project have distinct safe diagnostics.
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
The latter fixes `project-goals` as specification data, `project-rules` as
controlling policy and each supporting `document.<alias>` as specification data.
All selections are unconditional; a document alias cannot replace `task`.

Declare the same required identities in the selected Profile's ordinary
`context_requirements`. The role package retains those exact inputs and rules;
Role display names and supporting text grant no tools, credentials or permission
to waive controlling rules. Changing a draft requires a newly reviewed ordinary
publication; retained bindings/packages keep their original identities.
Do not forward this whole Context implicitly to the analyst: the manifest selects
its accepted gate handoff separately, and its Profile declares its own Context.
