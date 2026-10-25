## ADDED Requirements

### Requirement: Automatic-intake authoring works against the released runtime
The existing pipeline-input Console journey SHALL use the deployed protected API to show current authorized connections, compatible Source/preparation choices, independent code Repository/target, predicates, optional Reporter mappings, preview and confirmed publication. Input activation SHALL be explicit and independent of Project Issue import and synchronization. The form SHALL distinguish missing connection/grant/runtime/profile, incompatible configuration, stale version, provider throttling and transport failure with localized actionable safe guidance. Generic safe-failure text alone, opaque identity entry or arbitrary defaults SHALL NOT conceal missing installed support. Manual tasks and intake SHALL remain accessible when automatic intake is absent or fails.

#### Scenario: Operator configures a source from an existing integration
- **WHEN** an operator with compatible installation and current Project access opens automatic execution input settings
- **THEN** authorized choices, preview and confirmed publication work without token/private-file entry, with actual input progress and independent Issue/Work/Run links when present

#### Scenario: Runtime profile is not installed
- **WHEN** a connection exists but lacks a compatible consumer/preparation runtime profile
- **THEN** the form explains the missing capability and deployment action without presenting a successful empty source scan or requiring unrelated task/workflow setup

#### Scenario: Operator retries after server recovery
- **WHEN** a failed input query succeeds after runtime recovery
- **THEN** retry replaces the old failure with actual data/empty state without another credential submission or duplicate publication

### Requirement: Project tasks expose import and separate status synchronization
The Console SHALL offer authorized Project Source discovery, bounded preview and reviewed import controls before an Issue or Factory Configuration exists. A successfully imported native task SHALL appear through the existing Issue list/card with its primary source and current synchronization state. The completed workflow canvas and inspector SHALL remain the business-status editor with existing validation, frozen publication, conflict review and separate default selection. Native field/status translation SHALL remain a separately reviewed supported source mapping against the Issue's exact workflow pin, explicit ownership and current read/write grants. Import, field refresh, business status, execution and delivery SHALL be presented separately; neither connection setup, workflow publication nor import SHALL imply a Run or native mutation. Controls SHALL retain English/Russian, keyboard/narrow-width access, safe errors and transient draft behavior.

#### Scenario: Operator imports a task before configuring a pipeline
- **WHEN** the operator previews an authorized native resource and confirms Project import without a Factory target or writer
- **THEN** a canonical Issue becomes visible with ordinary initial status and source correlation, and the UI shows no execution or completed remote write

#### Scenario: Operator maps an existing business status
- **WHEN** a linked Issue uses a published workflow and the operator reviews a supported OctaCity-owned push mapping
- **THEN** native states/labels and granted directions are reviewed separately from the workflow canvas and a permitted explicit status move exposes its actual outbound projection

#### Scenario: Workflow default changes during source mapping review
- **WHEN** a different workflow/default is published while the operator edits a linked Issue mapping
- **THEN** the Issue retains its pinned workflow until explicit adoption and stale/incompatible mapping review cannot silently switch it to the latest default

#### Scenario: Accepted status has deferred reporting
- **WHEN** a permitted local or pipeline business transition is retained but its native update cannot yet be delivered
- **THEN** the card keeps the accepted local status and actual execution results and shows pending/conflicting/exhausted synchronization rather than claiming remote success or rolling back the status
