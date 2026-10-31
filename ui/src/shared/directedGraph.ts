export const NODE_WIDTH = 224;
export const NODE_HEIGHT = 104;
export const EDGE_LABEL_HEIGHT = 24;
const LABEL_GAP = 8;
const COLUMN_GAP = 112;
const ROW_GAP = 64;
const MARGIN = 48;
const LANE_GAP = 40;
const CLEARANCE = 32;
const CORNER_RADIUS = 12;
export type GraphPoint = { x: number; y: number };
export type GraphBounds = GraphPoint & { width: number; height: number };

/** Local display geometry only: never reorder or modify the execution policy. */
export function directedGraph(
  definition: {
    nodes: readonly { id: string }[];
    entry: string;
    edges: readonly { id: string; from: string; to: string }[];
  },
  positions: ReadonlyMap<string, GraphPoint> = new Map(),
) {
  const outgoing = new Map<string, string[]>(definition.nodes.map((status) => [status.id, []]));
  for (const edge of definition.edges) outgoing.get(edge.from)?.push(edge.to);
  const columns = new Map<string, number>();
  // Breadth-first placement keeps branches in adjacent columns. Visiting each node
  // once also terminates cycles; return edges are still drawn in exterior lanes.
  for (const root of [definition.entry, ...outgoing.keys()]) {
    if (!outgoing.has(root) || columns.has(root)) continue;
    columns.set(root, 0);
    const queue = [root];
    for (let index = 0; index < queue.length; index++) {
      const id = queue[index]!;
      for (const target of outgoing.get(id) ?? []) {
        if (columns.has(target)) continue;
        columns.set(target, columns.get(id)! + 1);
        queue.push(target);
      }
    }
  }
  const pairs = new Set<string>();
  const routes = definition.edges.map((edge) => {
    const from = columns.get(edge.from)!;
    const to = columns.get(edge.to)!;
    const pair = `${edge.from}:${edge.to}`;
    const kind = to === from + 1 && !pairs.has(pair) ? 'between' : to > from ? 'above' : 'below';
    pairs.add(pair);
    return { edge, kind };
  });
  const aboveCount = routes.filter((route) => route.kind === 'above').length;
  const rows = new Map<number, number>();
  const nodes = new Map<string, GraphPoint>();
  for (const [id, column] of columns) {
    const row = rows.get(column) ?? 0;
    rows.set(column, row + 1);
    nodes.set(id, {
      x: Math.max(MARGIN, positions.get(id)?.x ?? MARGIN + column * (NODE_WIDTH + COLUMN_GAP)),
      y: Math.max(
        MARGIN + aboveCount * LANE_GAP,
        positions.get(id)?.y ?? MARGIN + aboveCount * LANE_GAP + row * (NODE_HEIGHT + ROW_GAP),
      ),
    });
  }
  const bottom = Math.max(
    MARGIN + NODE_HEIGHT,
    ...[...nodes.values()].map((point) => point.y + NODE_HEIGHT),
  );
  const contentPoints = [...nodes.values()].flatMap((point) => [
    point,
    { x: point.x + NODE_WIDTH, y: point.y + NODE_HEIGHT },
  ]);
  let aboveLane = 0;
  let belowLane = 0;
  const edges = new Map<string, { path: string; label: GraphPoint & { width: number } }>();
  for (const { edge, kind: initialKind } of routes) {
    const from = nodes.get(edge.from)!;
    const to = nodes.get(edge.to)!;
    const start = { x: from.x + NODE_WIDTH, y: from.y + NODE_HEIGHT / 2 };
    const end = { x: to.x, y: to.y + NODE_HEIGHT / 2 };
    const kind =
      initialKind === 'between' && end.x - start.x < 2 * CLEARANCE ? 'below' : initialKind;
    const middleX = (start.x + end.x) / 2;
    const laneY =
      kind === 'above'
        ? MARGIN + aboveLane++ * LANE_GAP
        : kind === 'below'
          ? bottom + MARGIN + belowLane++ * LANE_GAP
          : 0;
    const points =
      kind === 'between'
        ? [start, { x: middleX, y: start.y }, { x: middleX, y: end.y }, end]
        : [
            start,
            { x: start.x + CLEARANCE, y: start.y },
            { x: start.x + CLEARANCE, y: laneY },
            { x: end.x - CLEARANCE, y: laneY },
            { x: end.x - CLEARANCE, y: end.y },
            end,
          ];
    const label = {
      x: middleX,
      y: (kind === 'between' ? (start.y + end.y) / 2 : laneY) - EDGE_LABEL_HEIGHT / 2 - LABEL_GAP,
      width:
        kind === 'between'
          ? end.x - start.x - 2 * LABEL_GAP
          : Math.max(NODE_WIDTH, Math.abs(end.x - start.x) - 2 * CLEARANCE),
    };
    edges.set(edge.id, { path: roundedPath(points), label });
    contentPoints.push(
      ...points,
      { x: label.x - label.width / 2, y: label.y - EDGE_LABEL_HEIGHT / 2 },
      { x: label.x + label.width / 2, y: label.y + EDGE_LABEL_HEIGHT / 2 },
    );
  }
  // Content bounds exclude empty surface padding; include exterior routes and labels for Fit.
  const xs = contentPoints.map((point) => point.x);
  const ys = contentPoints.map((point) => point.y);
  const x = xs.length ? Math.min(...xs) : 0;
  const y = ys.length ? Math.min(...ys) : 0;
  const contentBounds: GraphBounds = {
    x,
    y,
    width: xs.length ? Math.max(...xs) - x : 0,
    height: ys.length ? Math.max(...ys) - y : 0,
  };
  return {
    contentBounds,
    nodes,
    edges,
    width: Math.max(640, ...[...nodes.values()].map((point) => point.x + NODE_WIDTH + MARGIN)),
    height: Math.max(300, bottom + MARGIN + belowLane * LANE_GAP),
  };
}

/** Round orthogonal corners while retaining straight, centered arrow approaches. */
function roundedPath(points: GraphPoint[]) {
  const distinct = points.filter(
    (point, index) =>
      index === 0 || point.x !== points[index - 1]!.x || point.y !== points[index - 1]!.y,
  );
  let path = `M ${distinct[0]!.x} ${distinct[0]!.y}`;
  for (let index = 1; index < distinct.length - 1; index++) {
    const previous = distinct[index - 1]!;
    const corner = distinct[index]!;
    const next = distinct[index + 1]!;
    const incoming = Math.hypot(corner.x - previous.x, corner.y - previous.y);
    const outgoing = Math.hypot(next.x - corner.x, next.y - corner.y);
    const radius = Math.min(CORNER_RADIUS, incoming / 2, outgoing / 2);
    path += ` L ${corner.x - ((corner.x - previous.x) / incoming) * radius} ${corner.y - ((corner.y - previous.y) / incoming) * radius} Q ${corner.x} ${corner.y} ${corner.x + ((next.x - corner.x) / outgoing) * radius} ${corner.y + ((next.y - corner.y) / outgoing) * radius}`;
  }
  return `${path} L ${distinct.at(-1)!.x} ${distinct.at(-1)!.y}`;
}
