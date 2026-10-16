## ADDED Requirements

### Requirement: Pipeline inputs expose guided GitHub Issue selection
The console SHALL allow an operator to configure GitHub Issues as a source at the selected Factory pipeline input, choosing visible installed repository/capabilities, target configuration, Issue state, all-label, any-label and excluded-label sets, input mapping, optional exclusive group and enabled state. It SHALL explain empty label selection as unrestricted, provide bounded read-only preview and authoritative validation, and publish reviewed immutable replacements through the existing confirmed-intent workflow. The operator SHALL NOT need to write provider JSON, enter opaque resource identities or supply a GitHub token in the browser. Selector drafts and preview payloads SHALL remain outside browser persistence.

#### Scenario: Operator prepares assessment and implementation inputs
- **WHEN** an operator configures an assessment input with no required labels and separate `approved + bug` and `approved + feature-request` inputs
- **THEN** the console shows each input next to its exact target pipeline and makes exclusions and exclusive grouping reviewable before activation

#### Scenario: Operator selects no required labels
- **WHEN** all three label sets are empty
- **THEN** the console explains that labeled and unlabeled Issues can match and does not silently add a label restriction

#### Scenario: Server rejects an incompatible input
- **WHEN** validation reports a hidden dependency, contradictory selector, unsupported mapping or stale version
- **THEN** the console identifies the safe error in the appropriate section, preserves the in-memory draft and prevents activation until a new valid intent is confirmed

### Requirement: GitHub reporting mappings and source progress are reviewable
The console SHALL expose guided configuration of arbitrary Work status values mapped to permitted managed-label additions/removals and optional Issue open/close state. It SHALL present safe Source/Reporter status and Work/Run correlations from REST, distinguish matching from admitted and pending projection from applied effects, and identify ambiguous grouped inputs without deriving lifecycle authority from GitHub labels. Reporting and source choices SHALL be filtered by server-published capabilities; no control SHALL grant itself write authority or expose protected provider material.

#### Scenario: Operator configures assessment reporting
- **WHEN** an operator maps accepted `approved` and `rejected` outcomes to mutually exclusive managed labels
- **THEN** review shows exact label effects, unrelated-label preservation and whether Issue state will change before publication

#### Scenario: Status projection is pending
- **WHEN** a Run completed assessment but its GitHub update is retrying or exhausted
- **THEN** the console shows those distinct facts and does not display the downstream pipeline as admitted without an authoritative admission receipt

### Requirement: Stage authoring exposes source label-update actions
The console SHALL let an operator configure a source-update action for any declared stage outcome in the existing Flow editor, including intermediate and nested stages. The editor SHALL expose the original Issue source, permitted labels to add, labels to remove and the exact resulting mapping/effects, and SHALL persist them as ordinary immutable trusted-action configuration rather than a special stage implementation. Different outcomes SHALL support different effects, and stages without an action SHALL leave the Issue unchanged. No approval, rejection or classification vocabulary SHALL be mandatory.

#### Scenario: Operator adds an update after a nonterminal stage
- **WHEN** an operator selects an intermediate stage outcome and configures removal of one arbitrary label and addition of another
- **THEN** the editor shows the source-update action on that route, validates its authority through REST and allows publishing without moving it to the pipeline's terminal stage

#### Scenario: Operator inspects stage update progress
- **WHEN** an intermediate stage committed and its source update is pending, applied or failed
- **THEN** the console associates that reporting state with the exact stage/action and keeps it distinct from the overall pipeline result
