## ADDED Requirements

### Requirement: Released automatic-input management is operationally complete
When automatic intake is installed, the released server SHALL compose the existing protected input/group discovery, exact-version reads, choices, validation, immutable publication, read-only preview, dependency history and Source/Reporter diagnostic operations over the same authoritative store and trusted profile selections used by execution. A valid request SHALL NOT fail merely because its advertised handler was omitted from composition. Empty history SHALL return a bounded successful empty collection. A missing installation, incompatible profile, denied grant, stale version or provider outage SHALL retain distinct safe outcomes; missing runtime support SHALL NOT be disguised as an empty successful result. Optional automatic intake SHALL NOT be a prerequisite for manual admission or independent manual Issue creation when available.

#### Scenario: Granted integration is selected in a pipeline
- **WHEN** an authorized operator opens automatic-input settings for a visible Factory Configuration and a current connection/resource grant with a compatible installed Source profile
- **THEN** the API supplies the granted choices, current inputs and exact selectable preparation/target references, and accepts a valid explicitly confirmed versioned publication
- **AND** credential entry, connection creation and Project granting are not repeated in that form

#### Scenario: No input has used the connection
- **WHEN** an authorized operator requests retained dependencies or current inputs for a configured Project with no matching entries
- **THEN** the relevant bounded collection returns an empty successful page rather than an unavailable-handler error

#### Scenario: Source support or authority is missing
- **WHEN** a request depends on an uninstalled Source profile or a resource outside the current Project grant
- **THEN** the API returns the appropriate safe capability or authorization result before provider invocation and discloses no hidden resource or secret

#### Scenario: Publication response is lost
- **WHEN** publication commits and the client retries the same intent/key after losing its response
- **THEN** management returns the original input/group version and neither enables a duplicate worker nor creates another admission

### Requirement: Runtime diagnostics distinguish configuration from intake progress
Automatic-intake diagnostics SHALL distinguish installed support, valid configuration, enabled input, supervised worker availability, provider-deferred scan, candidate disposition, accepted Work/Run and reporting delivery. Configuration/read checks and previews SHALL NOT be presented as evidence of polling, admission or successful reporting. Safe classified reasons and request correlation SHALL allow the operator to choose a relevant next action without exposing private paths, raw provider responses, credential values or Issue bodies. Runtime faults SHALL remain scoped to affected inputs/effects.

#### Scenario: Provider is throttled after input activation
- **WHEN** an enabled input's worker receives bounded provider retry advice
- **THEN** diagnostics identify that input as deferred with its retry state while retained Work/Run and ordinary server/manual readiness remain available

#### Scenario: Reporting is pending after accepted execution
- **WHEN** an accepted configured action queues an external update that has not completed
- **THEN** diagnostics retain the exact action/source correlation and pending or exhausted projection independently of the accepted execution result
