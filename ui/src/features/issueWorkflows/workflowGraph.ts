import type { WorkflowDefinition } from '../../api/issueWorkflows';
import { directedGraph, type GraphPoint } from '../../shared/directedGraph';
export { NODE_WIDTH, NODE_HEIGHT, EDGE_LABEL_HEIGHT } from '../../shared/directedGraph';
export type {
  GraphPoint as WorkflowPoint,
  GraphBounds as WorkflowBounds,
} from '../../shared/directedGraph';

/** Adapts the workflow topology to display geometry without changing its policy. */
export function workflowGraph(
  definition: WorkflowDefinition,
  positions: ReadonlyMap<string, GraphPoint> = new Map(),
) {
  return directedGraph(
    {
      nodes: definition.statuses,
      entry: definition.initial_status_id,
      edges: definition.transitions,
    },
    positions,
  );
}
