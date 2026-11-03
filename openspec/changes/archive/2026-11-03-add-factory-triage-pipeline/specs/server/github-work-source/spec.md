## MODIFIED Requirements

### Requirement: GitHub Issues are selected by explicit pipeline input predicates
Each GitHub Work Source input SHALL select an installed repository and Issue state (`open`, `closed` or `all`) plus bounded `all_of`, `any_of` and `none_of` label sets. A match SHALL contain every `all_of` label, at least one `any_of` label when that set is nonempty, and no `none_of` label. Three empty label sets SHALL mean no label restriction, including unlabeled Issues. An optional advertised `label_presence` predicate SHALL accept `any` or `none`; omission SHALL retain `any`. `none` SHALL require an actually empty observed label set and SHALL reject nonempty `all_of` or `any_of` as contradictory. Unsupported predicates SHALL be rejected explicitly, not ignored. Invalid or contradictory selectors SHALL be rejected before activation. Label names SHALL be operator configuration rather than built-in Factory states. Pull requests SHALL be excluded even when returned by the provider's Issues listing. Preview, group selection and actual admission SHALL use the same retained predicate semantics.

#### Scenario: Operator selects every open Issue
- **WHEN** an enabled input selects `open` with three empty label sets and omits label presence
- **THEN** labeled and unlabeled open Issues are eligible for ordinary admission while closed Issues and pull requests are excluded

#### Scenario: Operator selects approved defects
- **WHEN** an input requires `approved` and `bug`, excludes `rejected`, and an Issue has those required labels without the excluded label
- **THEN** the Issue matches that input regardless of its unrelated labels

#### Scenario: Selector uses alternatives and exclusions
- **WHEN** an input requires `approved`, accepts either `feature-request` or `enhancement`, and excludes `blocked`
- **THEN** only Issues satisfying all three predicates match, and an absent alternative or present excluded label prevents admission

#### Scenario: Selector is contradictory
- **WHEN** an input both requires and excludes the same label or exceeds its declared predicate bounds
- **THEN** validation rejects that input with a safe field-level error and no polling or admission begins

#### Scenario: Operator selects strictly unlabeled tickets
- **WHEN** a supported input selects open Issues with `label_presence: none`
- **THEN** preview and actual admission include only Issues whose observed label set is empty, excluding even unrelated labels and all PRs

#### Scenario: An older installation cannot enforce unlabeled selection
- **WHEN** an input requests label presence from an installation that does not advertise that predicate
- **THEN** validation reports unsupported configuration without interpreting empty label sets as the requested restriction

#### Scenario: A retained legacy predicate is loaded
- **WHEN** the server reads or replays an input whose original predicate omitted label presence
- **THEN** its original canonical definition, predicate digest and exact retained pins remain valid without republishing or silently migrating the input

#### Scenario: Triage adds labels after admission
- **WHEN** accepted reporting labels the original ticket and a subsequent scan no longer matches it
- **THEN** its retained Work/Run and independently enabled source observation remain valid without a new intake cycle

## ADDED Requirements

### Requirement: Raw-ticket triage uses explicit purpose-bound completion inputs
An authorized triage Source preparation policy SHALL be able to bind a reviewed immutable same-Project completion document through the shared managed-input service instead of requiring implementation acceptance authored in the ticket. First-time setup SHALL support the reviewed versioned preset document without a prerequisite Build or separate document-upload workflow; ordinary Source publication SHALL freeze its content/reference and preparation SHALL retain its managed input before admission. Its exact reference, digest, purpose and preparation policy SHALL be frozen with the retained Source snapshot and ordinary Work inputs. Title-only tickets SHALL be admissible when this policy permits absent body. Missing detail SHALL remain absent and SHALL be handled by the declared triage result policy.

This completion document SHALL describe success of the triage process, SHALL NOT be represented as source-authored ticket acceptance, and SHALL NOT populate an Issue acceptance field or certify later implementation. Existing required field/body/heading acceptance mappings SHALL retain their behavior; no missing-content fallback SHALL select the triage mode implicitly. Cross-Project, unavailable, expired, altered or insufficiently retained documents and oversized source content SHALL fail before admission without fabricated or truncated requirements.

#### Scenario: An unlabeled ticket has no Acceptance section
- **WHEN** its reviewed triage policy pins an authorized retained triage completion document and the source snapshot fits the configured bounds
- **THEN** ordinary admission retains one Work/Run with distinct ticket and process-completion provenance without changing the Issue's absent implementation acceptance

#### Scenario: Existing implementation input lacks acceptance
- **WHEN** a non-triage input requires a body heading that is absent
- **THEN** it retains the existing missing-acceptance disposition and starts no Run by borrowing triage completion criteria

#### Scenario: Completion criteria change during preparation
- **WHEN** a document is replaced after its exact input reference was frozen
- **THEN** recovery uses the original authorized retained document or fails safely without selecting the replacement or duplicating the Run
