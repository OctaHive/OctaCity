## Purpose

Defines the supported post-triage ticket pipeline from conditional reproduction and OpenSpec authoring through isolated test preparation, implementation, independent review, validation and pull request publication for manual merge.

## ADDED Requirements

### Requirement: Development consumes an accepted triage handoff

An explicit Issue execution request or separately enabled downstream intake SHALL bind an eligible accepted triage outcome to a distinct Work and Run for the same Project and Issue. Admission SHALL verify visibility, applicability to the Issue requirements revision, compatibility with the declared result contract and repository subject, then retain the exact source Flow/Run and outcome identity and digest. The downstream Work SHALL retain the original ticket and its own implementation acceptance inputs. Only explicitly selected triage fields and authorized evidence references SHALL be projected. Repeated observations of the same handoff SHALL reuse the logical execution under existing admission rules. Free-text conclusions, Issue cards, external labels and Reporter receipts SHALL NOT confer admission authority. Missing, stale, incompatible, rejected, review-pending or non-development results SHALL block admission with a reason.

#### Scenario: Accepted ticket enters delivery once
- **WHEN** an enabled downstream intake observes an accepted development-eligible triage result more than once
- **THEN** one downstream execution retains that predecessor and starts without rerunning triage or sizing

#### Scenario: An operator explicitly starts delivery

- **WHEN** an authorized operator starts the published delivery Configuration from the Issue with a compatible accepted triage outcome
- **THEN** admission freezes that exact handoff and original task without enabling automatic intake or requiring any Reporter delivery

#### Scenario: The ticket changed after triage

- **WHEN** the selected result belongs to superseded Issue requirements or an incompatible code subject
- **THEN** admission identifies the mismatched handoff and creates no implementation execution

#### Scenario: Label is added without an accepted result
- **WHEN** a ticket has a delivery selection label but lacks an applicable accepted triage result
- **THEN** no implementation is admitted and the missing handoff is visible

### Requirement: Unresolved defects receive one deeper reproduction stage

When an explicit Reporter action is configured on the accepted triage outcome, the triage stage SHALL request the mapped existing label `need reproduce` through its configured reporting policy when classification accepts a defect with `not_reproduced`, `unknown` or `needs_research` preliminary reproducibility. This triage effect SHALL be independent of downstream delivery activation or admission. The delivery pipeline SHALL NOT add `need reproduce`. Defects requiring missing input SHALL remain paused until input is supplied. Delivery SHALL use the retained reproduction outcome to run one deeper reproduction stage before size routing for unresolved defects; reproduced defects and features SHALL bypass it. This stage SHALL retain the reproduction checks and observed outcome against the admitted subject. A completed `cannot_reproduce` outcome SHALL end delivery with the ticket open. When configured, an explicit Reporter action SHALL request adding `can't reproduce` and removing the managed `need reproduce` label. A reproduced outcome SHALL continue and request removal of the managed `need reproduce` label when configured. Reporter SHALL preserve unrelated labels. Pending, failed and confirmed external delivery SHALL be distinguishable from the accepted local outcome.

Timeouts, missing dependencies, unavailable environments and insufficient information SHALL pause or technically fail the affected stage without asserting `cannot_reproduce`. The pipeline SHALL NOT repeatedly re-admit the same unresolved ticket after termination.

#### Scenario: Triage marks an unresolved defect while delivery is disabled
- **WHEN** triage accepts an unresolved defect classification with an explicit Reporter action and the downstream delivery pipeline is disabled
- **THEN** triage requests the `need reproduce` label through its Reporter policy without admitting a delivery execution

#### Scenario: Defect is still not reproducible
- **WHEN** deeper reproduction checks complete without observing the reported symptom and the declared Reporter effects are configured
- **THEN** its retained outcome is `cannot_reproduce`, Reporter requests the configured label changes, the ticket stays open and no test authoring, implementation or PR publication starts

#### Scenario: Deeper reproduction succeeds
- **WHEN** the deeper reproduction stage observes the defect
- **THEN** delivery continues to size routing with its reproduction evidence and requests removal of the managed label if configured

