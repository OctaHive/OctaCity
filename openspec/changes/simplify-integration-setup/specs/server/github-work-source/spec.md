## MODIFIED Requirements

### Requirement: Capabilities and credentials remain independently scoped
The installed GitHub provider SHALL advertise Source and Reporter separately from Delivery, enforce repository and network ceilings before accessing protected material, and reject credential-class substitution. Authorized setup discovery SHALL resolve only addresses permitted by the installed enrollment policy before publishing an exact resource ceiling for ordinary use. Credentials SHALL remain in protected server-side storage resolved through scoped private handles. Existing operator-installed private files SHALL remain supported. Management SHALL accept raw credentials only as transient input to the authorized write-only integration credential operation or protected combined setup command. One submitted token SHALL provision only the selected supported purposes through independently scoped bindings and SHALL NOT implicitly grant repository reads or Delivery. Raw credentials SHALL NOT appear in Work inputs, provider responses, ordinary REST representations, persisted browser state, logs, audit or diagnostics. Pipeline editors SHALL select authorized connection/resource references without receiving or entering tokens. A Source or Reporter outage SHALL remain scoped to its Factory inputs/effects and SHALL NOT disable ordinary Builds or require those capabilities for manual admission.

#### Scenario: Delivery credential is presented as an Issue source credential
- **WHEN** a caller tries to use Delivery authority to read tickets or update labels without the separately permitted capability
- **THEN** the operation is rejected before a provider effect and no credential is exposed

#### Scenario: GitHub source is not installed
- **WHEN** a deployment runs ordinary Builds or manually admitted Factory Work without a GitHub input
- **THEN** those workflows remain available without GitHub credentials or polling

#### Scenario: Setup selects task reading only
- **WHEN** an administrator submits a token with broader remote permissions but selects only task reading
- **THEN** the connection provisions task-read authority and cannot borrow task-update, repository-read or Delivery authority

### Requirement: Managed GitHub connections preserve installed scope and identity
GitHub connection administration SHALL configure a named connection using the installed adapter, permitted API endpoint, token authentication and explicitly selected native repository identities with owner/name display context. On enrollment-enabled installations, protected setup SHALL accept a repository project address and the submitted token without requiring that repository to be preinstalled. It SHALL derive requests only through the approved GitHub endpoint and installed address/enrollment policy, verify exact native repository identity and canonical context, and publish immutable resource/profile references usable after restart without per-repository installation or image rebuild. Public and private repository setup SHALL use the submitted credential without relying on unauthenticated repository discovery, ambient tokens or CLI sessions. It SHALL provision or adopt independently scoped private credentials for enabled purposes and expose safe configuration/access readiness without returning token values. Repository enumeration, setup resolution and connection checks SHALL remain bounded, respect the trusted network ceiling and verify native repository identity before ordinary protected use. A requested resource outside enrollment policy SHALL remain rejected even if its token can access it. Unsupported authentication methods or capabilities SHALL NOT be advertised as usable. Native GitLab/Jira implementations SHALL NOT be implied by common connection metadata alone.

#### Scenario: Administrator provisions an Issue-read connection
- **WHEN** an authorized administrator submits a token for an allowed GitHub repository and enables only task reading
- **THEN** the connection can be granted to an OctaCity Project for Issue discovery without granting Reporter or Delivery authority

#### Scenario: Existing installed configuration is adopted
- **WHEN** an administrator explicitly adopts a private installed GitHub configuration and scoped handles
- **THEN** the managed connection retains its installation/native resource identities, requires no token read-back, and does not enable inputs or duplicate existing Work

#### Scenario: Token rotates while reporting is pending
- **WHEN** a credential is rotated behind the same permitted purpose handle
- **THEN** pending reporting retains its original operation, native item, profile and mapping and uses the current permitted credential without resetting deadlines or creating Work

#### Scenario: Connection or write access is revoked
- **WHEN** a connection is disabled or its Project's task-update grant is revoked before the next dispatch
- **THEN** that dispatch performs no new native mutation or credential fallback, while accepted Factory history remains retained under the original bounded reporting policy

#### Scenario: Private repository differs from the Factory demo repository
- **WHEN** an authorized administrator submits a non-demo private repository URL and token under a compatible enrollment policy
- **THEN** setup verifies it through authenticated bounded repository metadata lookup, exposes its human address and retains its native repository identity without adding a bootstrap flag or public lookup prerequisite

#### Scenario: Submitted URL attempts credential exfiltration
- **WHEN** a repository URL contains an unapproved host, embedded credentials or an unsupported path, or resolution redirects to an unapproved destination
- **THEN** the adapter rejects it without transmitting the token to that destination or registering its resource

#### Scenario: Server restarts after new repository setup
- **WHEN** an enrolled connection is granted to a Project and the server restarts
- **THEN** its exact resource and scoped credentials remain usable by authorized Source, Reporter and read-only checks without rewriting the installed bundle

#### Scenario: Connection address is changed while reporting is pending
- **WHEN** an administrator selects a different native repository in a replacement connection version
- **THEN** pending reporting retains its original repository identity and authorization checks and cannot be redirected to the replacement repository

#### Scenario: Connection is archived before reporting dispatch
- **WHEN** a managed GitHub connection is archived before the next Source or Reporter dispatch
- **THEN** no new native request or credential fallback is made through it, and retained Work/reporting keeps its original repository/item identities and results

#### Scenario: Unused connection is permanently deleted
- **WHEN** an authorized administrator deletes an eligible unused GitHub connection
- **THEN** only its exclusively owned unreferenced private bindings are cleaned up, with no repository/Issue mutation or remote token revocation, and another connection to the same repository keeps its independent credentials and grants
