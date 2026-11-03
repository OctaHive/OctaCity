## ADDED Requirements

### Requirement: Flow outcomes can declare a concise Issue summary

An authorized Flow author SHALL be able to enable an Issue summary on a declared step outcome. The immutable Flow version SHALL retain the result slot, presentation type and bounded source fields used by the summary. Triage, specifications and review SHALL offer built-in presentations with known field meanings. A custom presentation SHALL support a title, a brief conclusion and supported typed references. All presentations SHALL use the same outcome binding and publication validation. Validation SHALL reject incompatible fields and ambiguous producers for one slot in a single execution. Mutually exclusive outcomes SHALL be able to supply the same slot.

Only the accepted result for that declared outcome SHALL supply summary content. Free text SHALL be limited to the designated conclusion field. Existing Flow versions without the setting SHALL retain their execution behavior and SHALL expose no new Issue summaries. The display setting SHALL grant read presentation only.

#### Scenario: Two branches produce a triage conclusion

- **WHEN** a Flow declares separate eligible and needs-review outcomes for the same triage slot on mutually exclusive paths
- **THEN** either accepted outcome can supply that slot and carries its own conclusion and available facts

#### Scenario: A draft selects an incompatible output

- **WHEN** an author chooses a review presentation whose referenced outcome fields cannot supply its required contract
- **THEN** validation identifies the incompatible fields before publication

#### Scenario: A step retains execution details alongside its conclusion

- **WHEN** the configured outcome accepts a short conclusion alongside private working material
- **THEN** only the configured summary fields are eligible for Issue display
