## ADDED Requirements

### Requirement: Factory stand integration setup is ready before project selection
The `tools/factory-stand` startup path SHALL install the pinned supported adapter, protected integration management transport, credential publication support and explicit bounded resource-enrollment policy independently of any real external project. A clean default startup SHALL NOT require a preselected GitHub repository, token, CLI authentication or unauthenticated GitHub repository discovery. The console SHALL allow the operator to enter a supported project address and token through the protected setup form. A demo repository SHALL NOT be the sole allowed resource or a hidden prerequisite for setup. Existing explicit installed resource configurations SHALL remain supported without silently widening their ceilings. Ordinary restart SHALL retain connections, credentials, Project grants, enrolled resources and accepted Work/Run history.

Startup readiness SHALL verify installed setup capability and connection discovery through the actual browser gateway. Missing setup composition or forbidden/unavailable integration routes SHALL produce actionable launch failure rather than successful integration readiness. The protected gateway service identity SHALL remain verified; direct unverified access SHALL remain forbidden. Adapter-ready startup SHALL be independent of external provider availability, and later connection checks SHALL report that availability separately.

#### Scenario: Clean Factory stand starts while GitHub is unavailable
- **WHEN** an operator starts tools/factory-stand with empty integration state and no token while GitHub cannot be reached
- **THEN** startup exposes installed GitHub setup and the connection list through the protected gateway without resolving a real repository, and Add integration is available
- **AND** submitting a connection later reports provider unavailability without claiming successful setup

#### Scenario: Operator selects a different repository
- **WHEN** an operator uses the setup form to select an allowed non-demo repository under the stand's explicit enrollment policy
- **THEN** no command-line repository flag, source edit, image rebuild or manual private configuration rewrite is required

#### Scenario: Existing stand is restarted
- **WHEN** a stand with installed private configuration, enrolled resources and retained execution history is restarted
- **THEN** compatible state is reused without replacing identities, discarding credentials, widening old installation policy or duplicating Work

#### Scenario: Integration composition is missing
- **WHEN** the server is healthy but protected setup capability or connection discovery is forbidden or unavailable through the gateway
- **THEN** the launcher fails with a scoped integration readiness diagnostic before reporting the Factory stand ready

#### Scenario: Client bypasses the protected gateway
- **WHEN** a client calls protected setup or credential administration through unverified direct server ingress
- **THEN** the request is forbidden before private preparation, enrollment or provider access
