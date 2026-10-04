export interface DagLayoutInput {
  edges: ReadonlyArray<{ from: string; to: string }>;
  nodes: ReadonlyArray<{ id: string; sortKey: string }>;
}

export interface DagLayout {
  height: number;
  nodeHeight: number;
  nodes: Array<{ id: string; x: number; y: number }>;
  nodeWidth: number;
  width: number;
}

const NODE_WIDTH = 168;
const NODE_HEIGHT = 68;
const LAYER_GAP = 92;
const ROW_GAP = 28;
const GRAPH_PADDING = 24;

/** Places a bounded DAG deterministically without exposing topology policy to the SVG renderer. */
export function layoutDag({ edges, nodes }: DagLayoutInput): DagLayout {
  const sortedNodes = [...nodes].sort(compareNodes);
  const nodeIds = new Set(sortedNodes.map(({ id }) => id));
  const incoming = new Map(sortedNodes.map(({ id }) => [id, 0]));
  const outgoing = new Map(sortedNodes.map(({ id }) => [id, [] as string[]]));
  const layers = new Map(sortedNodes.map(({ id }) => [id, 0]));

  for (const edge of edges) {
    if (!nodeIds.has(edge.from) || !nodeIds.has(edge.to)) continue;
    incoming.set(edge.to, (incoming.get(edge.to) ?? 0) + 1);
    outgoing.get(edge.from)?.push(edge.to);
  }

  const ready = sortedNodes.filter(({ id }) => incoming.get(id) === 0).map(({ id }) => id);
  const visited = new Set<string>();
  while (ready.length > 0) {
    ready.sort();
    const nodeId = ready.shift();
    if (nodeId === undefined) break;
    visited.add(nodeId);
    for (const dependentId of (outgoing.get(nodeId) ?? []).sort()) {
      layers.set(
        dependentId,
        Math.max(layers.get(dependentId) ?? 0, (layers.get(nodeId) ?? 0) + 1),
      );
      const remainingIncoming = (incoming.get(dependentId) ?? 1) - 1;
      incoming.set(dependentId, remainingIncoming);
      if (remainingIncoming === 0) ready.push(dependentId);
    }
  }

  // The server guarantees a DAG; malformed nodes remain visible in one fallback layer.
  const fallbackLayer = Math.max(0, ...layers.values()) + 1;
  for (const node of sortedNodes) {
    if (!visited.has(node.id)) layers.set(node.id, fallbackLayer);
  }

  const grouped = new Map<number, typeof sortedNodes>();
  for (const node of sortedNodes) {
    const layer = layers.get(node.id) ?? 0;
    grouped.set(layer, [...(grouped.get(layer) ?? []), node]);
  }
  const layerNumbers = [...grouped.keys()].sort((left, right) => left - right);
  const positioned = layerNumbers.flatMap((layer) =>
    (grouped.get(layer) ?? []).map(({ id }, row) => ({
      id,
      x: GRAPH_PADDING + layer * (NODE_WIDTH + LAYER_GAP),
      y: GRAPH_PADDING + row * (NODE_HEIGHT + ROW_GAP),
    })),
  );
  const largestLayer = Math.max(1, ...[...grouped.values()].map((layer) => layer.length));
  return {
    height: GRAPH_PADDING * 2 + largestLayer * NODE_HEIGHT + (largestLayer - 1) * ROW_GAP,
    nodeHeight: NODE_HEIGHT,
    nodes: positioned,
    nodeWidth: NODE_WIDTH,
    width:
      GRAPH_PADDING * 2 +
      Math.max(1, layerNumbers.length) * NODE_WIDTH +
      Math.max(0, layerNumbers.length - 1) * LAYER_GAP,
  };
}

function compareNodes(
  left: DagLayoutInput['nodes'][number],
  right: DagLayoutInput['nodes'][number],
): number {
  return compareText(left.sortKey, right.sortKey) || compareText(left.id, right.id);
}

function compareText(left: string, right: string): number {
  if (left < right) return -1;
  if (left > right) return 1;
  return 0;
}
