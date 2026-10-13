import { useId, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import styles from './Factory.module.css';

type Root = components['schemas']['FactoryFlowRootDraft'];
type Connection = 'control' | 'context' | 'data';
type Route = {
  outcome: string;
  target: { kind: string; key: string };
  repeats: number;
  exhausted: string | null;
  detail: string;
};
const short = (value: string, maximum = 30) =>
  value.length > maximum ? `${value.slice(0, maximum - 1)}…` : value;

/** A focused neighbourhood avoids overlapping unrelated routes; the table retains every exact value. */
export function FlowGraphOverview({
  value,
  readOnly = false,
}: {
  value: Root;
  readOnly?: boolean;
}) {
  const { t } = usePresentation();
  const markerId = useId();
  const [selected, setSelected] = useState(value.entry);
  const [connection, setConnection] = useState<Connection>('control');
  const node =
    value.nodes.find((node) => node.key === selected) ??
    value.nodes.find((node) => node.key === value.entry) ??
    value.nodes[0];
  const routes: Route[] =
    connection === 'control'
      ? value.transitions
          .filter((edge) => edge.predecessor === node?.key)
          .map((edge) => ({
            outcome: edge.outcome,
            target: edge.target,
            repeats: edge.max_repeats,
            exhausted: edge.exhausted_terminal,
            detail: '',
          }))
      : connection === 'context'
        ? value.context_projections
            .filter((edge) => edge.predecessor === node?.key)
            .map((edge) => ({
              outcome: edge.outcome,
              target: { kind: 'node', key: edge.successor },
              repeats: 0,
              exhausted: null,
              detail: edge.selections.join(' · '),
            }))
        : value.data_projections
            .filter((edge) => edge.predecessor === node?.key)
            .map((edge) => ({
              outcome: edge.outcome,
              target: { kind: 'node', key: edge.successor },
              repeats: 0,
              exhausted: null,
              detail: `${edge.schema.identity} · ${edge.schema.version}`,
            }));
  const groups = new Map<string, { target: Route['target']; routes: Route[] }>();
  for (const route of routes) {
    const key = `${route.target.kind}:${route.target.key}`;
    const group = groups.get(key) ?? { target: route.target, routes: [] };
    group.routes.push(route);
    groups.set(key, group);
  }
  const height = Math.max(200, groups.size * 120 + 40);
  const sourceY = height / 2;
  return (
    <section className={styles.graphOverview}>
      <div className={styles.graphControls}>
        <SelectMenu
          ariaLabel={t('factory.graphNode')}
          value={node?.key ?? ''}
          onValueChange={setSelected}
          options={value.nodes.map((node) => ({
            value: node.key,
            label: node.key || t('factory.unnamedNode'),
          }))}
        />
        <SelectMenu
          ariaLabel={t('factory.connectionType')}
          value={connection}
          onValueChange={(value) => {
            if (value === 'control' || value === 'context' || value === 'data')
              setConnection(value);
          }}
          options={[
            { value: 'control', label: t('factory.controlConnections') },
            { value: 'context', label: 'Context' },
            { value: 'data', label: t('factory.dataConnections') },
          ]}
        />
      </div>
      <p>{t(readOnly ? 'factory.graphReadOnlyLegend' : 'factory.graphFocusedLegend')}</p>
      <svg role="img" aria-label={t('factory.graph')} viewBox={`0 0 1040 ${height}`}>
        <defs>
          <marker
            id={markerId}
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
        {node ? (
          <g>
            <title>{node.key}</title>
            <rect
              x="20"
              y={sourceY - 32}
              width="280"
              height="64"
              rx="10"
              className={styles.entryVertex}
            />
            <text x="36" y={sourceY - 2}>
              {short(node.key)}
            </text>
            <text className={styles.graphSecondary} x="36" y={sourceY + 20}>
              {node.kind}
            </text>
          </g>
        ) : null}
        {Array.from(groups, ([key, group], index) => {
          const y = 70 + index * 120;
          const outcomes = group.routes.map((route) => route.outcome).join(' · ');
          return (
            <g key={key}>
              <path
                data-edge-kind={connection}
                d={`M 300 ${sourceY} C 465 ${sourceY} 540 ${y} 700 ${y}`}
                markerEnd={`url(#${markerId})`}
                fill="none"
              />
              <rect
                className={styles.edgeLabel}
                x="342"
                y={y - 16}
                width="304"
                height="30"
                rx="4"
              />
              <text x="350" y={y + 5}>
                <title>{outcomes}</title>
                {short(outcomes)}
              </text>
              <title>{group.target.key}</title>
              <rect x="704" y={y - 32} width="316" height="64" rx="10" />
              <text x="720" y={y - 2}>
                {short(group.target.key)}
              </text>
              <text className={styles.graphSecondary} x="720" y={y + 20}>
                {group.target.kind === 'terminal' ? t('factory.terminals') : t('factory.node')}
                {group.routes.some((route) => route.repeats > 0)
                  ? ` · ${t('factory.boundedRepeat')}`
                  : ''}
              </text>
            </g>
          );
        })}
      </svg>
      <table>
        <caption>{t('factory.declaredRoutes')}</caption>
        <thead>
          <tr>
            <th>{t('factory.outcomes')}</th>
            <th>{t('factory.target')}</th>
            <th>{t('factory.repeatBound')}</th>
            <th>{t('factory.routeDetails')}</th>
          </tr>
        </thead>
        <tbody>
          {routes.map((route) => (
            <tr key={`${route.outcome}:${route.target.kind}:${route.target.key}`}>
              <th scope="row">{route.outcome}</th>
              <td>
                {route.target.kind === 'node' ? (
                  <button type="button" onClick={() => setSelected(route.target.key)}>
                    {route.target.key}
                  </button>
                ) : (
                  route.target.key
                )}
              </td>
              <td>{route.repeats}</td>
              <td>
                {route.exhausted
                  ? `${t('factory.exhaustion')}: ${route.exhausted}`
                  : route.detail || '—'}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
