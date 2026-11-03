## ADDED Requirements

### Requirement: Project Factory policy settings expose real editable rules
Project settings SHALL contain a Factory section with separate Launch policies and Access profiles catalogs. Authorized operators SHALL be able to view Standard and custom records, create records, publish replacements and explicitly choose exact Project default versions. Launch editing SHALL expose the two supported manual/automatic admission controls and explain independent Source activation. Access editing SHALL reuse typed qualified permission controls, show effective constraints and their limiting sources, and distinguish Project-editable narrowing from installation prerequisites. Policy editing SHALL NOT require raw JSON, opaque references, credentials or another Runtime editor. Saving SHALL confirm a configuration change only and SHALL start no model, Build or Work.

#### Scenario: Operator opens Factory settings for the first time
- **WHEN** an operator opens a provisioned Project's Factory settings
- **THEN** named Standard launch and access records are inspectable with actual rules, default status and versions, and missing installation authority has a visible explanation

#### Scenario: Operator changes an access profile
- **WHEN** an operator edits supported tools, hosts or another permission category
- **THEN** the editor preserves other supported settings, previews the authoritative effective ceiling and reports over-ceiling selections at their field before confirmed publication

#### Scenario: Concurrent publication conflicts
- **WHEN** another operator replaces the current policy before confirmation
- **THEN** the editor preserves the in-memory draft, identifies the conflict and requires a new review rather than silently resubmitting against the latest version

### Requirement: Configuration policy selectors show contents and preserve deliberate choices
Factory Configuration authoring SHALL visibly label Launch policy, Root Flow and Access profile selectors. A new draft SHALL initially select exact Project defaults and show their names, versions, concise contents and View/Edit links. The operator SHALL retain explicit manual selection and all supported advanced settings. An empty or inaccessible choice SHALL explain its prerequisite rather than display an unexplained None. Raw identifiers/digests SHALL remain in Details. Navigation to and from Project policy editing SHALL preserve the mounted Configuration draft; a newly published policy version SHALL require explicit adoption. Changing a Project default SHALL NOT replace an existing draft choice or repin saved Configurations.

#### Scenario: Operator creates a Configuration without prior policy setup
- **WHEN** a provisioned Project has valid defaults and the operator creates a Configuration draft
- **THEN** both exact defaults are selected with readable summaries, and the operator can inspect, edit or override them without creating installation aliases

#### Scenario: Operator returns from policy settings
- **WHEN** an operator opens the selected policy from an unfinished Configuration, publishes or cancels an edit, and returns
- **THEN** unrelated Configuration fields and its original policy pins remain intact, and a newly published version is offered for explicit adoption

#### Scenario: Existing legacy selection is inspected
- **WHEN** an operator opens an older Configuration using a mapped installation reference
- **THEN** the Console shows the concrete mapped rules and original-reference provenance under Details, while an unresolved mapping remains an actionable blocker and is not silently replaced

#### Scenario: Localized form is used on a phone
- **WHEN** an operator uses the English or Russian policy settings and Configuration selectors at a 390-pixel viewport with the keyboard
- **THEN** visible labels, summaries, validation and confirmation remain readable without horizontal overflow, menus restore focus and all supported advanced settings remain reachable
