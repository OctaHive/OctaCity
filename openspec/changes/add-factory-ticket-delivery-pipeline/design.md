## Context

See `proposal.md` for scope. Triage now has an ordinary manually authorable path alongside the preset. It supplies accepted eligibility/classification, sizes `small|medium|large` and retained reproduction observations when the published graph declares those structured outcomes. Its completion criteria describe triage. Implementation acceptance comes from the Issue and the delivery requirements contract.

The repository contains requirements, protected tests, implementation, review and validation templates under `config/factory/flows/base-flow/`, plus their application handlers and tests. The visual Flow editor, Project documents, explicit Work task and accepted-result connections, Project launch/access policies and Issue outcome cards are the existing operator path. See [manual Factory authoring](../../../docs/operations/factory-pipeline-authoring.md).

Delivery currently supports Observe, Publish and Cancel; merge is deliberately absent from `server/protocols/octacity-delivery-protocol/src/request.rs`. The main VCS spec keeps merge external and manual; this change preserves that contract. The main Dark Factory requirements contract currently requires independent specification review for larger Work and needs an explicit delta for the requested compact pipeline. Existing behavior is not evidence that this downstream journey is already usable.

## Goals / Non-Goals

**Goals:** Compose one downstream pipeline that an operator can build from an empty graph with the requested order, durable handoff from triage and a published PR ready for manual merge. Keep one reasoning invocation per applicable responsibility and one independent code reviewer. Extend existing public owners only for missing behavior.

**Required execution guarantees:** Configure, enforce and verify invocation and permission isolation for every stage. This includes independent workspace, Artifact, tool/command, network and credential access, including between stages sharing one Role. Reuse and extend the existing runtime boundaries where needed to deliver these guarantees. Immutable candidates and recovery remain required runtime contracts.

**Non-Goals:** Another scheduler, stage-specific lifecycle store, copy of the entire base-flow, mandatory full specification-review ceremony, reviewer quorum, feature-research branch, automatic merge, deployment or new forge providers.

## Decisions

### 1. Separate downstream execution with an exact triage predecessor

Compose selected base-flow parts with ordinary gates, Build nodes, subflows and trusted actions. The operator can author the graph with **Add pipeline**, publish an exact Flow version and select it in an ordinary Factory Configuration. Offer `config/factory/presets/ticket-delivery/` through the existing resolver as an optional example using the same publication and runtime validation. Flow/step names and preset identity carry no runtime dispatch authority.

Use the same retained predecessor binding for explicit Issue execution and separately enabled automatic intake. Select the exact eligible accepted result on the same Project and Issue, with compatible ticket requirements revision and code subject. Freeze the producing Flow/Run, outcome identity and digest with the original ticket and implementation acceptance at admission. Project only declared fields such as kind, size, component, priority and reproduction provenance. Use the existing preparation/admission owners and retain a distinct downstream Work/Run. Missing, inaccessible, stale or incompatible results block admission with an actionable reason. Stable admission identity deduplicates repeated observations and replay of the same handoff; label edits cannot create another attempt. The card shown on the Issue is a presentation of a retained result and supplies no admission authority.

When the Source observes a ticket that already has a scoped local Issue link, preparation retains the Issue revision whose title and description match the observation. The downstream Work takes implementation acceptance from that Issue revision. Older associations without a proven revision remain historical. Admission checks the Issue and the producer's retained record in the same transaction that publishes the downstream Run.

Automatic intake resolves the native ticket through its existing Issue link and looks for an accepted execution tied to the current requirements. Preparation retains the digest of that result and verifies the original task and implementation acceptance before publication. A ticket awaiting a qualifying result can be observed again during reconciliation. Other tickets on the same page can proceed.

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

Use an explicit Reporter action on the accepted triage outcome. When configured, triage requests `need reproduce` for `not_reproduced`, `unknown` and `needs_research` defects when it accepts classification, even if downstream delivery is disabled or has not admitted the ticket. The delivery pipeline does not add this label; it consumes the retained triage outcome and later removes the label after deeper reproduction. `needs_input` remains paused rather than entering development. Already reproduced defects and features bypass deeper reproduction. The downstream route reads the accepted result even if the triage label update is pending.

Use the existing research/command boundary to retain symptom, environment, exact subject, checks and outcome. A completed negative check selects terminal `cannot_reproduce`. If Reporter is configured, request `can't reproduce`, removal of managed `need reproduce` and open state through the existing Reporter/outbox. Successful reproduction continues and requests removal of the managed `need reproduce` label when configured. Technical failure never selects a negative finding. Preserve unrelated labels. Map these exact default strings to existing installation labels during setup; do not create labels implicitly. The terminated downstream input remains deduplicated so a poll cannot restart it.