#### Scenario: Reproduction environment is unavailable
- **WHEN** the reproduction stage cannot prepare its required environment
- **THEN** the failure remains unresolved, no `can't reproduce` effect is requested and implementation does not start

#### Scenario: Feature or already reproduced defect enters
- **WHEN** accepted triage identifies a feature or a defect already reproduced for the applicable subject
- **THEN** delivery goes directly to size routing without another reproduction invocation

### Requirement: Triage size selects conditional OpenSpec authoring

The pipeline SHALL route `large` tickets through an isolated OpenSpec authoring invocation producing a coherent proposal, delta specifications, design and implementation tasks before test authoring. The retained requirements artifact set SHALL be the input to subsequent stages and part of the delivered candidate. `small` and `medium` tickets SHALL proceed to test authoring using an immutable Accepted Work Contract derived from the ticket's explicit expected behavior and accepted triage handoff, after any configured human requirements acceptance. Triage process-completion criteria SHALL NOT substitute for implementation acceptance. Missing expected behavior or unresolved material requirements SHALL pause the ticket rather than invent acceptance criteria. Human requirements acceptance SHALL follow the pinned component/size/priority policy for either branch. No additional independent AI specification-review invocation SHALL be required by this pipeline.

#### Scenario: Large feature or defect needs specifications
- **WHEN** size routing receives an accepted `large` ticket
- **THEN** OpenSpec artifacts are prepared and retained before tests or implementation use those exact requirements

#### Scenario: Non-large ticket has sufficient expected behavior
- **WHEN** size routing receives a `small` or `medium` ticket with explicit implementation acceptance
- **THEN** test authoring starts without an OpenSpec authoring invocation

#### Scenario: Triage succeeded without implementation acceptance
- **WHEN** the only acceptance document describes completion of triage and the ticket supplies no usable expected behavior
- **THEN** delivery pauses for requirements input and does not reuse that document to certify implementation

### Requirement: Verification tests are prepared in an isolated invocation before implementation

The pipeline SHALL create acceptance or regression tests from the frozen requirements and base revision in a separate agent invocation before implementation starts. Test source, assertions and author transcript SHALL be retained as an immutable protected bundle inaccessible to the implementation invocation. Implementation SHALL receive the requirements and a public test-scope summary. For behavior changes, test authoring SHALL demonstrate an expected behavioral failure on the base; an environment or harness failure SHALL NOT establish that result. Already satisfied acceptance SHALL produce a visible no-change outcome rather than fabricate a defect. Implementation SHALL still follow developer-visible Red/Green TDD for its own changes.

#### Scenario: Tests precede the fix
- **WHEN** a defect reaches test authoring
- **THEN** a protected regression test demonstrates the missing behavior on the base before the independent implementation invocation is dispatched

#### Scenario: Implementer attempts to inspect acceptance tests
- **WHEN** the implementation invocation requests protected test content or author transcript
- **THEN** its declared artifact and workspace access denies that read while requirements remain available

### Requirement: Operators can attach additional protected verification through Console

The test step SHALL accept additional verification inputs from uploaded files or a directory, or from selected paths in an additional Project repository. Console SHALL allow an operator to select Octafile tasks or shell entrypoints and set the Tester assignment separately. Preparation SHALL retain exact input bytes, repository revision when applicable, relative paths and executable selections together with the Tester-authored suite before the baseline run. Only explicitly selected entrypoints SHALL run, with fixtures available inside the isolated verification workspace.

#### Scenario: A supplementary test repository changes during implementation
- **WHEN** an operator has selected additional verification from a Project repository and that repository changes after test preparation
- **THEN** post-review verification executes the original retained files and configured entrypoints along with the Tester-authored tests

### Requirement: Implementation review and validation use the same candidate

