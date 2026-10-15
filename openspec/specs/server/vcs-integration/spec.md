# VCS Integration Specification

## Purpose

Defines provider-neutral webhook ingestion and VCS access without coupling build triggers to GitHub, Gerrit, Git, or another provider and without executing repository-controlled code in the server.

## Requirements

### Requirement: Separate versioned provider protocols
The server SHALL expose separate bounded versioned adapter protocols for webhook providers and VCS implementations. Each adapter SHALL declare its identity, protocol range, supported operations, event or repository capabilities, and immutable executable digest before use.

#### Scenario: Adapter supports only VCS browsing
- **WHEN** a Git adapter declares revision, history, tree, and content capabilities but no webhook capability
- **THEN** the server uses it only for declared VCS operations and does not route webhook deliveries to it

#### Scenario: Protocol range is incompatible
- **WHEN** an installed adapter has no protocol version in common with the server
- **THEN** the server rejects it before sending credentials, repository data, or webhook bodies

### Requirement: Provider-neutral VCS access
The VCS protocol SHALL support bounded reference listing, commit selection and metadata, tree and file-content reads, and resolution of an allowed mutable reference to an immutable revision. Results SHALL use provider-neutral envelopes with bounded implementation-specific metadata.

#### Scenario: Branch is selected
- **WHEN** a client selects an allowed branch for a build
- **THEN** the Git implementation resolves it once and the accepted build records the exact immutable commit

#### Scenario: Repository content is browsed
- **WHEN** a client requests a tree or file at an immutable revision within configured limits
- **THEN** the server returns bounded content metadata without evaluating, loading, or executing repository code

### Requirement: Manual and managed webhook configuration
The server SHALL support unmanaged webhook configuration, in which an operator creates the remote webhook manually, and managed configuration, in which a provider adapter creates, observes, rotates, and removes the remote registration. Both modes SHALL produce the same server-owned webhook integration identity.

#### Scenario: Operator configures a webhook manually
- **WHEN** an operator requests an unmanaged webhook integration
- **THEN** the server returns the callback and verification requirements needed to configure the remote provider without requiring provider administration credentials

#### Scenario: Managed registration is unavailable
- **WHEN** a provider does not support managed webhook registration
- **THEN** unmanaged configuration remains available if the provider can authenticate and normalize deliveries

### Requirement: Authenticate before normalization
The webhook-provider protocol SHALL receive a bounded raw body, an allowlisted bounded header set, the selected integration identity, and protected verification material. The provider adapter SHALL authenticate the exact delivery before returning a normalized repository event to the application layer.

#### Scenario: Signature is invalid
- **WHEN** a webhook delivery fails provider authentication
- **THEN** the server rejects it without publishing a normalized event, resolving a revision, or creating a build

#### Scenario: Different providers report an equivalent push
- **WHEN** authenticated GitHub and Gerrit adapters normalize equivalent repository changes
- **THEN** the trigger engine receives the same provider-neutral event kind, repository identity, reference and revision fields

### Requirement: Idempotent event ingestion
Every authenticated repository event SHALL include a provider delivery identity or a deterministic adapter-generated equivalent scoped to the configured integration. The server SHALL durably deduplicate that identity before evaluating triggers.

#### Scenario: Provider retries delivery
- **WHEN** the same authenticated webhook delivery is received more than once
- **THEN** the server records repeated receipt but creates at most one trigger occurrence and build

### Requirement: Adapter failures are classified
Webhook and VCS adapters SHALL distinguish invalid requests, unsupported operations, permanent repository or configuration failures, transient provider failures, cancellation, and protocol faults. The server SHALL apply bounded retry only to transient idempotent operations.

#### Scenario: Provider is temporarily unavailable
- **WHEN** revision resolution or managed webhook registration receives a retryable provider failure
- **THEN** the server retains the work durably and retries within configured limits without duplicating registrations or builds

### Requirement: Server does not execute repository code
VCS access and webhook handling SHALL NOT evaluate an Octafile, invoke repository scripts, load repository plugins into the server process, or materialize a working tree for execution.

#### Scenario: Malicious repository metadata is received
- **WHEN** repository-controlled names, file contents, commit messages, or webhook fields contain executable-looking content
- **THEN** the server treats them as bounded data and executes nothing

### Requirement: Separate write-capable delivery protocol
Write-capable Git publication and forge review operations SHALL use a bounded versioned Delivery adapter protocol separate from read-only VCS and webhook protocols. A Delivery adapter SHALL declare supported publication and review operations, identity, protocol range, executable digest, credential class, idempotency behavior, and observation capabilities before selection. Read-only VCS callers and repository-controlled Jobs SHALL NOT obtain a Delivery adapter credential or invoke its write operations.

Provider integration SHALL be organized as one plugin per provider rather than one plugin per lifecycle stage. GitHub Delivery and future GitHub Work Source, repository-read and Work Reporter capabilities SHALL use the same `octacity-github` plugin and shared provider transport, installation and operator configuration. Sharing a plugin SHALL NOT merge protocol contracts, grant write credentials to a read/reporting caller, or make provider state authoritative for a Factory transition. Concrete external intake and reporter adapters remain independently scoped follow-up work.

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
