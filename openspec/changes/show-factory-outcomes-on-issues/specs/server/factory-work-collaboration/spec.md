## ADDED Requirements

### Requirement: Issue Overview resolves the latest eligible summary per slot

The system SHALL resolve a bounded set of current summaries for the exact Issue and its associated Work and Runs in the same Project. Each slot SHALL select the most recently admitted applicable execution with an eligible accepted result. Within that execution it SHALL select the latest applicable accepted occurrence. Ties SHALL resolve deterministically. A newer execution that has yet to produce a result SHALL allow the previous applicable result to remain current. A result for superseded requirements SHALL be excluded from the current summaries and SHALL remain available through authorized historical reads subject to existing retention.

The system SHALL check current visibility of the Issue, original execution and linked resources before returning summaries or links. It SHALL use exact retained references for specifications and reviews. Hidden resources SHALL leave no visible reference or count. A previously visible resource that has expired SHALL be described as unavailable when its retained metadata is still visible.

#### Scenario: An older retry finishes after a newer accepted triage result

- **WHEN** both executions belong to the same Issue requirements and publish to the same slot
- **THEN** the newer admitted execution's accepted result remains current without adding a duplicate card

#### Scenario: Different stages contribute different cards

- **WHEN** visible accepted outcomes fill the triage and specifications slots
- **THEN** each slot has its own current conclusion and source Run

#### Scenario: Requirements change after a review

- **WHEN** the Issue requirements change after the review was accepted
- **THEN** the earlier review is available through its historical execution and excluded from current Issue summaries

### Requirement: Explicit triage classification can update local Issue planning

An explicitly enabled triage action SHALL apply accepted priority and the mapped size label to the exact associated Issue through existing guarded Issue mutation rules. Priority SHALL use supported Issue priority values. A size label SHALL resolve to an existing active Project label through a reviewed mapping. The action SHALL preserve unrelated labels and SHALL replace only a prior size label owned by the same configured classification policy. Unknown assessments SHALL leave the corresponding field alone.

The mutation SHALL verify current Project and Issue write authority, requirements applicability, ordering and the current values of fields the action intends to change. A conflicting operator edit or newer applicable classification SHALL take precedence over a stale action. The selected valid changes SHALL commit together in one Issue revision with Run and outcome provenance and scoped idempotent replay. A conflict or missing authority SHALL leave the accepted assessment readable for an authorized viewer. This local action SHALL produce no external provider intent and SHALL preserve the original Work scheduling priority.

#### Scenario: An authorized classification is delivered twice

- **WHEN** an accepted classification with priority and a mapped size is applied and the same operation is retried
- **THEN** the Issue retains one resulting revision, its priority and one owned size label, while unrelated labels are preserved

#### Scenario: An operator changes priority during triage

- **WHEN** an older outcome attempts to write priority after an intervening conflicting operator edit
- **THEN** the action reports a conflict and the Issue keeps the operator's edit

#### Scenario: Triage returns an unknown priority

- **WHEN** the accepted result has an unknown priority and a valid mapped size
- **THEN** the local action applies only the valid size update