Alternative: making a failed shell invocation mean non-reproduction would conflate unavailable execution with a checked symptom.

### 4. Freeze requirements before independent tests

For large tickets, an isolated profile invokes OpenSpec authoring to produce proposal, delta specs, design and tasks in an owned workspace, validates their coherence and captures the requirements ChangeSet. The authoring invocation ends there. The orchestrator dispatches test authoring and implementation separately; it does not ask a propose skill to implement within its own planning turn. The final candidate includes those requirements files. A later requirements correction creates a new existing Workflow Cycle and invalidates descendant test/review results.

For non-large tickets, freeze an Accepted Work Contract from explicit ticket requirements and triage. The triage process-completion document is never implementation acceptance. Missing expected behavior pauses for input. This delivery graph omits a separate independent specification-review invocation; other published Flows may retain one. Update the generic requirements policy contract accordingly rather than silently violating the current main spec.

The test-authoring invocation receives only the selected requirements and base context. Capture its tests using existing protected-test publication, then demonstrate the expected failure on the base. Harness/setup errors stop preparation. Already satisfied acceptance yields a retained no-change stop, without an empty fix PR. Implementation receives the public scope summary and requirements, not protected tests or the author's transcript. Developer-visible tests remain part of implementation TDD; acceptance isolation does not exempt the coding agent from repository rules.

The test step can also select uploaded files or a directory, or paths at an exact revision of a separate Project repository. Retain portable relative paths so scripts can use sibling fixtures. The operator selects Octafile tasks or shell entrypoints and can describe the setup in the Tester assignment. Freeze the selected bytes and entrypoints alongside the generated tests before baseline execution. Files used as fixtures are only mounted. Files selected as entrypoints are executed in the isolated Build.

### 5. Review and validate a captured candidate

Use existing trusted ChangeSet capture, review planning and protected-test executors. One reviewer sees the candidate and requirements, not the coding transcript. Then deterministic validation materializes the same candidate and original protected bundle and runs the required repository checks. A passing review is not a substitute for execution. Forward only bounded remediation that does not disclose protected assertions. Any implementation rework changes the candidate and repeats both review and validation within the original budget. Requirements/test defects pause for an explicit correction cycle rather than letting the implementer edit the protected bundle.

### 6. Publish the accepted candidate for manual merge

Reuse the existing Observe/Publish/Cancel Delivery protocol and `octacity-github` publication capability. DevOps prepares the public PR metadata from the selected task, accepted requirements, review and validated revision. The scoped trusted action publishes and observes the PR. Only its confirmed receipt can complete Delivery; confirmation needs no further model invocation. AI invocations receive no direct forge-write credentials. The trusted adapter publishes only the accepted candidate under the existing target policy; no merge command, merge credential or merge policy is added.

Retain the original publication identity, exact repository/base/candidate, Decision and PR reference under the existing Delivery operation/store owner. Recover unknown responses through observe-before-retry so a successful remote publication returns the same PR. A publication conflict or failure remains visible and does not claim successful delivery. A replacement candidate must pass review and validation before publication.

Successful Delivery ends in `delivered_for_review` with the PR URL and publication receipt. A human reviews and merges it in the forge under repository rules, including for production branches. This change adds no in-pipeline merge approval or subsequent automatic merge, and does not require a merge-status monitor to complete the journey. PR publication must not be reported as merged code or automatically completed implementation of the ticket. Configured reporting may project delivery for review separately from merge.

### 7. Author, publish and inspect through the common operator path

Build the named Flow visually. Set editable process goals and completion criteria when useful. Publish shared guidance in Project documents, then select its exact version and purpose in each recipient step. Configure control transitions and typed completions separately from accepted data connections. Derive result formats, effective budgets and access requests through the existing editor where supported, preserving manual overrides and reviewing the effective constraints before publishing.

Publish the Configuration with the exact Root Flow, launch policy and access profile. Prefer the reviewed Project defaults when they satisfy the Flow. Keep automatic intake disabled for the first manual trial. Configure Reporter independently from admission, and require explicit selection of the trusted publication target and policy. Use existing validation/readiness and diagnostics for missing installed capabilities and stopped or waiting execution.

