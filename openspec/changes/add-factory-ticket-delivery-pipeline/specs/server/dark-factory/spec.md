## MODIFIED Requirements

### Requirement: Requirements cycles preserve exact candidate lineage
A requirements flow SHALL support a deterministic specification-bypass branch under immutable configured size bounds and a requirements-authoring branch outside those bounds. The pinned Flow Definition SHALL explicitly select whether independent requirements review is required; omitting that invocation SHALL NOT omit the declared downstream code-review and validation gates. Before bypassing specification authoring, the server SHALL freeze an Accepted Work Contract containing the immutable Work Envelope, accepted Eligibility and Triage Results, acceptance criteria, exact base, required Research Handoffs, policy digest, and provenance. The Accepted Work Contract SHALL feed the same declared protected test-authoring, implementation, review, and verification gates as an accepted specification unless the pinned Flow Definition explicitly omits one.

For larger feature Work, requirements authoring SHALL create a new specification or change with acceptance criteria and declared design, performance, security, UI, compatibility, and operational requirements as applicable. For a defect, requirements authoring SHALL identify or amend the violated specification and SHALL record regression acceptance criteria. Accepted requirements SHALL be captured as an exact immutable ChangeSet, linked to the admitted Work and original base, and SHALL be the only specification candidate that can make the applicable Flow Run ready for implementation.

An implementation, review, or verification node MAY return a typed Requirements Defect referencing exact contradictory, incomplete, or infeasible requirements evidence. Deterministic policy SHALL start a new append-only Workflow Cycle at the declared requirements node, retain all prior attempts and candidates, and mark descendants of the superseded requirements candidate ineligible to authorize later delivery or deployment. A new cycle SHALL traverse its declared review and acceptance gates again.

#### Scenario: Requirements author and reviewer are isolated
- **WHEN** the independent requirements-review node is dispatched
- **THEN** its Context Manifest contains the candidate specification and declared acceptance evidence but excludes the author's hidden transcript and undeclared context

#### Scenario: Small Work bypasses specification authoring but not test-first gates
- **WHEN** deterministic policy accepts Work within the configured specification-bypass bounds
- **THEN** the server freezes one Accepted Work Contract and routes it to the declared protected test-authoring or later gates without treating model prose as requirements

#### Scenario: Requirements policy changes after admission
- **WHEN** an operator replaces the gate policy or its bypass parameters after a Work admission
- **THEN** the admitted run evaluates and restores its contract using the frozen binding and cannot substitute the replacement

#### Scenario: Rejected Work reaches the requirements policy
- **WHEN** requirements acceptance is requested with an accepted terminal or verification-only triage decision
- **THEN** the policy rejects the request without creating an Accepted Work Contract or requirements-authoring request

#### Scenario: Defect requirements preserve regression intent
- **WHEN** a defect requires a specification correction
- **THEN** the accepted requirements ChangeSet identifies the violated contract and regression acceptance criteria before implementation becomes ready

#### Scenario: Implementation finds a specification defect
- **WHEN** implementation returns a valid Requirements Defect for the exact active specification candidate
- **THEN** the server appends a new requirements cycle and no downstream result from the superseded candidate may satisfy a later gate

#### Scenario: Ticket delivery selects its compact requirements path
- **WHEN** a pinned ticket-delivery Flow bypasses specifications for `small` and `medium` and authors OpenSpec for `large` without a separate requirements reviewer
- **THEN** each branch freezes its exact contract and completes any declared human requirements acceptance before test authoring, and implementation still requires the declared independent code review and prepared-test validation
