import { useId, useState } from 'react';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { WorkflowDefinition } from '../../api/issueWorkflows';
import styles from './WorkflowCanvas.module.css';
import { WorkflowInspector, type WorkflowSelection } from './WorkflowInspector';

const NODE_WIDTH = 208;
const NODE_HEIGHT = 152;
const NODE_GAP = 112;
const MARGIN = 48;

export function WorkflowCanvas({ definition }: { definition: WorkflowDefinition }) {
  const { t } = usePresentation();
  const markerId = useId();
  const [selection, setSelection] = useState<WorkflowSelection | null>(null);
  const width = Math.max(640, definition.statuses.length * (NODE_WIDTH + NODE_GAP));
  const positions = new Map(definition.statuses.map((status, index) => [status.id, index]));
  const forwardCount = definition.transitions.filter(
    (edge) => positions.get(edge.from)! < positions.get(edge.to)!,
  ).length;
  const returnCount = definition.transitions.length - forwardCount;
  const nodeY = MARGIN + forwardCount * 56;
  const height = Math.max(300, nodeY + NODE_HEIGHT + MARGIN + returnCount * 56);
  let forwardLane = 0;
  let returnLane = 0;
  return (
    <div className={styles.workspace}>
      <div className={styles.canvas} role="group" aria-label={t('workflow.graph')}>
        <div className={styles.surface} style={{ width, height }}>
          <svg width={width} height={height} role="group" aria-label={t('workflow.transitions')}>
            <defs>
              <marker
                id={markerId}
                viewBox="0 0 10 10"
                refX="10"
                refY="5"
                markerWidth="7"
                markerHeight="7"
                orient="auto"
              >
                <path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
              </marker>
            </defs>
            {definition.transitions.map((edge) => {
              const from = positions.get(edge.from)!;
              const to = positions.get(edge.to)!;
              const forward = from < to;
              const lane = forward ? forwardLane++ : returnLane++;
              const startX = MARGIN + from * (NODE_WIDTH + NODE_GAP) + NODE_WIDTH / 2 + 24;
              const endX = MARGIN + to * (NODE_WIDTH + NODE_GAP) + NODE_WIDTH / 2 - 24;
              const y = forward ? nodeY : nodeY + NODE_HEIGHT;
              const laneY = y + (forward ? -1 : 1) * (64 + lane * 56);
              const approachY = y + (forward ? -20 : 20);
              const path = `M ${startX} ${y} C ${startX} ${laneY} ${endX} ${laneY} ${endX} ${approachY} L ${endX} ${y}`;
              const name = (id: string) => definition.statuses[positions.get(id)!]!.name;
              return (
                <g
                  key={edge.id}
                  className={styles.edge}
                  role="button"
                  tabIndex={0}
                  aria-pressed={selection?.kind === 'transition' && selection.id === edge.id}
                  aria-label={`${edge.name}: ${name(edge.from)} → ${name(edge.to)}`}
                  onClick={() => setSelection({ kind: 'transition', id: edge.id })}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ' ') {
                      event.preventDefault();
                      setSelection({ kind: 'transition', id: edge.id });
                    }
                  }}
                >
                  <title>{edge.name}</title>
                  <path
                    d={path}
                    markerEnd={`url(#${markerId})`}
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.5"
                  />
                  <path d={path} fill="none" stroke="transparent" strokeWidth="16" />
                  <text x={(startX + endX) / 2} y={y * 0.25 + laneY * 0.75 - 8} textAnchor="middle">
                    {edge.name.length > 28 ? `${edge.name.slice(0, 27)}…` : edge.name}
                  </text>
                </g>
              );
            })}
          </svg>
          {definition.statuses.map((status, index) => (
            <article
              key={status.id}
              aria-label={status.name}
              className={styles.node}
              style={{
                left: MARGIN + index * (NODE_WIDTH + NODE_GAP),
                top: nodeY,
                width: NODE_WIDTH,
                height: NODE_HEIGHT,
              }}
            >
              <button
                type="button"
                className={styles.nodeSelect}
                aria-label={t('workflow.selectStatus', { name: status.name })}
                aria-pressed={selection?.kind === 'status' && selection.id === status.id}
                onClick={() => setSelection({ kind: 'status', id: status.id })}
              />
              <h4 title={status.name}>{status.name}</h4>
              <div className={styles.markers}>
                {definition.initial_status_id === status.id ? (
                  <span className={styles.initial}>{t('workflow.initial')}</span>
                ) : null}
                {definition.terminal_status_ids.includes(status.id) ? (
                  <span className={styles.terminal}>{t('workflow.terminal')}</span>
                ) : null}
              </div>
            </article>
          ))}
        </div>
      </div>
      <WorkflowInspector definition={definition} selection={selection} />
    </div>
  );
}
