## MODIFIED Requirements

### Requirement: Integrations has dedicated reusable connection administration
The console SHALL provide a dedicated provider-neutral Integrations section for authorized operators to list, create, inspect, replace, check and disable named connections, manage credentials and inspect permitted dependents. Its primary creation flow SHALL be one form containing internal display name, external project address, transient password-style token input and supported task-read/task-modify permission choices, followed by one explicit **Check and save** submission. It SHALL use server-advertised installed setup capabilities and authoritative validation to select a compatible provider, requesting a human-readable provider choice only when needed to disambiguate. The operator SHALL NOT need to enter opaque installation, native resource, profile or credential identities, choose technical authentication handles, visit another credential screen or activate a successfully saved connection separately. A safe summary of submitted choices SHALL be visible within the same form. Only installed supported capabilities SHALL be offered, and unsupported addresses SHALL receive actionable safe guidance. GitHub SHALL expose canonical repository owner/name context and token setup without making GitHub-specific fields mandatory for other providers.

Connection details SHALL show name, canonical external address, declared permissions, enabled state and separate capability/check readiness, with accessible Edit, Change token and Disable actions. Existing advanced administration/adoption SHALL remain supported without being prerequisites for normal setup. The section SHALL distinguish connection configuration from access granted to an OctaCity Project and from pipeline filtering/routing. Forms SHALL retain immutable intent, version-conflict and safe error handling, keyboard access and English/Russian messages; disruptive changes SHALL retain explicit consequence confirmation. Setup SHALL NOT create Project grants, import tasks, enable pipeline inputs or start execution.

#### Scenario: Operator creates one connection for multiple pipelines
- **WHEN** an administrator configures a named GitHub connection and grants selected resources to a Project
- **THEN** several pipeline editors can select that connection without copying its credentials or its per-pipeline predicates

#### Scenario: Provider is not installed
- **WHEN** a provider has no compatible installed adapter
- **THEN** the console identifies it as unavailable and cannot present metadata alone as a working source

#### Scenario: Operator connects an external project in one form
- **WHEN** the operator opens Add integration, enters a name, supported project URL and token, selects task permissions and submits Check and save
- **THEN** successful setup opens a usable connection detail without a separate profile-selection, credential-management or activation screen
- **AND** Project access is an explicit separate action and remains ungranted

#### Scenario: Setup cannot confirm write permission safely
- **WHEN** reading succeeds but the server reports writing as unverified
- **THEN** the same form/detail explains unverified write readiness without claiming that the checkbox grants remote rights or requiring a test mutation

#### Scenario: Setup response is interrupted
- **WHEN** the browser loses the setup response
- **THEN** it recovers the safe operation status without retaining the token, creating another connection or converting an uncertain outcome into a confirmed failure

#### Scenario: Setup is used at a narrow width
- **WHEN** an operator opens the form at a supported narrow viewport using either locale and keyboard navigation
- **THEN** all four field groups, validation feedback and primary action remain readable and operable without horizontal content overflow

### Requirement: Credential entry stays isolated and transient
The console SHALL collect raw credentials only in a separately authorized write-only integration setup/rotation boundary. The single integration setup form SHALL be allowed to combine credential entry with name, project address and permission choices, while ordinary metadata-only forms and pipeline editors SHALL remain credential-free. Existing credentials SHALL be represented by configured/missing state and safe metadata, with replacement rather than value read-back; editing non-secret fields SHALL NOT require reentering a configured token. Secret input SHALL remain transient, SHALL NOT enter browser persistence, URLs, query/mutation caches, analytics, confirmation summaries or errors, and SHALL be cleared after submission or leaving the flow. Server errors SHALL be rendered as safe classifications without echoing the submitted value. Selecting an existing permitted server-side handle SHALL NOT require retrieving its contents. Automatic retries and status recovery SHALL use safe operation references without browser-retained credentials.

#### Scenario: Administrator submits or rotates a token
- **WHEN** the protected combined setup or credential replacement form is submitted
- **THEN** the console shows a safe outcome, clears the secret field and exposes no token value in detail/review screens or cached state

#### Scenario: Administrator leaves an unfinished credential form
- **WHEN** the credential flow is closed or navigation leaves it
- **THEN** returning does not restore the secret from browser storage or client caches

#### Scenario: Administrator renames an existing integration
- **WHEN** the administrator changes only the display name of a configured connection
- **THEN** no token is retrieved, reentered or rotated and the native resource identity remains unchanged

### Requirement: Project access and capability readiness are explicit
The console SHALL provide a separate Project-access form for authorized administrators to select a saved connection/resource and grant/revoke its permitted purposes for an OctaCity Project. Task-read and task-modify SHALL have understandable operator labels, while repository-read and Delivery SHALL remain independently advertised/granted and SHALL NOT be bundled into task-modify. Offered purposes SHALL be a subset of the connection's declared purposes and installed ceiling; the UI SHALL explain that actual remote token rights can further restrict use. Pipeline users SHALL see only their authorized choices and SHALL NOT gain connection-administration rights by creating a pipeline. Project access SHALL require no token entry and SHALL NOT implicitly activate an input, synchronize tasks or start a Run. Connection checks SHALL be visibly read-only and distinguish configuration, authentication, resource access and confirmed/denied/unverified capability readiness from pipeline activation. Disablement/revocation SHALL show safe dependency effects without claiming to cancel already accepted remote requests or undo Work.

#### Scenario: Read access is available without Reporter access
- **WHEN** a Project has task-read access and no task-update grant
- **THEN** the console permits read intake, explains unavailable reporting and does not require a write credential to use the read source

#### Scenario: Administrator revokes access
- **WHEN** an administrator reviews revocation for a connection used by visible inputs and pending reporting
- **THEN** the console shows those dependents, explains blocked subsequent provider operations and preserves the distinction from accepted pipeline results

#### Scenario: Manual Factory intake remains selected
- **WHEN** the operator chooses manual intake with no provider connection
- **THEN** the console allows ordinary manual Work creation without opening integration setup or requiring tokens

#### Scenario: Project administrator grants a saved integration
- **WHEN** an authorized administrator opens Project access after saving a read-only connection
- **THEN** the form selects that connection by name, offers only allowed purposes and grants read access without token entry or a new connection

#### Scenario: Manual internal task creation is available
- **WHEN** the internal task capability is enabled and a user creates a manual task without an integration
- **THEN** integration setup and Project integration grants remain unnecessary and no provider or source identity is invented