The pipeline SHALL execute implementation or defect correction, capture its exact candidate, run one independent code-review invocation, and then validate that candidate with the previously retained protected bundle and required repository checks. The reviewer SHALL receive the candidate and requirements without the implementation transcript. A successful review SHALL NOT replace test execution. Required test failures or review findings SHALL prevent delivery and return bounded findings to implementation within a configured rework limit, defaulting to one rework cycle. A changed candidate SHALL receive a fresh review and validation; the original protected bundle SHALL remain fixed unless an explicit requirements or test correction starts a new preparation cycle. Exhaustion or indeterminate outcomes SHALL pause with findings. Protected assertions SHALL NOT leak through implementation remediation.

#### Scenario: Candidate passes review and acceptance tests
- **WHEN** the captured candidate receives a successful independent review and passes the prepared tests and required repository checks
- **THEN** that exact candidate becomes eligible for PR delivery

#### Scenario: Optional Tester analysis cannot skip mandatory checks
- **WHEN** the operator attaches a scoped Tester invocation to the post-review verification Build
- **THEN** Factory starts and retains the mandatory runner result before dispatching Tester, even if Tester never requests execution
- **AND** the downstream validation gate uses the runner's verified verdict for the exact candidate, regardless of a positive AI conclusion
- **AND** failed, missing, timed-out or skipped required checks cannot authorize delivery

#### Scenario: Review requests a fix or a prepared test fails
- **WHEN** review or validation identifies a change within the remaining rework budget
- **THEN** implementation receives bounded findings, and the replacement candidate repeats review and validation before delivery

#### Scenario: Rework is exhausted
- **WHEN** another correction is required after the configured rework limit
- **THEN** execution pauses with retained findings and no PR publication

### Requirement: Successful delivery ends with a pull request for manual merge

The pipeline SHALL select a DevOps AI-agent invocation for Delivery using the existing trusted PR publication capability. DevOps SHALL prepare the public PR metadata. The scoped trusted Delivery action SHALL perform forge writes, observe the exact result and retain the receipt. Confirmation SHALL NOT require another AI invocation. Only the candidate that passed review and validation SHALL be published. Successful publication SHALL end the pipeline with `delivered_for_review`, the PR reference and publication receipt. A human SHALL perform any later merge in the forge, including into production branches. This pipeline SHALL NOT invoke merge, enable forge auto-merge or add an in-pipeline approval that subsequently triggers merge. The existing publication-only Delivery protocol SHALL remain unchanged.

Recovery SHALL reuse the original publication identity and PR after an unknown response. Review or validation expiry SHALL prevent another forge write. An exact read-only recovery SHALL remain possible within the original attempt deadline and current claim. Publication failure or conflict SHALL remain visible without claiming successful delivery. A changed candidate SHALL require fresh review and validation before publication. AI invocations, including DevOps, SHALL NOT receive direct forge-write credentials. Reporting SHALL distinguish delivered-for-review from merged code and SHALL NOT claim merge or complete the ticket solely because a PR was published. Observing a later manual merge SHALL NOT be required to finish this pipeline.

#### Scenario: Successful pipeline publishes the accepted candidate
- **WHEN** the exact candidate passed review and validation and its configured publication is authorized
- **THEN** the trusted Delivery action publishes and observes one PR using the prepared metadata, and the pipeline ends with `delivered_for_review` and a link for manual merge

#### Scenario: Publication response is lost
- **WHEN** the provider created the PR but its response was lost
- **THEN** recovery within the original attempt observes and returns the original PR, even if the review has expired, without another publication identity or any merge operation

#### Scenario: PR targets production
- **WHEN** the trusted Delivery action publishes the accepted candidate to a PR targeting a production branch
- **THEN** the pipeline ends with delivery for review, merge remains a human forge operation and no pipeline merge approval is requested

#### Scenario: Publication is blocked
- **WHEN** existing Delivery target policy or provider authority prevents publication
- **THEN** the publication failure or conflict is visible and the ticket is not reported as delivered or merged

### Requirement: Ticket delivery is authorable through the ordinary Factory editor