Opt in to outcome cards on the specification, review and confirmed publication outcomes. Use the existing Issue Outcomes section for a short conclusion, visible specification and review links, and the original Run. For a direct PR link, extend the existing typed reference projection only if needed to resolve an authorized retained Delivery receipt. Never accept a model-provided URL as a verified publication link. Run details hold branches, selected Profiles, safe input summaries, exact approval requests, public test scope, validation and publication receipts. Use the existing Run journal to show where a step stopped or waits, including deadline, terminal usage availability and retry disposition where retained. A timeout or missing terminal usage cannot be reported as a completed step or successful publication. No private transcript, protected test content or raw runtime output belongs in a card.

### 8. Bind AI roles and input data separately for every stage

Pin a compatible Project Role/Profile per stage. The example uses `Tester`, `Coder`, `DevOps` and `Analytic` as role names. The manually authored graph can select existing roles with equivalent capabilities. Keep reusable methods and skills in the Profile and the actual assignment in the step. Select all task, document and predecessor inputs independently of the Profile. The implementation and review calls are separate isolated invocations, including when they select the same Coder Profile.

| Stage | AI role | Stage-specific inputs and access |
| --- | --- | --- |
| Deeper reproduction | Tester | Ticket symptoms, accepted triage and applicable environment/configuration; selected source fragments only when required by the reproduction method. |
| Requirements/specifications | Analytic | Ticket, triage, Project rules and selected documentation/configuration; repository fragments only when explicitly selected. |
| Acceptance-test authoring | Tester | Frozen requirements, base/API contracts, selected test setup/configuration and optional source fragments; no future implementation or its transcript. |
| Implementation/fix | Coder | Accepted requirements, allowed source workspace, selected configurations and public reproduction/test summaries; no protected tests or test-author transcript. |
| Independent review | Coder | Exact candidate/diff, accepted requirements, selected source/configuration and public findings; no implementation transcript or protected assertions. |
| Test execution/validation | Tester | Candidate identity or endpoint, test plan, selected environment/configuration and actual execution reports. The trusted runner mounts the protected bundle and candidate required by the test method; AI source/bundle visibility is separately configured and can be omitted for black-box validation. |
| Delivery | DevOps | Original task, accepted requirements summary, review and validated revision for metadata preparation. The trusted action independently receives the exact candidate, validation freshness and installed target policy. |

The table supplies defaults and allowed inputs, not an implicit grant of every item. Configure source visibility (`none`, selected revision-bound ranges, or an allowed workspace), selected predecessor outputs and extra Artifact/configuration files independently for each stage using `FlowContextBinding`, mounts and tool permissions. Missing required inputs block only their consumer; a control edge or shared Role never forwards all predecessor data. Profile context requirements must agree with these bindings. Retain the resolved Context Manifest for each invocation. A tool cannot retrieve omitted source or artifacts through a broader undeclared connection.

Tester authoring requires an AI invocation. Factory always dispatches the mandatory CI/CD Build for the frozen verification plan on the exact reviewed candidate. The Flow may additionally attach a Tester invocation to analyze the retained verdict and perform explicitly authorized diagnostics. That analysis cannot replace the mandatory runner result or turn a missing, failed, timed-out or skipped required check into a pass. The downstream validation gate consumes the runner's verified report. DevOps hands only the prepared metadata to the trusted Delivery action. This guarantees execution of declared checks; it does not claim that those checks prove the absence of every defect.

On Microsandbox, the scoped Tester and its private reader reserve one whole
vCPU each inside the original two-vCPU Job allocation. The installed Runtime,
selected Profile, ordinary Build Configuration, enclosing requests and Agent
policy must agree on that allocation; the verifier's separately registered single-slot Agent is unchanged.
The reader's installed time ceiling is narrowed to the Job's remaining window.
Neither partitioning nor a new Profile version expands an already admitted Job.

The scoped tool observes the Factory-owned ordinary Build for the frozen verifier node. Bind it to the current Tester lease, admitted Run and Workflow Cycle and to one qualified verifier binding in that Flow. The call supplies no repository, revision, file, command or Build override. The normal Factory journal owns the verifier intent and the Build owner deduplicates acceptance of that operation. A retained `start` call remains idempotent for compatibility; the mandatory run never depends on it. A status read does not dispatch work. Factory completes the verifier before starting its bound Tester and runs that analysis before continuing the downstream gate. The two executions therefore need only one active Flow node slot. Private reader resource limits and the separate validation Agent remain independently enforced.

