## Purpose

Defines the supported post-triage ticket pipeline from conditional reproduction and OpenSpec authoring through isolated test preparation, implementation, independent review, validation and pull request publication for manual merge.

## ADDED Requirements

### Requirement: Development consumes an accepted triage handoff

The system SHALL supply a versioned `ticket-delivery` preset using ordinary Factory execution. A separately enabled downstream intake or explicit Issue execution request SHALL bind the original ticket, exact accepted triage result, kind, size, reproduction outcome and repository subject to a distinct downstream Work and Run. Repeated intake SHALL reuse the same logical execution under existing admission rules. Labels alone SHALL NOT establish that triage succeeded. Rejected, review-pending or non-development resolutions SHALL NOT enter this preset.

#### Scenario: Accepted ticket enters delivery once
- **WHEN** an enabled downstream intake observes an accepted development-eligible triage result more than once
- **THEN** one downstream execution retains that predecessor and starts without rerunning triage or sizing

#### Scenario: Label is added without an accepted result
- **WHEN** a ticket has a delivery selection label but lacks an applicable accepted triage result
- **THEN** no implementation is admitted and the missing handoff is visible

### Requirement: Unresolved defects receive one deeper reproduction stage

The triage stage SHALL request the existing label `need reproduce` through its configured reporting policy when classification accepts a defect with `not_reproduced`, `unknown` or `needs_research` preliminary reproducibility. This triage effect SHALL be independent of downstream delivery activation or admission. The delivery pipeline SHALL NOT add `need reproduce`. Defects requiring missing input SHALL remain paused until input is supplied. Delivery SHALL use the retained reproduction outcome to run one deeper reproduction stage before size routing for unresolved defects; reproduced defects and features SHALL bypass it. This stage SHALL retain the reproduction checks and observed outcome against the admitted subject. A completed `cannot_reproduce` outcome SHALL end delivery, add `can't reproduce`, remove the managed `need reproduce` label and leave the ticket open. A reproduced outcome SHALL remove the managed `need reproduce` label and continue. Reporter delivery failure SHALL remain visible separately from the accepted outcome and SHALL preserve unrelated labels.

Timeouts, missing dependencies, unavailable environments and insufficient information SHALL pause or technically fail the affected stage without asserting `cannot_reproduce`. The preset SHALL NOT repeatedly re-admit the same unresolved ticket after termination.

#### Scenario: Triage marks an unresolved defect while delivery is disabled
- **WHEN** triage accepts an unresolved defect classification and the downstream delivery pipeline is disabled
- **THEN** triage requests the `need reproduce` label through its Reporter policy without admitting a delivery execution

#### Scenario: Defect is still not reproducible
- **WHEN** a defect marked `need reproduce` completes deeper reproduction checks without observing the reported symptom
- **THEN** its retained outcome is `cannot_reproduce`, the configured labels become `can't reproduce` without `need reproduce`, the ticket stays open and no tests, implementation or PR are started

#### Scenario: Deeper reproduction succeeds
- **WHEN** the deeper reproduction stage observes the defect
- **THEN** delivery continues to size routing with its reproduction evidence and removes the managed `need reproduce` label

#### Scenario: Reproduction environment is unavailable
- **WHEN** the reproduction stage cannot prepare its required environment
- **THEN** the failure remains unresolved, no `can't reproduce` effect is requested and implementation does not start

#### Scenario: Feature or already reproduced defect enters
- **WHEN** accepted triage identifies a feature or a defect already reproduced for the applicable subject
- **THEN** delivery goes directly to size routing without another reproduction invocation

### Requirement: Triage size selects conditional OpenSpec authoring

The preset SHALL route `large` tickets through an isolated OpenSpec authoring invocation producing a coherent proposal, delta specifications, design and implementation tasks before test authoring. The retained requirements artifact set SHALL be the input to subsequent stages and part of the delivered candidate. `small` and `medium` tickets SHALL proceed to test authoring using an immutable Accepted Work Contract derived from the ticket's explicit expected behavior and accepted triage handoff, after any configured human requirements acceptance. Triage process-completion criteria SHALL NOT substitute for implementation acceptance. Missing expected behavior or unresolved material requirements SHALL pause the ticket rather than invent acceptance criteria. Human requirements acceptance SHALL follow the pinned component/size/priority policy for either branch. No additional independent AI specification-review invocation SHALL be required by this preset.

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

