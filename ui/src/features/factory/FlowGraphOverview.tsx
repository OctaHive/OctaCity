import { useDraftRowKeys } from './useDraftRowKeys';
import { useId } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import styles from './Factory.module.css';

type Root = components['schemas']['FactoryFlowRootDraft'];
/** A derived overview; the typed forms below are the editable and accessible graph representation. */
export function FlowGraphOverview({ value }: { value: Root }) {
  const { t } = usePresentation();
  const graphId = useId();
  const vertices = [
    ...value.nodes.map((node) => ({ kind: 'node', key: node.key })),
    ...value.terminals.map((terminal) => ({ kind: 'terminal', key: terminal.key })),
  ];
  const position = (kind: string, key: string) => {
    const index = vertices.findIndex((vertex) => vertex.kind === kind && vertex.key === key);
    return index < 0 ? null : { x: 40 + (index % 2) * 380, y: 40 + Math.floor(index / 2) * 120 };
  };
  const edgeKeys = useDraftRowKeys([
    ...value.transitions,
    ...value.context_projections,
    ...value.data_projections,
  ]);
  const edges = [
    ...value.transitions.map((edge) => ({
      kind: 'control',
      from: position('node', edge.predecessor),
      to: position(edge.target.kind, edge.target.key),
    })),
    ...value.context_projections.map((edge) => ({
      kind: 'context',
      from: position('node', edge.predecessor),
      to: position('node', edge.successor),
    })),
    ...value.data_projections.map((edge) => ({
      kind: 'data',
      from: position('node', edge.predecessor),
      to: position('node', edge.successor),
    })),
  ];
  return (
    <section className={styles.graphOverview}>
      <p>{t('factory.graphLegend')}</p>
      <svg
        role="img"
        aria-label={t('factory.graph')}
        viewBox={`0 0 800 ${Math.max(160, Math.ceil(vertices.length / 2) * 120 + 60)}`}
      >
        <defs>
          <marker
            id={graphId}
            viewBox="0 0 10 10"
            refX="8"
            refY="5"
            markerWidth="6"
            markerHeight="6"
            orient="auto-start-reverse"
          >
            <path d="M 0 0 L 10 5 L 0 10 z" />
          </marker>
        </defs>
        {edges.map((edge, index) =>
          edge.from && edge.to ? (
            <path
              key={edgeKeys[index]}
              data-edge-kind={edge.kind}
              d={`M ${edge.from.x + 160} ${edge.from.y + 60} C ${edge.from.x + 160} ${edge.from.y + 100} ${edge.to.x + 160} ${edge.to.y - 30} ${edge.to.x + 160} ${edge.to.y}`}
              markerEnd={`url(#${graphId})`}
              fill="none"
            />
          ) : null,
        )}
        {vertices.map((vertex, index) => {
          const x = 40 + (index % 2) * 380,
            y = 40 + Math.floor(index / 2) * 120;
          return (
            <g key={`${vertex.kind}:${vertex.key}`}>
              <title>{vertex.key}</title>
              <rect
                x={x}
                y={y}
                width="320"
                height="60"
                rx="8"
                className={
                  vertex.key === value.entry && vertex.kind === 'node'
                    ? styles.entryVertex
                    : undefined
                }
              />
              <text x={x + 12} y={y + 35}>
                {vertex.key.length > 32
                  ? `${vertex.key.slice(0, 29)}…`
                  : vertex.key || t('factory.unnamedNode')}
              </text>
            </g>
          );
        })}
      </svg>
    </section>
  );
}
