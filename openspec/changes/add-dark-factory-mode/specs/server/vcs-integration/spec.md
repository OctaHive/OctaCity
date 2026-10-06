## ADDED Requirements

### Requirement: Separate write-capable delivery protocol
Write-capable Git publication and forge review operations SHALL use a bounded versioned Delivery adapter protocol separate from read-only VCS and webhook protocols. A Delivery adapter SHALL declare supported publication and review operations, identity, protocol range, executable digest, credential class, idempotency behavior, and observation capabilities before selection. Read-only VCS callers and repository-controlled Jobs SHALL NOT obtain a Delivery adapter credential or invoke its write operations.

#### Scenario: Read-only VCS adapter is selected
- **WHEN** the server resolves or browses source for a Build or Factory Run
- **THEN** it uses only read capabilities and cannot publish a branch, create a review, or merge a candidate

#### Scenario: Delivery adapter protocol is incompatible
- **WHEN** an installed adapter cannot satisfy the required version, operation, identity, or idempotency contract
- **THEN** delivery admission fails before exposing a forge credential or publishing repository state

### Requirement: Exact accepted candidate delivery
The Delivery protocol SHALL accept a stable delivery identity, exact repository, base and accepted candidate identities, ChangeSet and Decision digests, bounded target policy, and permitted review metadata. Before mutation, the adapter SHALL verify or observe repository ancestry and existing external state. It SHALL publish only the accepted candidate, create or update at most one review target for the stable identity, and return provider-neutral external references and typed outcomes.

#### Scenario: Pull request is created
- **WHEN** a human-approved delivery command references the current accepted Decision and no matching external review exists
- **THEN** the adapter publishes the exact candidate and creates one review target linked to the stable delivery identity

#### Scenario: Delivery request is replayed after an unknown response
- **WHEN** the same stable delivery request is retried after the provider may have accepted it
- **THEN** the adapter observes existing external state and returns or repairs the same logical delivery instead of creating another branch or review target

#### Scenario: Published candidate differs from accepted candidate
- **WHEN** the remote branch or proposed review head does not equal the exact candidate bound to the accepted Decision
- **THEN** the adapter reports a conflict and does not claim successful delivery

### Requirement: Human merge remains external in the initial release
The initial Delivery protocol and policy SHALL NOT authorize automatic merge. It MAY publish a candidate and create or update a review target, but the Factory Run SHALL distinguish `delivered_for_review` from externally observed merge state and SHALL NOT report code as merged solely because an LLM, evaluator, or delivery adapter recommended acceptance.

#### Scenario: Accepted candidate reaches review
- **WHEN** delivery successfully creates or updates the review target
- **THEN** the Factory Run records delivery for human review without invoking a merge operation