The preset SHALL create acceptance or regression tests from the frozen requirements and base revision in a separate agent invocation before implementation starts. Test source, assertions and author transcript SHALL be retained as an immutable protected bundle inaccessible to the implementation invocation. Implementation SHALL receive the requirements and a public test-scope summary. For behavior changes, test authoring SHALL demonstrate an expected behavioral failure on the base; an environment or harness failure SHALL NOT establish that result. Already satisfied acceptance SHALL produce a visible no-change outcome rather than fabricate a defect. Implementation SHALL still follow developer-visible Red/Green TDD for its own changes.

#### Scenario: Tests precede the fix
- **WHEN** a defect reaches test authoring
- **THEN** a protected regression test demonstrates the missing behavior on the base before the independent implementation invocation is dispatched

#### Scenario: Implementer attempts to inspect acceptance tests
- **WHEN** the implementation invocation requests protected test content or author transcript
- **THEN** its declared artifact and workspace access denies that read while requirements remain available

### Requirement: Implementation review and validation use the same candidate

The preset SHALL execute implementation or defect correction, capture its exact candidate, run one independent code-review invocation, and then validate that candidate with the previously retained protected bundle and required repository checks. The reviewer SHALL receive the candidate and requirements without the implementation transcript. A successful review SHALL NOT replace test execution. Required test failures or review findings SHALL prevent delivery and return bounded findings to implementation within a configured rework limit, defaulting to one rework cycle. A changed candidate SHALL receive a fresh review and validation; the original protected bundle SHALL remain fixed unless an explicit requirements or test correction starts a new preparation cycle. Exhaustion or indeterminate outcomes SHALL pause with findings. Protected assertions SHALL NOT leak through implementation remediation.

#### Scenario: Candidate passes review and acceptance tests
- **WHEN** the captured candidate receives a successful independent review and passes the prepared tests and required repository checks
- **THEN** that exact candidate becomes eligible for PR delivery

#### Scenario: Review requests a fix or a prepared test fails
- **WHEN** review or validation identifies a change within the remaining rework budget
- **THEN** implementation receives bounded findings, and the replacement candidate repeats review and validation before delivery

#### Scenario: Rework is exhausted
- **WHEN** another correction is required after the configured rework limit
- **THEN** execution pauses with retained findings and no PR publication

### Requirement: Successful delivery ends with a pull request for manual merge

The preset SHALL select a DevOps AI-agent invocation for Delivery using the existing trusted PR publication capability. DevOps SHALL prepare and observe publication through scoped tools; only trusted actions SHALL perform forge writes. Only the candidate that passed review and validation SHALL be published. Successful publication SHALL end the pipeline with `delivered_for_review`, the PR reference and publication receipt. A human SHALL perform any later merge in the forge, including into production branches. This preset SHALL NOT invoke merge, enable forge auto-merge or add an in-pipeline approval that subsequently triggers merge. The existing publication-only Delivery protocol SHALL remain unchanged.

Recovery SHALL reuse the original publication identity and PR after an unknown response. Publication failure or conflict SHALL remain visible without claiming successful delivery. A changed candidate SHALL require fresh review and validation before publication. AI invocations, including DevOps, SHALL NOT receive direct forge-write credentials. Reporting SHALL distinguish delivered-for-review from merged code and SHALL NOT claim merge or complete the ticket solely because a PR was published. Observing a later manual merge SHALL NOT be required to finish this pipeline.

#### Scenario: Successful pipeline publishes the accepted candidate
- **WHEN** the exact candidate passed review and validation and its configured publication is authorized
- **THEN** DevOps uses trusted Delivery to publish one PR and the pipeline ends with `delivered_for_review` and a link for manual merge

#### Scenario: Publication response is lost
- **WHEN** the provider created the PR but its response was lost
- **THEN** recovery observes and returns the original PR without another publication identity or any merge operation

#### Scenario: PR targets production
- **WHEN** DevOps publishes the accepted candidate to a PR targeting a production branch
- **THEN** the pipeline ends with delivery for review, merge remains a human forge operation and no pipeline merge approval is requested

#### Scenario: Publication is blocked
- **WHEN** existing Delivery target policy or provider authority prevents publication
- **THEN** the publication failure or conflict is visible and the ticket is not reported as delivered or merged

### Requirement: Setup and progress remain a compact ordinary preset journey

The preset SHALL be discoverable and configurable through existing preset APIs and Console setup, with per-stage roles/profiles and input selections, repository, upstream triage selection, existing label mappings, rework budget, requirements-acceptance rules, PR target and publication policy. Setup SHALL report required missing capabilities together. Operator confirmation inside this pipeline SHALL be limited to configured requirements acceptance; the successful path SHALL require no other per-stage confirmation, reviewer quorum, model-scored acceptance or repeated general readiness stage. Existing runtime input, isolation and permission enforcement SHALL remain applicable. Run diagnostics SHALL show skipped conditional stages, selected roles/profiles, safe input summaries, current execution, reproduction result, OpenSpec artifacts, approval requests/decisions, test bundle identity, review/validation summaries, PR and publication outcome without exposing protected test content.

