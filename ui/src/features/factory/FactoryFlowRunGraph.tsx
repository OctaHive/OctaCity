import { useId } from 'react';
import { Link } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { FactoryFlowRun } from './FactoryRunCommands';
import styles from './Factory.module.css';

/** Only loaded journal relationships are drawn; the table preserves complete identities. */
export function FactoryFlowRunGraph({
  getFlowHref,
  flows,
}: {
  getFlowHref: (flow: FactoryFlowRun) => string;
  flows: readonly FactoryFlowRun[];
}) {
  const { t } = usePresentation();
  const markerId = useId();
  const byId = new Map(flows.map((flow) => [flow.id, flow]));
  const children = new Map<string, FactoryFlowRun[]>();
  for (const flow of flows) {
    if (flow.parent && byId.has(flow.parent.flow_run_id)) {
      const siblings = children.get(flow.parent.flow_run_id) ?? [];
      siblings.push(flow);
      children.set(flow.parent.flow_run_id, siblings);
    }
  }
  const rows: { flow: FactoryFlowRun; depth: number; x: number; y: number }[] = [];
  const visited = new Set<string>();
  const visit = (flow: FactoryFlowRun, depth: number) => {
    if (visited.has(flow.id)) return;
    visited.add(flow.id);
    rows.push({ flow, depth, x: 40 + depth * 60, y: 40 + rows.length * 100 });
    for (const child of children.get(flow.id) ?? []) visit(child, depth + 1);
  };
  for (const flow of flows) if (!flow.parent || !byId.has(flow.parent.flow_run_id)) visit(flow, 0);
  // Incomplete pages stay renderable; no missing parent or malformed relationship is inferred.
  for (const flow of flows) visit(flow, 0);
  const positions = new Map(rows.map((row) => [row.flow.id, row]));
  const width = 800 + Math.max(0, ...rows.map((row) => row.depth)) * 60;
  return (
    <section className={styles.graphOverview}>
      <p className={styles.supportingText}>{t('factory.runtimeFlowHelp')}</p>
      <svg
        role="img"
        aria-label={t('factory.nestedFlows')}
        viewBox={`0 0 ${width} ${Math.max(120, rows.length * 100 + 40)}`}
      >
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
        {rows.map(({ flow, x, y }) => {
          const parent = flow.parent ? positions.get(flow.parent.flow_run_id) : undefined;
          return (
            <g key={flow.id}>
              {parent ? (
                <path
                  d={`M ${parent.x + 16} ${parent.y + 60} L ${parent.x + 16} ${y + 30} L ${x} ${y + 30}`}
                  markerEnd={`url(#${markerId})`}
                  fill="none"
                />
              ) : null}
              <title>{flow.id}</title>
              <rect x={x} y={y} width="720" height="60" rx="8" />
              <text x={x + 12} y={y + 35}>
                {t(flow.parent ? 'factory.nestedFlow' : 'factory.rootFlow')} · v
                {flow.definition.version}
              </text>
            </g>
          );
        })}
      </svg>
      <table>
        <caption>{t('factory.nestedFlows')}</caption>
        <thead>
          <tr>
            <th>{t('factory.diag.flowRuns')}</th>
            <th>{t('factory.pinnedClosure')}</th>
            <th>{t('factory.parentCall')}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(({ flow }) => (
            <tr key={flow.id}>
              <th scope="row">
                <span>{t(flow.parent ? 'factory.nestedFlow' : 'factory.rootFlow')}</span>
                <br />
                <Link to={getFlowHref(flow)}>{flow.id}</Link>
              </th>
              <td>
                {flow.definition.id} · v{flow.definition.version}
              </td>
              <td>
                {flow.parent ? `${flow.parent.flow_run_id} → ${flow.parent.node_attempt_id}` : '—'}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
