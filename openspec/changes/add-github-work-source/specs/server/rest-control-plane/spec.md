## ADDED Requirements

### Requirement: Factory source inputs have typed versioned management
The management REST API SHALL expose authorization-filtered discovery, inspection, validation, creation and immutable replacement of Project-owned Work Source input bindings and exclusive input groups. Representations SHALL expose safe source/reporter capability choices, installed repository choices, exact target Factory Configuration and Repository references, bounded Issue state/label predicates, input mappings, status mappings and enabled state without provider tokens or raw provider request payloads. Mutations SHALL require the existing idempotency and current-version preconditions. Validation SHALL return normalized input selection, safe incompatibilities and target/version details before activation.

#### Scenario: Client configures an approved defect input
- **WHEN** an authorized client selects a visible installed GitHub source, a visible target configuration, and the required `approved` and `bug` labels
- **THEN** validation and publication retain a typed immutable input binding under that Project and exact mutation replay returns the same version

#### Scenario: Client references a hidden source or target
- **WHEN** an input names a repository, source capability, target configuration or reporting grant unavailable to its Project
- **THEN** the API returns the safe authorization or validation outcome without revealing hidden resources or activating the input

### Requirement: Source previews and diagnostics distinguish discovery from execution
The management REST API SHALL expose bounded read-only selector previews and safe input/reporting diagnostics including effective selector, exact versions, last scan/progress, next retry, candidate dispositions and visible admitted Work/Run correlations. A preview SHALL use installed read authority, respect provider bounds and SHALL NOT reserve identities, advance production cursors, admit Work or write to GitHub. Diagnostics SHALL distinguish filtered, ambiguous, replayed, newly admitted, unavailable and projection-pending/exhausted states without claiming preview results or external labels are authoritative execution outcomes. Raw Issue bodies and provider secrets SHALL be excluded from preview and diagnostic summaries.

#### Scenario: Client previews an unrestricted label selector
- **WHEN** a client previews a visible bounded `open` Issue source without required labels
- **THEN** the API returns safe labeled and unlabeled candidate summaries and selector reasons without creating Runs or modifying scan progress

#### Scenario: GitHub returns a rate limit
- **WHEN** preview or discovery is deferred by the provider
- **THEN** diagnostics expose a safe scoped retry time instead of reporting no matching Issues or global server failure

### Requirement: Stage source-update actions have typed configuration and provenance
The management REST API SHALL support configuring and validating source label-update effects for individual declared Flow stage outcomes through existing versioned Flow/action publication. Clients SHALL be able to select the original source binding, visible Reporter mapping and explicit labels to add/remove without entering provider wire payloads or credentials. Publication SHALL freeze the resolved mapping and reject conflicting or unauthorized effects. Diagnostics SHALL correlate each queued/applied/failed projection to its originating stage, outcome and action identity, including intermediate stages before the Run is terminal.

#### Scenario: Client configures label changes after an intermediate stage
- **WHEN** an authorized client configures a declared stage outcome to add and remove selected permitted labels
- **THEN** validation presents those exact source effects and publication retains them in the immutable Flow/action configuration independently of other stage outcomes
