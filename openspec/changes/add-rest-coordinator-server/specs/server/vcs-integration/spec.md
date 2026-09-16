## Purpose

Defines provider-neutral webhook ingestion and VCS access without coupling build triggers to GitHub, Gerrit, Git, or another provider and without executing repository-controlled code in the server.

## ADDED Requirements

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
