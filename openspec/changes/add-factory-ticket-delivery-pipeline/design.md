## Context

See `proposal.md` for scope. The sibling triage change supplies accepted eligibility/classification, sizes `small|medium|large`, reproduction observations, generic preset resolution and configured Reporter effects. Its completion document certifies triage only. It deliberately stops before implementation.

The repository already contains requirements, protected tests, implementation, review and validation templates under `config/factory/flows/base-flow/`, plus their application handlers and behavioral tests. `FactoryPresetHandlers` currently exposes the released triage preset. Generic Flow configuration, Context projections and accepted contracts are the composition seam.

Delivery currently supports Observe, Publish and Cancel; merge is deliberately absent from `server/protocols/octacity-delivery-protocol/src/request.rs`. The main VCS spec keeps merge external and manual; this change preserves that contract. The main Dark Factory requirements contract currently requires independent specification review for larger Work and needs an explicit delta for the requested compact pipeline. Existing behavior is not evidence that this downstream journey is already usable.

## Goals / Non-Goals

**Goals:** Compose one usable downstream preset with the requested order, durable handoff from triage and a published PR ready for manual merge. Keep one reasoning invocation per applicable responsibility and one independent code reviewer. Extend existing public owners only for missing behavior.

**Required execution guarantees:** Configure, enforce and verify invocation and permission isolation for every stage. This includes independent workspace, Artifact, tool/command, network and credential access, including between stages sharing one Role. Reuse and extend the existing runtime boundaries where needed to deliver these guarantees. Immutable candidates and recovery remain required runtime contracts.

**Non-Goals:** Another scheduler, stage-specific lifecycle store, copy of the entire base-flow, mandatory full specification-review ceremony, reviewer quorum, feature-research branch, automatic merge, deployment or new forge providers.

## Decisions

### 1. Separate downstream execution with an exact triage predecessor

Publish `config/factory/presets/ticket-delivery/` through the existing resolver and readiness/setup surfaces. Compose selected base-flow parts with ordinary gates, Build nodes, subflows and trusted actions. Logical stage names do not dispatch Rust code.

An enabled downstream input consumes the accepted triage result through retained explicit projections and admits a distinct Work/Run associated with the same Issue/native ticket. An explicit Issue execution request uses the same input contract. Extend existing preparation/admission with that predecessor binding if needed; do not create an extra queue or journal. Bind kind, size, eligibility, reproduction evidence, original expected behavior and selected code subject. Reject source or subject mismatches. Existing stable input identity deduplicates polling and restart; label edits cannot create another attempt. Enable this intake once at setup, independently of triage.

Alternative: chaining implicitly on triage completion would erase its separate activation boundary; using labels as the accepted contract would lose the assessment and its provenance.

### 2. Use this compact graph

```text
triage: unresolved defect -> request need reproduce label
  -> accepted triage handoff to the separately enabled delivery pipeline
  -> unresolved defect? -> deeper reproduction [Tester]
                           -> cannot reproduce: label + leave open + stop
                           -> unavailable/missing input: pause
                           -> reproduced: continue
  -> large? -> OpenSpec authoring [Analytic]
  -> freeze specification or Accepted Work Contract
  -> human requirements acceptance when component/size/priority policy matches
  -> isolated acceptance-test authoring [Tester]
  -> implementation / fix [Coder]
  -> independent code review [Coder, separate invocation]
  -> test execution [Tester] with trusted original-test validation + repository checks
  -> Delivery [DevOps] -> trusted PR publication
  -> delivered_for_review + PR link

human merge in the forge, including production branches, is outside this pipeline

review findings / test failure -> bounded implementation rework
                              -> review -> validation
```

`large` uses OpenSpec; both `small` and `medium` bypass it. That is the explicit interpretation of “not large.” No agent resizes the ticket. One deeper reproduction stage can run the commands it needs within its own budget; it is not an unbounded research loop. The default implementation rework allowance is one additional cycle, configurable at publication. The only human gate inside this pipeline is configured requirements acceptance. PR merge remains a manual forge operation. There is no separate AI spec reviewer, model-scored acceptance or repeated setup readiness node. Ordinary output-schema and candidate enforcement belong to existing stage completion.

### 3. Keep reproduction outcomes and label delivery separate

This change extends the triage preset's configured reporting policy after the sibling contract is available. Triage itself requests `need reproduce` for `not_reproduced`, `unknown` and `needs_research` defects when it accepts classification, even if downstream delivery is disabled or has not admitted the ticket. The delivery pipeline does not add this label; it consumes the retained triage outcome and later removes the label after deeper reproduction. `needs_input` remains paused rather than entering development. Already reproduced defects and features bypass deeper reproduction. The downstream route reads the accepted result even if the triage label update is pending.

