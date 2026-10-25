## Why

Project Issue workflows are currently read and edited as lists of statuses and transitions, making branches and return paths difficult to understand. Operators need to inspect and author the actual state graph directly, with the selected element's settings beside it.

## What Changes

- Make a graph canvas the primary presentation for the selected default and catalog workflow versions, including historical versions.
- Create workflows on an empty canvas; add, move and connect status nodes, and select nodes or transition arrows to edit their properties in a right-side inspector.
- Mark initial and terminal statuses, render declared transition names and direction, and support branches and cycles without inventing connections from status order.
- Provide pan, zoom, fit-to-view and keyboard alternatives using the same graph and draft; adapt the inspector to narrow layouts.
- Keep existing REST reads, authoritative validation, immutable publication, default selection and confirmed-command behavior. Preserve provider-free manual task creation.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ui/operator-console`: graphical Project Issue workflow viewing and authoring with a contextual inspector, retaining existing business and publication boundaries.

## Impact

- Primarily `ui/src/features/issueWorkflows/`, its existing tests, English/Russian strings and workflow documentation.
- Reuse `IssueWorkflowsApi`, generated workflow constraints, `WorkflowEditor` publication state and shared confirmation/selection/focus controls.
- Existing Factory/Build SVG views are reference implementations, not targets for migration. No new REST operations, database migrations, server policy engine or graph dependency is planned.
- Verification stays within focused behavioral TDD and existing frontend checks; no separate test infrastructure, benchmark project or cross-product qualification matrix.
