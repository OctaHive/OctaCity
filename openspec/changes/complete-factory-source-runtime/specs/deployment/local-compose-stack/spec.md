## ADDED Requirements

### Requirement: Factory stand advertises only composed automatic-intake capability
The documented `tools/factory-stand` path SHALL install and compose the trusted profiles, protected routes and Source/optional Reporter lifecycle needed for its advertised automatic-intake features. Runtime/profile selections SHALL derive from explicit installed policy, selected connection resources and Project configuration, not fixed real repository names, credential bytes or Project UUIDs in production code. Existing installations SHALL be adopted compatibly without replacing referenced immutable bundles or resetting state. Connection setup and Project grants SHALL not start intake; explicit enabled input publication SHALL remain necessary. Readiness SHALL distinguish optional absence from advertised-but-broken composition and SHALL NOT require a live provider call or remote write during startup. Provider outages SHALL not block ordinary CI/manual readiness.

#### Scenario: Supported stand starts before an input is enabled
- **WHEN** a stand with installed Source support starts without enabled inputs while GitHub is unreachable
- **THEN** the protected authoring/catalog/history routes work, supervised intake support is reported truthfully, no real Issue is admitted, and provider availability remains a separate check/deferred condition

#### Scenario: Advertised source handler or worker is omitted
- **WHEN** the stand advertises installed automatic intake but lacks required composition
- **THEN** scoped readiness/qualification fails with an actionable diagnostic instead of declaring that feature ready based only on server health or connection checks

#### Scenario: Retained stand restarts
- **WHEN** a stand with configured credentials, grants, inputs, accepted Work/Run and pending reporting restarts without reset
- **THEN** private directory initialization succeeds, worker ownership recovers, exact retained state is reused and eligible progress resumes without duplicate Work or extra agents

### Requirement: Packaged automatic intake has end-to-end qualification
Qualification SHALL exercise the released browser/gateway/server, actual PostgreSQL and object storage, installed adapter host/process and supervised worker lifecycle with a hermetic approved provider fixture. It SHALL configure a connection and separate Project grant through the supported UI, preview and publish an input, observe one eligible native task become actual Work/Run with retained results, apply an explicitly configured effect only to the fixture, and verify restart/replay and revocation. The native ticket source and code Repository SHALL be independently selectable. Read-only/no-Reporter, manual-intake and no-card variants SHALL be proven. Browser API stubs or direct internal use-case calls alone SHALL NOT satisfy this gate. Optional live GitHub read qualification SHALL be recorded separately; live mutation qualification SHALL require an explicitly authorized disposable target and SHALL NOT be run against an ordinary user repository implicitly.

#### Scenario: Full packaged journey passes
- **WHEN** the real Console selects an approved fixture resource, previews it and confirms its enabled input using valid mapped task/acceptance content and a qualified target
- **THEN** the supervised server creates and exposes exactly one native-correlated Work/Run with observable retained execution results, and an accepted configured reporting action produces its exact fixture effect
- **AND** repeating the scan and restarting preserve that same admission and bounded reporting history

#### Scenario: Real browser requests reach an omitted runtime service
- **WHEN** UI components would pass with mocked responses but the deployed source API returns unavailable due to missing composition
- **THEN** the packaged qualification fails and cannot be reported as completed by substituting fixtures at the browser REST boundary