Isolation is an implementation deliverable of this change, including configuration and behavioral verification through the ordinary execution path. A Context Manifest or prompt instruction alone does not enforce access. Apply effective stage permissions to filesystem/mounts, Artifact reads, commands and child processes, network/MCP tools and credential bindings. Do not inherit another invocation's workspace, session, environment or secret access. If the selected backend cannot enforce a required restriction, stop dispatch without a permissive fallback.

Analytic writes only its allowed requirements paths and outputs; the test author writes its owned protected-test workspace; the implementation Coder writes allowed implementation paths. Review Coder can read the selected candidate but cannot alter it. Test execution uses isolated scratch/output paths without modifying the retained candidate or protected bundle. DevOps has no forge credential or merge tool. The Flow grants the trusted publication action its own access. All stages may write their explicitly allowed reports. These permissions are configured per stage and remain independent of display role names.

### 9. Configure human requirements acceptance

Pin requirements-acceptance rules in the ordinary published Flow policy. Rules combine component membership, triage size and priority ranges over the accepted triage fields (`priority_assessment` supplies importance); rules can select human acceptance or automatic continuation. They apply to the produced specification or the non-large Accepted Work Contract before any test authoring. There is no second sizing or priority assessment. A human sees the ticket, relevant triage fields and exact requirements/design candidate. Unmatched tickets continue automatically. Acceptance records the actor, exact requirement/contract digest, policy and current gate attempt. Rejection or a request for changes stops advancement and uses the declared correction route; changed requirements need a fresh acceptance and a new dependent test cycle.

Reuse the existing `HumanGate`, operator authorization and response journal. Requirements acceptance can use the existing escalation response binding (`Resume`/`Abort`) with the requirements node's logical presentation. Extend existing projections only for missing requirements/approval binding rather than introduce another approval subsystem. Freeze these policies at publication/admission. Pending requests survive restart and are visible in Console and headless diagnostics. Agents cannot approve their own gate or route around it.

There is no separate merge approval in this pipeline: after PR publication, the human performs merge in the forge. Production-branch review and approval remain governed by the repository's existing manual process.

## Risks / Trade-offs

- [A manually authored triage graph may emit only a text conclusion] → Require the compatible typed accepted outcome and explicit field projections at setup. Expose missing fields and let the operator publish the needed contract; do not reconstruct it from a card or label.
- [Independent protected tests require runnable project acceptance] → Pause preparation when expected behavior or environment is missing; never certify with triage completion criteria.
- [PR target advances while work runs] → Expose publication conflicts under the existing target policy; a replacement candidate must repeat review/validation. Automatic conflict resolution is outside this change.
- [Native repository policy requires human review] → Keep that review and manual merge in the forge; publication does not claim the change is merged.
- [Publication succeeds but its response is lost] → Observe the original stable delivery identity and return the same PR through existing recovery.

## Migration Plan

Qualify a manually assembled Flow first using existing publication and recovery contracts. Publish the optional example against the same validation and acceptance checks. An existing triage graph may need a new Flow version exposing the required outcome fields; its prior versions and admitted Runs remain immutable. Test manual and separately enabled automatic intake with hermetic model/provider fixtures and normal Agent/Build execution. Verify the Issue cards and original Run through `delivered_for_review`, with real external publication opt-in.


Rollback disables new intake and publication for future work; retain admitted history, PRs and publication receipts. Disabling does not close existing PRs or undo any manual repository changes.

### Native Codex workspace roots

The approved common adapter extension keeps the Octa task/resource directory at
`/workspace` and selects `/workspace/scratch` as the native Codex working
directory. The compiler adds `/workspace/output` as writable and adds
`/workspace/source` only for an admitted writable source. Read-only and no-code
consumers receive no writable source root. Prompts name the absolute source and
output paths. This avoids Codex attempting to create its protected metadata
beneath the intentionally read-only parent; the parent and mount policy remain
unchanged.

The plugin advertises `codex.execution-workspace.v1`. Its optional task setting
accepts only bounded, existing, normalized workspace-relative directories and
rejects links and escapes. Older plugins reject the setting; native installation
qualification also requires the advertised capability. The local stand uses the published Octa 0.9.0 release, verified against its
pinned archive digest and GitHub build provenance. The immutable installation
retains the upstream plugin, manifest, lock and compatibility data unchanged.
Server builds use the same pinned upstream source without a local patch. Previous installations and published profile bindings are retained;
new compatible Delivery profiles use separate identities rather than replacing
the installed declarations behind an existing profile.
