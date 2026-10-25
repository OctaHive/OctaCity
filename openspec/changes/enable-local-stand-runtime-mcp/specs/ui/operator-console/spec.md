## ADDED Requirements

### Requirement: Runtime setup leads clearly to profile model and reasoning selection
The AI settings Runtime view SHALL explain that it prepares shared MCP/integration choices and finite permissions, while each AI-agent profile owns its model, plugin-supported reasoning parameters and budget. It SHALL provide a direct keyboard-accessible handoff to the existing AI-agent/profile authoring view under the same Project, with guidance for choosing or creating a profile. Navigation between Runtime setup and profiles SHALL preserve the existing in-memory profile draft and SHALL NOT publish it, select another model or create an AI agent/profile automatically. English and Russian labels and narrow layouts SHALL use existing Console navigation and form components.

#### Scenario: Operator wants to select a model from Runtime settings
- **WHEN** the operator follows the model/reasoning setup action from a Runtime under a selected Project
- **THEN** Console opens that Project's existing AI-agent/profile journey and explains where model and plugin-declared reasoning parameters are edited without another model editor or losing the unfinished profile draft

### Requirement: Runtime setup explains installation prerequisites and validation outcomes
Runtime settings SHALL distinguish an absent installation capability, an unsupported transport, missing executable/network authority and an unconfigured Project choice. Empty permission states SHALL identify the installation prerequisite and its supported setup path rather than telling the operator to configure unavailable controls on the same screen. HTTP/stdio entry and permission controls SHALL reflect qualified server choices. Configuration validation/publication feedback SHALL state its scope and SHALL NOT imply that an MCP connection was tested or that execution succeeded. Existing integration setup SHALL remain a separate optional flow rather than a prerequisite for every MCP server.

#### Scenario: Stand provides model discovery but no execution policy
- **WHEN** an operator opens Runtime settings on the existing metadata-only installation
- **THEN** the view keeps model/profile authoring reachable, explains the explicit local installation prerequisite and does not present empty permission controls as configurable execution authority

#### Scenario: Operator reviews settings without starting execution
- **WHEN** reviewed Runtime settings pass validation or publication
- **THEN** the outcome confirms configuration only and does not claim an MCP process or remote connectivity check occurred
