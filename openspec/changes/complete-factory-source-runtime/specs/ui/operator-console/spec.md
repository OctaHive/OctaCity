## ADDED Requirements

### Requirement: Automatic-intake authoring works against the released runtime
The pipeline-input Console journey SHALL work through the deployed protected API and show current authorized connections, compatible Source/preparation choices, separate code Repository/target choices, predicates, optional Reporter mappings, preview and confirmed publication. Inputs SHALL become active only through explicit operator intent. The form SHALL distinguish missing connection/grant, missing installed runtime/profile, incompatible configuration, version conflict, provider throttling and transport failure using localized actionable safe guidance. An undiscriminating statement that a request failed safely SHALL NOT be the only explanation of a classified failure. Unsupported features SHALL NOT appear ready because connection checks succeeded; no arbitrary defaults or opaque identity entry SHALL substitute for a missing profile. Manual intake SHALL remain accessible when automatic intake is absent or fails.

#### Scenario: Operator configures a source from an existing integration
- **WHEN** an operator with an installed compatible profile and current Project access opens automatic intake
- **THEN** the form lists valid choices, allows preview and confirmed publication, and subsequently displays the input's retained version and Source progress without credentials or private configuration editing

#### Scenario: Runtime profile is not installed
- **WHEN** the connection is configured but no compatible automatic-intake runtime/profile is available
- **THEN** the form names the missing capability and appropriate deployment action, preserves permitted connection details and manual intake, and does not describe the condition as a successful empty source scan

#### Scenario: Work is admitted and its reporting is deferred
- **WHEN** an enabled input admits Work and the configured Reporter is temporarily unavailable
- **THEN** the Console links the actual Work/Run and displays the separate deferred reporting state rather than losing the accepted result or claiming remote success

#### Scenario: Operator retries after server recovery
- **WHEN** a previously failed input query succeeds after runtime recovery
- **THEN** retry replaces the error with the actual bounded data or empty state without another credential submission or duplicate publication
