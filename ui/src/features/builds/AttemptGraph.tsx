import type { DagEdge, JobResource } from './api';
import { formatEnumLabel } from '../../shared/display';
import { layoutDag } from './dagLayout';
import styles from './Builds.module.css';

interface AttemptGraphProps {
  edges: DagEdge[];
  jobs: JobResource[];
}

/** Renders a stable, read-only DAG whose semantics are repeated in the adjacent table. */
export function AttemptGraph({ edges, jobs }: AttemptGraphProps) {
  const layout = layoutDag({
    edges: edges.map((edge) => ({
      from: edge.predecessor_job_id,
      to: edge.dependent_job_id,
    })),
    nodes: jobs.map((job) => ({ id: job.id, sortKey: job.pipeline_node_id })),
  });
  const jobsById = new Map(jobs.map((job) => [job.id, job]));
  const positions = new Map(layout.nodes.map((node) => [node.id, node]));

  return (
    <div className={styles.graphScroller}>
      <svg
        aria-label="Attempt dependency graph"
        className={styles.graph}
        height={layout.height}
        role="img"
        viewBox={`0 0 ${layout.width} ${layout.height}`}
        width={layout.width}
      >
        <defs>
          <marker
            id="dependency-arrow"
            markerHeight="7"
            markerWidth="7"
            orient="auto-start-reverse"
            refX="6"
            refY="3.5"
          >
            <path className={styles.arrow} d="M 0 0 L 7 3.5 L 0 7 z" />
          </marker>
        </defs>
        {edges.map((edge) => {
          const predecessor = positions.get(edge.predecessor_job_id);
          const dependent = positions.get(edge.dependent_job_id);
          if (predecessor === undefined || dependent === undefined) {
            return null;
          }
          const startX = predecessor.x + layout.nodeWidth;
          const startY = predecessor.y + layout.nodeHeight / 2;
          const endX = dependent.x;
          const endY = dependent.y + layout.nodeHeight / 2;
          const midpoint = startX + (endX - startX) / 2;
          return (
            <path
              aria-label={edgeLabel(edge, jobs)}
              className={styles.edge}
              d={`M ${startX} ${startY} C ${midpoint} ${startY}, ${midpoint} ${endY}, ${endX} ${endY}`}
              key={`${edge.predecessor_job_id}:${edge.dependent_job_id}`}
              markerEnd="url(#dependency-arrow)"
            />
          );
        })}
        {layout.nodes.map(({ id, x, y }) => {
          const job = jobsById.get(id);
          if (job === undefined) return null;
          return (
            <g aria-label={`${job.pipeline_node_id}, ${stateLabel(job.state)}`} key={job.id}>
              <rect
                className={`${styles.graphNode} ${styles[`state_${stateFamily(job.state)}`]}`}
                height={layout.nodeHeight}
                rx="8"
                width={layout.nodeWidth}
                x={x}
                y={y}
              />
              <text className={styles.nodeTitle} x={x + 12} y={y + 27}>
                {truncate(job.pipeline_node_id)}
              </text>
              <text className={styles.nodeState} x={x + 12} y={y + 49}>
                {stateLabel(job.state)}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

/** Gives both graph and table a single operator-facing relationship description. */
export function edgeLabel(edge: DagEdge, jobs: JobResource[]): string {
  const names = new Map(jobs.map((job) => [job.id, job.pipeline_node_id]));
  return `${names.get(edge.predecessor_job_id) ?? edge.predecessor_job_id} precedes ${names.get(edge.dependent_job_id) ?? edge.dependent_job_id} (${policyLabel(edge.dependency_policy)})`;
}

export function policyLabel(policy: DagEdge['dependency_policy']): string {
  return formatEnumLabel(policy);
}

export function stateLabel(state: JobResource['state']): string {
  return formatEnumLabel(state);
}

export function stateFamily(state: string): 'danger' | 'info' | 'muted' | 'success' {
  if (state === 'failed' || state === 'cancelled') return 'danger';
  if (state === 'succeeded') return 'success';
  if (state === 'skipped') return 'muted';
  return 'info';
}

function truncate(value: string): string {
  return value.length > 20 ? `${value.slice(0, 19)}…` : value;
}