An authorized operator SHALL be able to assemble ticket delivery from an empty visual Flow graph, publish an exact Flow version and select it in an ordinary Factory Configuration with pinned Project launch and access policies. Ordinary editor choices SHALL support the declared steps, branches, accepted inputs, requirements gate, Reporter actions and Delivery target. The versioned `ticket-delivery` preset SHALL be an optional starting point through the existing resolver and SHALL produce the same editable, validated definition. Existing readiness and validation SHALL identify unsupported or missing bindings before executable readiness is claimed. Automatic intake and external effects SHALL require explicit configuration. The successful path SHALL need no per-stage confirmation beyond configured requirements acceptance, reviewer quorum, model-scored acceptance or repeated general readiness stage.

#### Scenario: An operator builds the flow from an empty graph

- **WHEN** an authorized operator assembles ticket delivery from **Add pipeline** and publishes the Flow and Configuration
- **THEN** the operator can start an eligible Issue manually using the same runtime and validation as the optional preset while automatic intake remains disabled

#### Scenario: A required execution binding is unavailable

- **WHEN** a step needs protected-test execution or trusted publication that the selected installation cannot supply
- **THEN** the common review identifies the blocked step and capability before the pipeline is considered executable

### Requirement: Accepted delivery results can appear on the Issue

An operator SHALL be able to enable concise cards on accepted specification, review and publication outcomes through the existing Issue outcome presentation. Cards SHALL show a short conclusion, authorized typed links to retained specification or review resources and the exact original Run. Confirmed publication SHALL expose a descriptive PR link derived from the authorized retained Delivery receipt. Arbitrary model URLs, pending publication and failed delivery SHALL NOT produce a confirmed PR link. Protected assertions, private transcripts and internal identifiers SHALL remain outside the card. Run diagnostics SHALL show the execution path, safe input summaries, pending requirements acceptance, review and validation evidence and publication status. A technical stop or wait SHALL identify the affected step and retained reason without presenting an unverified outcome as accepted.

#### Scenario: Work has retained specifications and review before publication

- **WHEN** accepted specification and review outcomes are configured for display while publication is pending
- **THEN** the Issue shows the authorized specification and review links and no confirmed PR link

#### Scenario: The original PR is recovered after a lost response

- **WHEN** trusted Delivery records a confirmed receipt for the original operation
- **THEN** the enabled publication card links to that exact PR and states that it was published for manual merge

### Requirement: Every AI stage has its declared role and isolated invocation

The example graph SHALL assign the Tester responsibility to deeper defect reproduction and test authoring, Coder to implementation and independent review, DevOps to Delivery and Analytic to requirements/specification authoring. Factory SHALL dispatch the mandatory protected verification Build independently of AI tool calls. Operators MAY attach a separate Tester invocation for analysis and authorized diagnostics after the runner result is retained. Pass/fail SHALL rely on the trusted runner's deterministic reports in both configurations. Each agent stage SHALL pin a compatible Project Role/Profile and its own isolated invocation, even when stages share a role or profile. The assignment and expected result SHALL be set on the step independently of reusable Profile instructions and skills. Compatible existing roles SHALL be selectable regardless of their display names. Role names SHALL NOT dispatch runtime code or grant input access. The Coder review invocation SHALL remain independent of implementation and SHALL NOT inherit its private transcript.

#### Scenario: Stages select the configured AI agents
- **WHEN** reproduction, specification, test authoring, implementation, review, test execution and Delivery are dispatched
- **THEN** their retained invocations identify the exact selected stage Profiles and independent step assignments and contexts

#### Scenario: Tester describes a failed run as successful
- **WHEN** the Tester execution summary claims success but a required prepared test failed or did not execute
- **THEN** validation remains unsuccessful and no delivery acceptance is granted

### Requirement: Stage data visibility is independently configurable

Each stage SHALL declare its own code resource and scope, Work task selection, selected accepted predecessor fields, exact Project documents or extra Artifacts, and tool-access scope. The system SHALL retain step state and accepted results in the existing Factory owners. Transitions SHALL convey control independently of data projection. The pipeline's available repositories SHALL convey no checkout without a step binding. Source visibility SHALL support no source, selected revision-bound fragments or an explicitly allowed workspace according to stage needs. The system SHALL retain the resolved input manifest per invocation and enforce it at artifact, filesystem and tool boundaries. A control edge, shared role or access granted to another stage SHALL NOT imply inherited data access. The implementation and code-review invocations SHALL NOT receive protected acceptance tests or their author's transcript. Validation SHALL distinguish the protected inputs mounted by its trusted test runner from data exposed to the Tester reasoning invocation; black-box Tester execution SHALL operate without source when configured. Missing required selected data SHALL stop its consumer without broadening access.