Use the existing research/command boundary to retain symptom, environment, exact subject, checks and outcome. A completed negative check selects terminal `cannot_reproduce`; request `can't reproduce`, removal of managed `need reproduce` and open state through the existing Reporter/outbox. Successful reproduction removes `need reproduce` and continues. Technical failure never selects a negative finding. Preserve unrelated labels. Map these exact default strings to existing installation labels during setup; do not create labels implicitly. The terminated downstream input remains deduplicated so a poll cannot restart it.

Alternative: making a failed shell invocation mean non-reproduction would conflate unavailable execution with a checked symptom.

### 4. Freeze requirements before independent tests

For large tickets, an isolated profile invokes OpenSpec authoring to produce proposal, delta specs, design and tasks in an owned workspace, validates their coherence and captures the requirements ChangeSet. The authoring invocation ends there. The orchestrator dispatches test authoring and implementation separately; it does not ask a propose skill to implement within its own planning turn. The final candidate includes those requirements files. A later requirements correction creates a new existing Workflow Cycle and invalidates descendant test/review results.

For non-large tickets, freeze an Accepted Work Contract from explicit ticket requirements and triage. The triage process-completion document is never implementation acceptance. Missing expected behavior pauses for input. No independent specification-review agent is selected by this preset; other published Flows may retain one. Update the generic requirements policy contract accordingly rather than silently violating the current main spec.

The test-authoring invocation receives only the selected requirements and base context. Capture its tests using existing protected-test publication, then demonstrate the expected failure on the base. Harness/setup errors stop preparation. Already satisfied acceptance yields a retained no-change stop, without an empty fix PR. Implementation receives the public scope summary and requirements, not protected tests or the author's transcript. Developer-visible tests remain part of implementation TDD; acceptance isolation does not exempt the coding agent from repository rules.

### 5. Review and validate a captured candidate

Use existing trusted ChangeSet capture, review planning and protected-test executors. One reviewer sees the candidate and requirements, not the coding transcript. Then deterministic validation materializes the same candidate and original protected bundle and runs the required repository checks. A passing review is not a substitute for execution. Forward only bounded remediation that does not disclose protected assertions. Any implementation rework changes the candidate and repeats both review and validation within the original budget. Requirements/test defects pause for an explicit correction cycle rather than letting the implementer edit the protected bundle.

### 6. Publish the accepted candidate for manual merge

Reuse the existing Observe/Publish/Cancel Delivery protocol and `octacity-github` publication capability. DevOps prepares the PR metadata and a bounded publication plan from the accepted candidate and review/validation summaries, then uses scoped trusted tools to publish and observe the PR. AI invocations receive no direct forge-write credentials. The trusted adapter publishes only the accepted candidate under the existing target policy; no merge command, merge credential or merge policy is added.

Retain the original publication identity, exact repository/base/candidate, Decision and PR reference under the existing Delivery operation/store owner. Recover unknown responses through observe-before-retry so a successful remote publication returns the same PR. A publication conflict or failure remains visible and does not claim successful delivery. A replacement candidate must pass review and validation before publication.

Successful Delivery ends in `delivered_for_review` with the PR URL and publication receipt. A human reviews and merges it in the forge under repository rules, including for production branches. This change adds no in-pipeline merge approval or subsequent automatic merge, and does not require a merge-status monitor to complete the journey. PR publication must not be reported as merged code or automatically completed implementation of the ticket. Configured reporting may project delivery for review separately from merge.

### 7. Use existing setup and evidence views

Extend generic preset discovery/resolution to select this preset instead of creating another endpoint family. Console setup selects upstream triage input, per-stage roles/Profiles and data bindings, exact dependencies, reproduction program, label mappings, budgets, requirements-acceptance rules, PR target and publication configuration. One combined readiness report names missing capabilities. Existing Run/Issue views show conditional branches, selected agents and safe input summaries, links to requirements, public test scope, review, validation, PR and publication receipt, and authorized human requirements-acceptance actions with the exact version awaiting acceptance. The final delivery state links to manual merge in the forge. Protected bundle metadata is visible; its source is not.

### 8. Bind AI roles and input data separately for every stage

Select Project-owned Roles with display names `Tester`, `Coder`, `DevOps` and `Analytic` and pin a compatible Profile per stage. Names describe responsibilities; explicit immutable bindings select execution. Existing selected Roles can be reused. Stage Profiles may differ in runtime, model, instructions, tools and data despite sharing one Role. The implementation and review calls are separate isolated invocations, including when they select the same Coder Profile.

