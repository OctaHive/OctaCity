import { useId, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import { directedGraph, NODE_WIDTH, NODE_HEIGHT } from '../../shared/directedGraph';
import { useGraphNodePositions } from '../../shared/useGraphNodePositions';
import { useGraphViewport } from '../../shared/useGraphViewport';
import canvasStyles from '../../shared/GraphCanvas.module.css';
import styles from './FlowCanvas.module.css';
import { FlowRouteFields } from './FlowTransitions';
import { FlowConnection } from './FlowConnection';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';

type Root = components['schemas']['FactoryFlowRootDraft'];
/** Display topology is separate from the immutable Flow; inspector selection has one parent owner. */
export function FlowCanvas({
  value,
  selected,
  onSelect,
  onAddStep,
  repeatCeiling,
  onChange,
}: {
  onAddStep: () => void;
  value: Root;
  selected: number;
  onSelect: (index: number) => void;
  repeatCeiling: number;
  onChange: (value: Root) => void;
}) {
  const { t } = usePresentation();
  const markerId = useId();
  const [connection, setConnection] = useState<{
    source: string;
    target: Root['transitions'][number]['target'] | null;
  } | null>(null);
  const keys = useDraftRowKeys(value.nodes);
  const routeKeys = useDraftRowKeys(value.transitions);
  const terminalKeys = useDraftRowKeys(value.terminals);
  const [routeSelection, setRouteSelection] = useState<string | null>(null);
  const selectedRoute = routeKeys.findIndex((key) => key === routeSelection);
  const nodes = [
    ...keys.map((id) => ({ id })),
    ...value.terminals.map((_, index) => ({ id: `terminal:${index}` })),
  ];
  const nodeId = (key: string) => keys[value.nodes.findIndex((node) => node.key === key)];
  const edges = value.transitions.flatMap((edge, index) => {
    const from = nodeId(edge.predecessor);
    const to =
      edge.target.kind === 'node'
        ? nodeId(edge.target.key)
        : value.terminals.some((terminal) => terminal.key === edge.target.key)
          ? `terminal:${value.terminals.findIndex((terminal) => terminal.key === edge.target.key)}`
          : undefined;
    return from && to ? [{ id: String(index), from, to }] : [];
  });
  const routeProblems = value.transitions.flatMap((edge, index) => {
    const source = value.nodes.find((node) => node.key === edge.predecessor);
    const messages: string[] = [];
    if (!source) messages.push(t('factory.graphMissingSource', { name: edge.predecessor }));
    else if (!source.outcomes.some((outcome) => outcome.key === edge.outcome))
      messages.push(
        t('factory.graphMissingOutcome', { name: edge.predecessor, outcome: edge.outcome }),
      );
    if (
      !(edge.target.kind === 'node' ? value.nodes : value.terminals).some(
        (item) => item.key === edge.target.key,
      )
    )
      messages.push(t('factory.graphMissingTarget', { name: edge.target.key }));
    return messages.length ? [{ index, edge, messages }] : [];
  });
  const placement = useGraphNodePositions();
  const graph = directedGraph(
    { nodes, entry: nodeId(value.entry) ?? '', edges },
    placement.positions,
  );
  const viewport = useGraphViewport(graph.contentBounds);
  return (
    <section aria-label={t('factory.stepGraph')} className={styles.graph}>
      {routeProblems.length ? (
        <ul role="alert" aria-label={t('factory.graphRouteProblems')}>
          {routeProblems.map((problem) => (
            <li key={routeKeys[problem.index]}>
              {problem.messages.map((message) => (
                <p key={message}>{message}</p>
              ))}
              <button type="button" onClick={() => setRouteSelection(routeKeys[problem.index]!)}>
                {t('factory.editGraphRoute', {
                  from: problem.edge.predecessor,
                  to: problem.edge.target.key,
                  outcome: problem.edge.outcome,
                })}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      <div className={styles.toolbar}>
        <button type="button" disabled={!value.nodes.length} onClick={viewport.fit}>
          {t('workflow.fitGraph')}
        </button>
        <button type="button" disabled={!viewport.canZoomIn} onClick={() => viewport.zoom(1)}>
          {t('workflow.zoomIn')}
        </button>
        <button type="button" disabled={!viewport.canZoomOut} onClick={() => viewport.zoom(-1)}>
          {t('workflow.zoomOut')}
        </button>
        <output aria-label={t('workflow.zoomLevel')}>{Math.round(viewport.scale * 100)}%</output>

        <button
          type="button"
          disabled={
            value.nodes.length >=
            FACTORY_AUTHORING_SCHEMAS.FactoryFlowRootDraft.properties.nodes.maxItems
          }
          onClick={onAddStep}
        >
          {t('factory.addNode')}
        </button>
        <SelectMenu
          ariaLabel={t('factory.graphNode')}
          value={String(selected)}
          options={value.nodes.map((node, index) => ({
            value: String(index),
            label: node.key || t('factory.unnamedNode'),
          }))}
          onValueChange={(value) => onSelect(Number(value))}
        />
      </div>
      {connection ? (
        <FlowConnection
          key={connection.source}
          root={value}
          source={keys.indexOf(connection.source)}
          target={connection.target}
          onTarget={(target) => setConnection({ ...connection, target })}
          onCancel={() => setConnection(null)}
          onAdd={(edge) => {
            onChange({ ...value, transitions: [...value.transitions, edge] });
            setConnection(null);
          }}
        />
      ) : null}
      {selectedRoute >= 0 && value.transitions[selectedRoute] ? (
        <fieldset>
          <legend>{t('factory.selectedGraphRoute')}</legend>
          <FlowRouteFields
            value={value}
            edge={value.transitions[selectedRoute]}
            repeatCeiling={repeatCeiling}
            onChange={(patch) =>
              onChange({
                ...value,
                transitions: value.transitions.map((edge, index) =>
                  index === selectedRoute ? { ...edge, ...patch } : edge,
                ),
              })
            }
            onRemove={() => {
              onChange({
                ...value,
                transitions: value.transitions.filter((_, index) => index !== selectedRoute),
              });
              setRouteSelection(null);
            }}
          />
        </fieldset>
      ) : null}
      <div
        {...viewport.props}
        role="region"
        className={canvasStyles.canvas}
        aria-label={t('factory.graph')}
        onPointerMove={(event) => {
          viewport.props.onPointerMove(event);
          placement.move(event, viewport.scale, graph.nodes);
        }}
        onPointerUp={(event) => {
          viewport.props.onPointerUp(event);
          placement.finish(event);
        }}
        onPointerCancel={(event) => {
          viewport.props.onPointerCancel(event);
          placement.cancelPointer(event);
        }}
        onLostPointerCapture={(event) => {
          viewport.props.onLostPointerCapture(event);
          placement.cancelPointer(event);
        }}
        onKeyDown={(event) => {
          if (event.key === 'Escape' && placement.isMoving()) {
            event.preventDefault();
            placement.cancel();
          } else viewport.props.onKeyDown(event);
        }}
      >
        <div
          className={canvasStyles.surface}
          style={{ ...viewport.style, width: graph.width, height: graph.height }}
        >
          <svg role="img" aria-label={t('factory.graph')} width={graph.width} height={graph.height}>
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
                <path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
              </marker>
            </defs>
            {edges.map((edge) => {
              const route = graph.edges.get(edge.id)!;
              return (
                <path
                  key={edge.id}
                  data-edge-kind="control"
                  d={route.path}
                  markerEnd={`url(#${markerId})`}
                  fill="none"
                  stroke="currentColor"
                  onClick={() => setRouteSelection(routeKeys[Number(edge.id)]!)}
                />
              );
            })}
          </svg>
          {edges.map((edge) => {
            const index = Number(edge.id);
            const route = value.transitions[index]!;
            const label = graph.edges.get(edge.id)!.label;
            return (
              <button
                key={edge.id}
                type="button"
                className={styles.routeLabel}
                style={{ left: label.x, top: label.y, width: label.width }}
                aria-label={t('factory.editGraphRoute', {
                  from: route.predecessor,
                  to: route.target.key,
                  outcome: route.outcome,
                })}
                onClick={() => {
                  setConnection(null);
                  setRouteSelection(routeKeys[index]!);
                }}
              >
                {route.outcome}
              </button>
            );
          })}
          {value.nodes.map((node, index) => {
            const point = graph.nodes.get(keys[index]!)!;
            return (
              <article
                key={keys[index]}
                data-graph-node
                className={`${canvasStyles.node} ${selected === index ? styles.selected : ''}`}
                style={{ left: point.x, top: point.y, width: NODE_WIDTH, height: NODE_HEIGHT }}
              >
                <button
                  type="button"
                  aria-label={t('factory.editGraphStep', {
                    name: node.key || t('factory.unnamedNode'),
                  })}
                  onPointerDown={(event) =>
                    placement.begin(event, keys[index]!, graph.nodes.get(keys[index]!)!)
                  }
                  title={t('factory.moveGraphStepHint')}
                  onKeyDown={(event) => {
                    if (!event.altKey || event.ctrlKey || event.metaKey) return;
                    const offsets = {
                      ArrowLeft: { x: -16, y: 0 },
                      ArrowRight: { x: 16, y: 0 },
                      ArrowUp: { x: 0, y: -16 },
                      ArrowDown: { x: 0, y: 16 },
                    };
                    if (!Object.hasOwn(offsets, event.key)) return;
                    event.preventDefault();
                    placement.shift(
                      keys[index]!,
                      offsets[event.key as keyof typeof offsets],
                      graph.nodes,
                    );
                  }}
                  data-factory-node-index={index}
                  aria-pressed={selected === index}
                  onClick={() => {
                    if (connection && keys[index] !== connection.source)
                      setConnection({ ...connection, target: { kind: 'node', key: node.key } });
                    else onSelect(index);
                  }}
                >
                  {node.key || t('factory.unnamedNode')}
                </button>
                <button
                  type="button"
                  disabled={
                    !node.key ||
                    !node.outcomes.length ||
                    value.transitions.length >=
                      FACTORY_AUTHORING_SCHEMAS.FactoryFlowRootDraft.properties.transitions.maxItems
                  }
                  onClick={() => setConnection({ source: keys[index]!, target: null })}
                >
                  {t('factory.connectGraphStep', { name: node.key || t('factory.unnamedNode') })}
                </button>
              </article>
            );
          })}
          {value.terminals.map((terminal, index) => {
            const point = graph.nodes.get(`terminal:${index}`)!;
            return (
              <div
                key={terminalKeys[index]}
                className={`${canvasStyles.node} ${styles.terminal}`}
                style={{ left: point.x, top: point.y, width: NODE_WIDTH, height: NODE_HEIGHT }}
              >
                <strong>{terminal.key}</strong>
                <small>{t('factory.terminals')}</small>
              </div>
            );
          })}
        </div>
      </div>
      {!value.nodes.length ? <p>{t('factory.emptyStepGraph')}</p> : null}
    </section>
  );
}