#### Scenario: Same-role stages receive different inputs
- **WHEN** Tester reproduction has selected source fragments and Tester validation is configured with only an endpoint, plan and reports
- **THEN** each receives only its own input selections and validation cannot read source through undeclared files or tools

#### Scenario: Configuration file is selected for one stage
- **WHEN** an operator selects an extra immutable configuration file for Analytic but not for Coder review
- **THEN** only the Analytic invocation receives it unless review independently selects the file

### Requirement: Stage permissions are isolated and enforced at execution

Invocation and permission isolation SHALL be a required deliverable of this pipeline. Each stage SHALL execute with its own enforced workspace/Artifact access, read/write paths, tools/commands, child-process, network and credential grants. Stages sharing a role SHALL NOT inherit each other's private workspace, session, environment or secret access. Omitting data from a prompt or Context Manifest SHALL NOT substitute for enforcement at the actual access boundary. The system SHALL use and, where needed, extend existing runtime enforcement; a backend unable to enforce a required restriction SHALL block dispatch without falling back to broader permissions.

The independent reviewer SHALL have read-only access to the selected candidate; allowed report-output paths SHALL remain separate. Test execution SHALL use isolated temporary/output paths without mutating the retained candidate or protected test bundle. Only implementation SHALL write its allowed implementation paths. Requirements and test authors SHALL write their selected owned deliverables. DevOps SHALL supply metadata to a separately scoped trusted publication action without forge-write credentials or merge authority.

#### Scenario: Review attempts to change code
- **WHEN** the Coder review invocation attempts to write a selected candidate file
- **THEN** execution denies the write while still permitting its declared review-report output

#### Scenario: Invocation attempts to reuse another stage's authority
- **WHEN** an invocation requests another stage's private workspace, protected Artifact, credential or undeclared network/tool access
- **THEN** the actual execution boundary denies that access even if both invocations share a role

#### Scenario: Required isolation cannot be enforced
- **WHEN** the selected execution backend cannot enforce a required stage restriction
- **THEN** the stage is not dispatched with wider permissions and the missing isolation capability is reported

### Requirement: Human acceptance follows requirements policy

The pipeline SHALL support pinned requirements-acceptance rules combining component, size and priority from the accepted triage result. Matching rules SHALL request human acceptance of the exact prepared specification or Accepted Work Contract before test authoring; unmatched tickets SHALL continue automatically. Priority SHALL use the retained triage priority assessment without a second agent assessment. A pending or rejected acceptance SHALL NOT permit downstream stages. A correction SHALL follow the declared requirements route and require acceptance of the new version when the rule applies.

Acceptance SHALL bind the authorized actor, exact requirements or contract digest, policy and current gate attempt. Changed requirements SHALL NOT reuse a previous approval for the changed version. Existing human-gate authorization, durable responses and recovery SHALL own these decisions; AI agents SHALL NOT approve themselves. Pending and decided gates SHALL be visible through Console and headless diagnostics. This requirements gate SHALL NOT grant merge authority.

#### Scenario: Ticket matches human requirements acceptance
- **WHEN** its retained component, size and priority match a configured rule and Analytic has prepared the specification
- **THEN** execution waits for human acceptance of that exact result before creating tests, and rejection does not start implementation

#### Scenario: Ticket does not match a requirements rule
- **WHEN** its frozen requirements are ready and no configured human-acceptance rule matches
- **THEN** the Tester authoring stage starts without a human acceptance request

#### Scenario: Requirements change after approval
- **WHEN** requirements change after human approval
- **THEN** the prior approval cannot authorize that changed version and the applicable gate requests a fresh decision
