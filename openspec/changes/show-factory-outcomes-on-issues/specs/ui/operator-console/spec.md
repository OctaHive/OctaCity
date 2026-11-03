## ADDED Requirements

### Requirement: Flow authors can enable a card from an outcome

The outcome editor SHALL provide **Show on Issue** and use the supported outcome type to supply built-in titles, field labels and layout. For custom outcomes the editor SHALL allow a title and a brief conclusion field. The publication review SHALL show which slot and result fields will appear. A separate **Apply classification to Issue** setting SHALL default to off and SHALL show the existing priority and size-label choices it can apply. Project label mappings SHALL be reusable across Flows. The ordinary read-only display path SHALL require no mapping editor, additional graph step, provider connection or credentials.

#### Scenario: An operator enables the standard triage card

- **WHEN** the operator enables display on a compatible triage outcome and reviews publication
- **THEN** the review shows the triage card and its fields without requiring per-field mapping

### Requirement: Issue Overview presents concise work outcomes

The Issue Overview SHALL show an **Outcomes** section with one concise card per current slot. Each card SHALL show a human-readable title, short conclusion, selected facts and meaningful links to visible resources and the original Run. Triage SHALL show decision, duplicate-check coverage when available, and available priority, size and component. It SHALL distinguish a recommendation from a confirmed assignment and represent unknown or provisional assessments accurately. A specifications card SHALL link to retained specification documents. A review card SHALL show its verdict and link to the retained review. Custom cards SHALL use the same visual hierarchy.

The Console SHALL use localized labels and accessible keyboard navigation. Technical IDs, raw payloads and attempt history SHALL be available through existing detail views where authorized. An Issue with no eligible summary SHALL keep its ordinary Overview usable.

#### Scenario: An operator opens a task after triage and specification preparation

- **WHEN** both accepted results are visible
- **THEN** the Overview shows a readable triage conclusion and a specifications card with descriptive document links

#### Scenario: Duplicate search was unavailable and priority was only recommended

- **WHEN** the accepted result records those facts
- **THEN** the card shows that duplicates were not checked and labels the priority as recommended

#### Scenario: A later triage succeeds

- **WHEN** a newer eligible triage result is accepted for the same slot
- **THEN** the Overview replaces the previous triage card and links to the newer original Run