#### Scenario: Operator follows the ordinary successful journey
- **WHEN** the operator enables a configured ticket-delivery preset and an eligible ticket enters
- **THEN** its applicable stages proceed automatically until an applicable human gate, and the Console and headless diagnostics show the same retained outcome and pending acceptance requests

### Requirement: Every AI stage has its declared role and isolated invocation

The preset SHALL select Project-owned AI-agent roles named `Tester` for deeper defect reproduction, test authoring and test execution/validation; `Coder` for implementation/fix and independent code review; `DevOps` for Delivery; and `Analytic` for requirements/specification authoring. Each stage SHALL pin its own compatible Role Profile and isolated invocation, even when stages share a role or profile. Role names SHALL NOT dispatch runtime code or grant input access. The Coder review invocation SHALL remain independent of implementation and SHALL NOT inherit its private transcript. Tester test execution SHALL use scoped tools to execute/observe the prepared test plan through the trusted runner; actual deterministic execution reports SHALL determine pass/fail, not model prose.

#### Scenario: Stages select the configured AI agents
- **WHEN** reproduction, specification, test authoring, implementation, review, test execution and Delivery are dispatched
- **THEN** their retained invocations identify respectively Tester, Analytic, Tester, Coder, Coder, Tester and DevOps with the exact stage Profiles and separate contexts

#### Scenario: Tester describes a failed run as successful
- **WHEN** the Tester execution summary claims success but a required prepared test failed or did not execute
- **THEN** validation remains unsuccessful and no delivery acceptance is granted

### Requirement: Stage data visibility is independently configurable

Each stage SHALL declare its own repository visibility, selected prior-stage results, requirements, extra files/configuration and tool-access scope. Source visibility SHALL support no source, selected revision-bound fragments or an explicitly allowed workspace according to stage needs. The system SHALL retain the resolved input manifest per invocation and enforce it at artifact, filesystem and tool boundaries. A control edge, shared role or access granted to another stage SHALL NOT imply inherited data access. The implementation and code-review invocations SHALL NOT receive protected acceptance tests or their author's transcript. Validation SHALL distinguish the protected inputs mounted by its trusted test runner from data exposed to the Tester reasoning invocation; black-box Tester execution SHALL operate without source when configured. Missing required selected data SHALL stop its consumer without broadening access.

#### Scenario: Same-role stages receive different inputs
- **WHEN** Tester reproduction has selected source fragments and Tester validation is configured with only an endpoint, plan and reports
- **THEN** each receives only its own input selections and validation cannot read source through undeclared files or tools

#### Scenario: Configuration file is selected for one stage
- **WHEN** an operator selects an extra immutable configuration file for Analytic but not for Coder review
- **THEN** only the Analytic invocation receives it unless review independently selects the file

### Requirement: Stage permissions are isolated and enforced at execution

Invocation and permission isolation SHALL be a required deliverable of this preset. Each stage SHALL execute with its own enforced workspace/Artifact access, read/write paths, tools/commands, child-process, network and credential grants. Stages sharing a role SHALL NOT inherit each other's private workspace, session, environment or secret access. Omitting data from a prompt or Context Manifest SHALL NOT substitute for enforcement at the actual access boundary. The system SHALL use and, where needed, extend existing runtime enforcement; a backend unable to enforce a required restriction SHALL block dispatch without falling back to broader permissions.

The independent reviewer SHALL have read-only access to the selected candidate; allowed report-output paths SHALL remain separate. Test execution SHALL use isolated temporary/output paths without mutating the retained candidate or protected test bundle. Only implementation SHALL write its allowed implementation paths. Requirements and test authors SHALL write their selected owned deliverables. DevOps SHALL use scoped trusted publication tools without direct forge-write credentials or merge authority.

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

The preset SHALL support pinned requirements-acceptance rules combining component, size and priority from the accepted triage result. Matching rules SHALL request human acceptance of the exact prepared specification or Accepted Work Contract before test authoring; unmatched tickets SHALL continue automatically. Priority SHALL use the retained triage priority assessment without a second agent assessment. A pending or rejected acceptance SHALL NOT permit downstream stages. A correction SHALL follow the declared requirements route and require acceptance of the new version when the rule applies.

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
