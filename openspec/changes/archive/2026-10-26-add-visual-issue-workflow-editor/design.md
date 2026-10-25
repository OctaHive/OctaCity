## Context

See `proposal.md` for motivation and the UI delta for behavior. `WorkflowView`,
`WorkflowCatalog` and `WorkflowEditor` already own bounded discovery, version
selection, a component-memory definition and validated/confirmed publication.
`WorkflowFields` currently renders every status and transition as a form;
`useWorkflowSummary` supplies textual publication/default-selection review.

`IssueWorkflowsApi` already exposes every required read and mutation. The generated
contract supplies limits and origin/evidence choices. Factory's SVG overview is
specialized to Factory routing; Build's `layoutDag` assumes a dependency graph.
Neither is a ready-made interactive state-machine editor. UI policy currently
allows no graph runtime dependency.

## Goals / Non-Goals

**Goals:** Make one definition directly viewable/editable as a graph, retaining
existing publication and task semantics with a small frontend-only change.

**Non-Goals:** A general graph framework, migration of Factory/Build views, a workflow
execution engine, n8n integrations, collaborative editing, undo history, minimaps,
import/export, persistent node positions or new server contracts. Verification
uses existing tests and gates, not a new qualification or benchmark subsystem.

## Decisions

### 1. One workflow canvas with read-only and editable modes

Keep a feature-local graph component and contextual inspector under
`ui/src/features/issueWorkflows/`. Render the default and selected catalog versions
with the same graph used by the editor. Select by stable status/transition ID.
The desktop inspector sits beside the canvas; at narrow widths it stacks below
or opens through an accessible properties control.

Use SVG connections and semantic DOM controls, following existing console tokens
and graph presentation patterns. This avoids a new graph dependency and a second
form-based editor. Do not import Factory business components or turn their types
into a shared graph language; extract a shared primitive only if actual reuse
appears during implementation.

### 2. Keep the existing definition as the only policy draft

Node and arrow operations edit the existing generated `WorkflowDefinition`.
The inspector reuses current inputs, selection controls and generated choices.
Initial/terminal toggles update `initial_status_id` and `terminal_status_ids`,
not status flags. Reuse the existing add/delete/reference-cleanup behavior and
first-status initial selection rather than maintaining parallel form and graph
mutation paths. The inspector exposes the workflow name and only the selected
status's name, initial radio selection and terminal checkbox, plus its delete
action. Do not duplicate the canvas with a status list, display technical IDs or
offer manual status-order controls. The existing array order remains a storage
and initial-layout detail; connections alone define permitted transitions.
Moving a node must not reorder the statuses array. Publication review lists names
without arrows between adjacent statuses and describes declared transitions separately.

Drag a connection point at the center of a node's right edge to another card.
The permanent connection begins at that same point. Arrowheads keep a fixed size
when selection thickens the line. A completed connection creates one draft
transition and selects its inspector for name/origin/evidence settings.
Add transition or keyboard activation of the point opens From/To controls with
explicit Connect/Cancel actions in that same inspector. Both inputs use one
creation operation; self-connections, invalid drops and Escape add no edge.
Deletion removes incident edges/references in that same draft and moves focus to
a remaining element or Add status. Blank drafts and unfinished fields remain
editable; validation/publication keep the existing requirements.

### 3. Separate bounded presentation state from policy

Hold positions keyed by status ID, viewport and selection only in mounted component
memory. Place compact cards in columns from the initial status by traversing declared
connections, with branches in separate rows and ports centered on the side edges.
Use direct horizontal lines between aligned cards and rounded orthogonal bends
between rows. Route return paths and parallel arrows in separate exterior lanes.
Bound transition labels by the available span and use visual ellipsis, retaining
the full accessible name and tooltip.
Visit each status once during placement so cycles remain supported, and retain
the status array and stable IDs unchanged. Render
every declared edge; do not run a DAG-only topology policy or invent edges between
adjacent statuses. Start with simple placement rather than an automatic graph
layout engine. Pointer dragging repositions nodes; canvas controls pan, zoom and
fit the graph. Dragging uses world coordinates independent of viewport scale;
Escape or pointer cancellation restores the previous automatic or manual placement.
Background dragging and focused-canvas arrow keys pan; +/- controls zoom around
the selected element through the same zoom command as toolbar buttons.
Fit/Home use content bounds of nodes, routes and labels, independent of the empty
surface minimum, together with current canvas dimensions. A bounded viewport
keeps the stacked inspector reachable. Open properties and Back to canvas provide
explicit focus transfer; focusing a clipped element pans it into view and reduces
scale if needed. Pointer focus does not invoke automatic camera reveal, so it
cannot shift a captured drag origin. Gesture completion and cancellation check the
owning pointer ID. Labels use the existing English/Russian presentation catalog.
Navigation must not change the policy signature, invalidate a
validation receipt or send layout fields through REST. Reload reconstructs layout;
this is deliberate because layout persistence is outside the existing contract.

### 4. Retain the current publication boundary

Keep `WorkflowEditor` validation, stale-baseline handling, frozen confirmed intent,
version headers, replay keys and query invalidation. Keep textual review as an
accessible summary derived from the same definition, with actual transitions
distinct from display order. `WorkflowCatalog` retains exact-version reads and
separate default selection. Show validation failures through existing feedback,
associating them with inspector fields only when the current response supports it.
No schema, endpoint, database or provider change is needed.

## Risks / Trade-offs

- Dense or cyclic graphs can overlap: use distinguishable directed paths, movable
  nodes and fit/zoom; keep the supported generated bounds instead of adding a
  new performance project or layout engine.
- Drag-only controls exclude keyboard users: provide named selectable elements
  and the same inspector's Add/connect/delete controls with visible focus.
- Replacing forms can bypass review or lose settings: integrate with the current
  editor state and adapt its behavioral tests instead of creating another submit
  path. English/Russian, themes and narrow layout use existing presentation rules.

## Migration Plan

Deploy as a normal Console update against the same REST API. Published definitions,
selected defaults and Issue pins require no migration. Reverting the UI restores
the preceding editor without transforming server data. Update workflow guidance
and only the visual baselines actually affected by this UI change.
