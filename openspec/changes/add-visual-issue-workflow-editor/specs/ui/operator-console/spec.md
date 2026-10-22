## MODIFIED Requirements

### Requirement: Project status settings and card moves are guided and independent
The Console SHALL expose bounded Project workflow settings through a graph canvas with editable status nodes, directed transition arrows and a contextual properties inspector. Initial and terminal selections SHALL remain workflow-level settings, with exactly one initial status and zero or more terminal statuses; permitted transition origins and supported evidence requirements SHALL use generated authoritative choices and reviewed immutable publication/default selection. The supplied default and custom workflows SHALL use the same viewing and editing controls. Manual creation SHALL work with the provided editable default without setup. Existing cards SHALL show their pinned version and require explicit reviewed compatible status correspondence for workflow adoption. Card controls SHALL enumerate only server-authorized organizational moves, explain ownership/evidence/applicability restrictions and confirm the current Issue/status precondition. History SHALL identify before/after status, reason and actor or pipeline/source provenance. A status move SHALL NOT be presented as execution start, gate approval or verification success. Stale intents SHALL require fresh review and preserve in-memory context without automatic resubmission.

#### Scenario: Project uses its own status vocabulary
- **WHEN** an operator publishes custom statuses and edges without a provider or configured pipeline
- **THEN** tasks use those settings through the same manual-create/move/card UI and no label spelling or phase name is required

#### Scenario: Old execution cannot move the current card
- **WHEN** the server classifies its configured action as stale or conflicting after a newer revision or status intent
- **THEN** the card retains its exact historical result and shows the unapplied transition rather than moving the card or relabeling the Run result

## ADDED Requirements

### Requirement: Workflow canvas displays the declared status graph
The Console SHALL use a graph canvas as the primary representation of the selected Project default and opened catalog workflow versions, including historical versions. Each status SHALL appear as a named node; each declared transition SHALL appear as a selectable, named, directed arrow. Initial and terminal selections SHALL have non-color-only markers. Branches, cycles, opposite directions and distinct transitions between the same statuses SHALL remain distinguishable. Status order or visual placement SHALL NOT imply undeclared transitions. Selecting a published node or arrow SHALL show its properties in a read-only inspector without mutating the workflow.

#### Scenario: Operator reads a branching workflow with a return path
- **WHEN** the opened version declares branches and a transition back to an earlier status
- **THEN** the canvas shows every declared node and directed transition, including the return path, and selection exposes the exact element's settings

#### Scenario: Operator opens an older workflow version
- **WHEN** an exact historical version is loaded from the catalog
- **THEN** its canvas and inspector show that version's statuses and transitions, with no arrows inferred from the status display order or current publication

### Requirement: Workflow authoring uses canvas nodes and a contextual inspector
Creating a workflow SHALL open an empty canvas with no copied default statuses or transitions. Operators SHALL be able to add and delete status nodes, create directed transitions by connecting nodes, delete transitions, and select either element to edit its properties in an inspector to the right of the desktop canvas. The inspector SHALL expose status names, workflow initial/terminal selections, and transition names, endpoints, origins and supported evidence through the existing workflow definition. Adding the first status SHALL select it as initial; removing a status SHALL remove its incident transitions and terminal reference, and removing the initial status SHALL select the first remaining status. Clearing a local draft SHALL be allowed, but an empty workflow SHALL NOT be published. All authoring controls SHALL edit one in-memory draft and use the existing REST validation, creation/publication and default-selection operations; pointer gestures SHALL NOT publish or select a default automatically. Existing task pins SHALL remain unchanged.

#### Scenario: Operator creates a workflow by drawing a transition
- **WHEN** the operator adds two named statuses to an empty canvas, connects them and configures the arrow in the inspector
- **THEN** validation and reviewed publication submit only those statuses and the configured transition through the existing workflow REST contract

#### Scenario: Operator removes the initial status
- **WHEN** the operator deletes the initial node with incoming or outgoing transitions
- **THEN** those transitions and its terminal reference disappear from the same draft, and the first remaining node becomes initial; a fully cleared draft remains unpublished

#### Scenario: Publication is rejected as stale
- **WHEN** the server rejects the reviewed workflow version precondition
- **THEN** the existing conflict flow preserves the local draft and requires refresh, validation and fresh review without silently overwriting the newer publication

### Requirement: Canvas navigation is accessible and separate from workflow policy
Operators SHALL be able to reposition nodes, pan, zoom and fit the graph to view. Layout, viewport and element selection SHALL remain transient presentation state and SHALL NOT change workflow status order, identities, transition policy or REST mutation bodies. Keyboard users SHALL be able to select nodes and transitions, add/remove them and establish connections through the same inspector and draft without pointer dragging. Focus and state labels SHALL remain visible, English/Russian presentation SHALL be complete, and the inspector SHALL remain reachable at supported narrow widths without horizontal page overflow. Draft and layout data SHALL NOT be persisted in browser storage.

#### Scenario: Operator rearranges the canvas
- **WHEN** the operator moves a node or changes the canvas viewport without changing policy fields
- **THEN** the graph presentation changes while the workflow definition and publication precondition remain unchanged and no mutation request is sent

#### Scenario: Operator authors a workflow with the keyboard at narrow width
- **WHEN** the operator adds/selects statuses, opens properties and creates a transition without dragging
- **THEN** the same draft can be configured and reviewed with understandable focus, and the inspector remains accessible within the page width
