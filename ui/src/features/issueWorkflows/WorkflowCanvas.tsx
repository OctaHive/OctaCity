import { useEffect, useId, useRef, useState, type PointerEvent } from 'react';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { WorkflowDefinition } from '../../api/issueWorkflows';
import styles from './WorkflowCanvas.module.css';
import { workflowGraph, NODE_WIDTH, NODE_HEIGHT, EDGE_LABEL_HEIGHT } from './workflowGraph';
import { useGraphNodePositions } from '../../shared/useGraphNodePositions';
import { useGraphViewport } from '../../shared/useGraphViewport';
import { WorkflowInspector, type WorkflowSelection } from './WorkflowInspector';
import { ISSUE_WORKFLOW_CONSTRAINTS as bounds } from '../../../.generated/api/issue-workflow';

export function WorkflowCanvas({
  definition,
  onChange,
}: {
  definition: WorkflowDefinition;
  onChange?: ((definition: WorkflowDefinition) => void) | undefined;
}) {
  const { t } = usePresentation();
  const markerId = useId();
  const inspectorId = useId();
  const inspectorRef = useRef<HTMLElement>(null);
  const [selection, setSelection] = useState<WorkflowSelection | null>(null);
  const [connectionSource, setConnectionSource] = useState<string | null>(null);
  const addTransitionButtonRef = useRef<HTMLButtonElement>(null);
  const [drawing, setDrawing] = useState<{
    from: string;
    pointerId: number;
    x: number;
    y: number;
  } | null>(null);
  const placement = useGraphNodePositions();
  const positions = placement.positions;
  const surfaceRef = useRef<HTMLDivElement>(null);
  const nodeButtonsRef = useRef(new Map<string, HTMLButtonElement>());
  const addStatusButtonRef = useRef<HTMLButtonElement>(null);
  const focusAfterRemovalRef = useRef(false);
  useEffect(() => {
    if (!focusAfterRemovalRef.current) return;
    focusAfterRemovalRef.current = false;
    const node = selection?.kind === 'status' ? nodeButtonsRef.current.get(selection.id) : null;
    (node ?? addStatusButtonRef.current)?.focus();
  }, [definition.statuses, selection]);
  function removeStatus(id: string) {
    if (!onChange) return;
    placement.remove(id);
    const statuses = definition.statuses.filter((item) => item.id !== id);
    onChange({
      ...definition,
      statuses,
      initial_status_id:
        definition.initial_status_id === id
          ? (statuses[0]?.id ?? '')
          : definition.initial_status_id,
      terminal_status_ids: definition.terminal_status_ids.filter((item) => item !== id),
      transitions: definition.transitions.filter((edge) => edge.from !== id && edge.to !== id),
    });
    setSelection(statuses[0] ? { kind: 'status', id: statuses[0].id } : null);
    focusAfterRemovalRef.current = true;
  }
  function removeTransition(id: string) {
    if (!onChange) return;
    const edge = definition.transitions.find((item) => item.id === id);
    if (!edge) return;
    onChange({
      ...definition,
      transitions: definition.transitions.filter((item) => item.id !== id),
    });
    setSelection({ kind: 'status', id: edge.from });
    focusAfterRemovalRef.current = true;
  }
  function connect({ from, to }: { from: string; to: string }) {
    if (
      !onChange ||
      definition.transitions.length >= bounds.maximumTransitions ||
      from === to ||
      !definition.statuses.some((status) => status.id === from) ||
      !definition.statuses.some((status) => status.id === to)
    )
      return;
    const id = crypto.randomUUID();
    onChange({
      ...definition,
      transitions: [
        ...definition.transitions,
        { id, name: '', from, to, origins: ['manual'], evidence: [] },
      ],
    });
    setConnectionSource(null);
    setSelection({ kind: 'transition', id });
  }
  function openConnection(from: string) {
    setSelection(null);
    setConnectionSource(from);
  }
  function select(element: WorkflowSelection) {
    setConnectionSource(null);
    setSelection(element);
  }
  function cancelPointer(event: PointerEvent<HTMLDivElement>) {
    placement.cancelPointer(event);
    if (drawing?.pointerId === event.pointerId) setDrawing(null);
  }
  function cancelConnection() {
    setConnectionSource(null);
    setDrawing(null);
    addTransitionButtonRef.current?.focus();
  }
  const { nodes, edges, width, height, contentBounds } = workflowGraph(definition, positions);
  const selectedNode = selection?.kind === 'status' ? nodes.get(selection.id) : undefined;
  const anchor = selectedNode
    ? { x: selectedNode.x + NODE_WIDTH / 2, y: selectedNode.y + NODE_HEIGHT / 2 }
    : selection?.kind === 'transition'
      ? edges.get(selection.id)?.label
      : undefined;
  const viewport = useGraphViewport(contentBounds, anchor);
  return (
    <div
      className={styles.workspace}
      onKeyDown={(event) => {
        if (!event.defaultPrevented && event.key === 'Escape' && placement.isMoving()) {
          event.preventDefault();
          placement.cancel();
        } else if (
          !event.defaultPrevented &&
          event.key === 'Escape' &&
          (connectionSource || drawing)
        ) {
          event.preventDefault();
          cancelConnection();
        }
      }}
      onPointerMove={(event) => {
        if (placement.move(event, viewport.scale, nodes)) return;
        if (!drawing || event.pointerId !== drawing.pointerId) return;
        const rect = surfaceRef.current!.getBoundingClientRect();
        setDrawing({
          ...drawing,
          x: (event.clientX - rect.left) / viewport.scale,
          y: (event.clientY - rect.top) / viewport.scale,
        });
      }}
      onPointerUp={(event) => {
        placement.finish(event);
        if (!drawing || event.pointerId !== drawing.pointerId) return;
        // Pointer capture keeps a drag alive outside its source; resolve the actual drop card.
        const target = document
          .elementFromPoint(event.clientX, event.clientY)
          ?.closest<HTMLElement>('[data-workflow-status]');
        setDrawing(null);
        if (target && surfaceRef.current?.contains(target)) {
          connect({ from: drawing.from, to: target.dataset.workflowStatus! });
        }
      }}
      onPointerCancel={cancelPointer}
      onLostPointerCapture={cancelPointer}
    >
      {onChange ? (
        <div className={styles.toolbar}>
          <button
            type="button"
            ref={addStatusButtonRef}
            disabled={definition.statuses.length >= bounds.maximumStatuses}
            onClick={() => {
              const id = crypto.randomUUID();
              onChange({
                ...definition,
                statuses: [...definition.statuses, { id, name: '' }],
                initial_status_id:
                  definition.statuses.length === 0 ? id : definition.initial_status_id,
              });
              select({ kind: 'status', id });
            }}
          >
            {t('workflow.addStatus')}
          </button>
          <button
            type="button"
            disabled={
              definition.statuses.length < 2 ||
              definition.transitions.length >= bounds.maximumTransitions
            }
            ref={addTransitionButtonRef}
            onClick={() => {
              openConnection(definition.initial_status_id);
            }}
          >
            {t('workflow.addTransition')}
          </button>
        </div>
      ) : null}
      <div className={styles.toolbar}>
        <button type="button" disabled={!viewport.canZoomOut} onClick={() => viewport.zoom(-1)}>
          {t('workflow.zoomOut')}
        </button>
        <output role="status" aria-label={t('workflow.zoomLevel')}>
          {Math.round(viewport.scale * 100)}%
        </output>
        <button type="button" disabled={!viewport.canZoomIn} onClick={() => viewport.zoom(1)}>
          {t('workflow.zoomIn')}
        </button>
        <button type="button" onClick={viewport.fit}>
          {t('workflow.fitGraph')}
        </button>
        <button
          type="button"
          aria-controls={inspectorId}
          onClick={() => {
            inspectorRef.current?.focus({ preventScroll: true });
            inspectorRef.current?.scrollIntoView({ block: 'nearest' });
          }}
        >
          {t('workflow.openProperties')}
        </button>
      </div>
      <div
        className={styles.canvas}
        {...viewport.props}
        title={t('workflow.navigationHint')}
        role="group"
        aria-label={t('workflow.graph')}
      >
        <div
          className={styles.surface}
          ref={surfaceRef}
          style={{ width, height, ...viewport.style }}
        >
          <svg width={width} height={height} role="group" aria-label={t('workflow.transitions')}>
            <defs>
              <marker
                id={markerId}
                viewBox="0 0 10 10"
                refX="10"
                refY="5"
                markerWidth="10"
                markerHeight="10"
                markerUnits="userSpaceOnUse"
                orient="auto"
              >
                <path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
              </marker>
            </defs>
            {definition.transitions.map((edge, index) => {
              const edgeName = edge.name || t('workflow.transitionNumber', { number: index + 1 });
              const { path, label } = edges.get(edge.id)!;
              const name = (id: string) =>
                definition.statuses.find((status) => status.id === id)!.name;
              return (
                <g
                  key={edge.id}
                  className={styles.edge}
                  role="button"
                  tabIndex={0}
                  aria-pressed={selection?.kind === 'transition' && selection.id === edge.id}
                  aria-label={`${edgeName}: ${name(edge.from)} → ${name(edge.to)}`}
                  onClick={() => select({ kind: 'transition', id: edge.id })}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ' ') {
                      event.preventDefault();
                      select({ kind: 'transition', id: edge.id });
                    }
                  }}
                >
                  <title>{edgeName}</title>
                  <path
                    d={path}
                    markerEnd={`url(#${markerId})`}
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.5"
                  />
                  <path d={path} fill="none" stroke="transparent" strokeWidth="16" />
                  <foreignObject
                    x={label.x - label.width / 2}
                    y={label.y - EDGE_LABEL_HEIGHT / 2}
                    width={label.width}
                    height={EDGE_LABEL_HEIGHT}
                  >
                    <div className={styles.edgeLabel}>{edgeName}</div>
                  </foreignObject>
                </g>
              );
            })}
            {drawing ? (
              <path
                role="img"
                aria-label={t('workflow.drawingConnection')}
                d={`M ${nodes.get(drawing.from)!.x + NODE_WIDTH} ${nodes.get(drawing.from)!.y + NODE_HEIGHT / 2} L ${drawing.x} ${drawing.y}`}
                className={styles.preview}
                fill="none"
                stroke="currentColor"
                strokeDasharray="6 4"
                markerEnd={`url(#${markerId})`}
              />
            ) : null}
          </svg>
          {definition.statuses.map((status) => (
            <article
              key={status.id}
              aria-label={status.name}
              data-graph-node
              data-workflow-status={status.id}
              className={styles.node}
              style={{
                left: nodes.get(status.id)!.x,
                top: nodes.get(status.id)!.y,
                width: NODE_WIDTH,
                height: NODE_HEIGHT,
              }}
            >
              <button
                type="button"
                ref={(element) => {
                  if (element) nodeButtonsRef.current.set(status.id, element);
                  else nodeButtonsRef.current.delete(status.id);
                }}
                className={styles.nodeSelect}
                onPointerDown={(event) => {
                  placement.begin(event, status.id, nodes.get(status.id)!);
                }}
                aria-label={t('workflow.selectStatus', {
                  name: status.name || t('workflow.unnamedStatus'),
                })}
                aria-pressed={selection?.kind === 'status' && selection.id === status.id}
                onClick={() => select({ kind: 'status', id: status.id })}
              />
              {onChange ? (
                <button
                  type="button"
                  className={styles.connectHandle}
                  aria-label={t('workflow.connectFrom', {
                    name: status.name || t('workflow.unnamedStatus'),
                  })}
                  title={t('workflow.connectHint')}
                  disabled={
                    definition.statuses.length < 2 ||
                    definition.transitions.length >= bounds.maximumTransitions
                  }
                  onClick={(event) => {
                    if (event.detail === 0) openConnection(status.id);
                  }}
                  onPointerDown={(event) => {
                    if (!event.isPrimary || event.button !== 0) return;
                    event.currentTarget.setPointerCapture(event.pointerId);
                    const rect = surfaceRef.current!.getBoundingClientRect();
                    setConnectionSource(null);
                    setDrawing({
                      from: status.id,
                      pointerId: event.pointerId,
                      x: (event.clientX - rect.left) / viewport.scale,
                      y: (event.clientY - rect.top) / viewport.scale,
                    });
                  }}
                />
              ) : null}
              {definition.transitions.some((edge) => edge.to === status.id) ? (
                <span className={styles.inputPort} aria-hidden="true" />
              ) : null}
              {!onChange && definition.transitions.some((edge) => edge.from === status.id) ? (
                <span className={styles.outputPort} aria-hidden="true" />
              ) : null}
              <h4 title={status.name}>{status.name || t('workflow.unnamedStatus')}</h4>
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
      <WorkflowInspector
        inspectorRef={inspectorRef}
        inspectorId={inspectorId}
        onReturnToCanvas={() => {
          const canvas = viewport.props.ref.current;
          const selected = canvas?.querySelector<HTMLElement | SVGElement>('[aria-pressed="true"]');
          (selected ?? canvas)?.focus({ preventScroll: true });
          if (selected && !selected.matches(':focus-visible')) viewport.reveal(selected);
          canvas?.scrollIntoView({ block: 'nearest' });
        }}
        definition={definition}
        selection={selection}
        onChange={onChange}
        onRemoveStatus={onChange ? removeStatus : undefined}
        onRemoveTransition={onChange ? removeTransition : undefined}
        connection={
          connectionSource
            ? { from: connectionSource, onConnect: connect, onCancel: cancelConnection }
            : undefined
        }
      />
    </div>
  );
}
