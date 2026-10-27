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
| `enable-local-stand-runtime-mcp` | Hard prerequisite for the standard stand's real reasoning execution, qualified Agent policy, tool materialization, permitted HTTP/stdio MCP and non-destructive Runtime installation updates. Consume it; do not implement another launcher or loosen the deny-all default here. |
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

Existing field/body/heading mappings remain strict. There is no fallback from a
missing required ticket acceptance to the triage document. Exact preparation
policy and authorized document references select the mode. Replacement after
freeze cannot alter pending/admitted inputs; retention covers the document for
the Run. This avoids optionalizing acceptance across all Work/Agent contracts or
building a second input journal.

### 5. Provide bounded, provenance-bearing context

The reviewed setup selects Project goals/rules/documents and their immutable
references, the ticket source scope and independently selected code Repository.
Project assessment receives the frozen ticket and declared Project context plus
bounded duplicate candidates. It cannot see a different Project or arbitrary
ticket body supplied by an ungranted MCP endpoint.

Use existing authorized canonical Issue reads/search and Source observations for
duplicate candidate acquisition. Implement only the missing bounded read-only
tool attachment through the existing MCP/tool host and management/integration
authorization. Return exact candidate identity, source/revision, observation time,
search scope and truncation/coverage; recheck grants after asynchronous reads.
Freeze consumed candidate facts into existing Context/input provenance. A global
embedding store, full external index or new credential broker is unnecessary.
If the installed scopes cannot supply the required lookup, readiness identifies
that tool/source blocker rather than supplying an empty successful search.

The analyst receives the original ticket, explicitly projected accepted handoff
and its own permitted repository/tool context. It does not inherit private stage
Context or hidden reasoning. Ticket bodies, repository files, tool results and
candidate descriptions are untrusted data, never role instructions or authority.
Goals missing from required Context, unresolved candidates and incomplete required
coverage follow declared review/needs-input policy. No bounded search claims a
global proof that duplicates do not exist.

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

Console composes existing Role/Profile, Source, workflow, effect and publication
editors into Project -> Triage setup -> review -> publish -> preview -> independently
enable intake. Existing input editing/disablement remains the lifecycle owner.
Multi-resource setup is resumable by ordinary receipts and exact versions; do not
promise one atomic transaction across publications or automatically enable a
partially prepared graph. Draft text and previews stay out of browser storage.

### 9. Qualify the real composition with finite external boundaries

Extend the packaged Source runtime journey with an actual configured verified
reasoning plugin and ordinary qualified Agent execution. Hermetic model/provider
endpoints can supply finite protocol-correct outputs, but cannot bypass the
compiler, materializer, permissions, Context selection, result verification or
outbox. Retain two distinct stage invocations/profile identities and actual
reproduction-command evidence. Run the same setup through HTTP and Console,
including narrow keyboard navigation and English/Russian.

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
- **A Source edit or another producer changes the same fields** -> Preserve frozen
  mapping/subject and use existing producer conflicts, ordering and version fences.
- **Current specs and overlapping changes drift** -> Record shared-contract ownership
  before implementing it; sync dependencies in order and never archive another change
  merely because this preset consumes its interfaces.

## Migration Plan

1. Verify upstream execution and Source contracts on the exact current baseline.
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