| Stage | AI role | Stage-specific inputs and access |
| --- | --- | --- |
| Deeper reproduction | Tester | Ticket symptoms, accepted triage and applicable environment/configuration; selected source fragments only when required by the reproduction method. |
| Requirements/specifications | Analytic | Ticket, triage, Project rules and selected documentation/configuration; repository fragments only when explicitly selected. |
| Acceptance-test authoring | Tester | Frozen requirements, base/API contracts, selected test setup/configuration and optional source fragments; no future implementation or its transcript. |
| Implementation/fix | Coder | Accepted requirements, allowed source workspace, selected configurations and public reproduction/test summaries; no protected tests or test-author transcript. |
| Independent review | Coder | Exact candidate/diff, accepted requirements, selected source/configuration and public findings; no implementation transcript or protected assertions. |
| Test execution/validation | Tester | Candidate identity or endpoint, test plan, selected environment/configuration and actual execution reports. The trusted runner mounts the protected bundle and candidate required by the test method; AI source/bundle visibility is separately configured and can be omitted for black-box validation. |
| Delivery | DevOps | Accepted candidate, review/validation summaries, PR/target policy, relevant configuration and applicable requirements-acceptance receipts; source is omitted unless explicitly needed and selected. |

The table supplies defaults and allowed inputs, not an implicit grant of every item. Configure source visibility (`none`, selected revision-bound ranges, or an allowed workspace), selected predecessor outputs and extra Artifact/configuration files independently for each stage using `FlowContextBinding`, mounts and tool permissions. Missing required inputs block only their consumer; a control edge or shared Role never forwards all predecessor data. Profile context requirements must agree with these bindings. Retain the resolved Context Manifest for each invocation. A tool cannot retrieve omitted source or artifacts through a broader undeclared connection.

Tester is a real AI-agent invocation for test execution as well as authoring: it uses scoped execution tools to run/observe the prepared plan and produce diagnostics. Executable tests and trusted reports determine pass/fail; its prose cannot turn failed, skipped or unexecuted required tests into a pass. These tools reuse ordinary Build/Job execution and the protected-test runner. DevOps similarly uses the trusted Delivery boundary rather than direct forge credentials or arbitrary shell publication.

Isolation is an implementation deliverable of this change, including configuration and behavioral verification through the ordinary execution path. A Context Manifest or prompt instruction alone does not enforce access. Apply effective stage permissions to filesystem/mounts, Artifact reads, commands and child processes, network/MCP tools and credential bindings. Do not inherit another invocation's workspace, session, environment or secret access. If the selected backend cannot enforce a required restriction, stop dispatch without a permissive fallback.

Analytic writes only its allowed requirements paths and outputs; the test author writes its owned protected-test workspace; the implementation Coder writes allowed implementation paths. Review Coder can read the selected candidate but cannot alter it. Test execution uses isolated scratch/output paths without modifying the retained candidate or protected bundle. DevOps can request scoped trusted PR publication, but has no direct forge credential or merge tool. All stages may write their explicitly allowed reports. These permissions are configured per stage and remain independent of display role names.

### 9. Configure human requirements acceptance

Pin requirements-acceptance rules in the preset's immutable policy. Rules combine component membership, triage size and priority ranges over the accepted triage fields (`priority_assessment` supplies importance); rules can select human acceptance or automatic continuation. They apply to the produced specification or the non-large Accepted Work Contract before any test authoring. There is no second sizing or priority assessment. A human sees the ticket, relevant triage fields and exact requirements/design candidate. Unmatched tickets continue automatically. Acceptance records the actor, exact requirement/contract digest, policy and current gate attempt. Rejection or a request for changes stops advancement and uses the declared correction route; changed requirements need a fresh acceptance and a new dependent test cycle.

Reuse the existing `HumanGate`, operator authorization and response journal. Requirements acceptance can use the existing escalation response binding (`Resume`/`Abort`) with the requirements node's logical presentation. Extend existing projections only for missing requirements/approval binding rather than introduce another approval subsystem. Freeze these policies at publication/admission. Pending requests survive restart and are visible in Console and headless diagnostics. Agents cannot approve their own gate or route around it.

There is no separate merge approval in this pipeline: after PR publication, the human performs merge in the forge. Production-branch review and approval remain governed by the repository's existing manual process.

## Risks / Trade-offs

- [Triage is being implemented concurrently] → Consume its accepted public result and reporting seams; keep this change's downstream and label-extension tasks here and leave sibling artifacts untouched.
- [Independent protected tests require runnable project acceptance] → Pause preparation when expected behavior or environment is missing; never certify with triage completion criteria.
- [PR target advances while work runs] → Expose publication conflicts under the existing target policy; a replacement candidate must repeat review/validation. Automatic conflict resolution is outside this preset.
- [Native repository policy requires human review] → Keep that review and manual merge in the forge; publication does not claim the change is merged.
- [Publication succeeds but its response is lost] → Observe the original stable delivery identity and return the same PR through existing recovery.

## Migration Plan

Compose downstream intake and the preset using existing publication and recovery contracts, then extend normal setup/views. Publish the new preset without modifying existing immutable configurations. Enable its input and PR publication configuration explicitly during setup. Qualify the journey through `delivered_for_review` with hermetic model/provider fixtures and the normal Agent/Build execution path; keep real external publication opt-in.


Rollback disables new intake and publication for future work; retain admitted history, PRs and publication receipts. Disabling does not close existing PRs or undo any manual repository changes.
