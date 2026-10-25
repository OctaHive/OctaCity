import { useRef, useState, type FocusEvent, type KeyboardEvent, type PointerEvent } from 'react';
import type { WorkflowPoint, WorkflowBounds } from './workflowGraph';

const MIN_ZOOM = 0.005;
const MAX_ZOOM = 2;
const ZOOM_FACTOR = 1.25;
const PAN_STEP = 40;
const FIT_PADDING = 16;

/** Transient camera state; coordinates never enter the workflow definition. */
export function useWorkflowViewport(bounds: WorkflowBounds, anchor?: WorkflowPoint) {
  const ref = useRef<HTMLDivElement>(null);
  const [view, setView] = useState({ x: 0, y: 0, scale: 1 });
  const panRef = useRef<{
    pointerId: number;
    clientX: number;
    clientY: number;
    origin: typeof view;
  } | null>(null);
  function fit() {
    const canvas = ref.current;
    if (!canvas || canvas.clientWidth <= 2 * FIT_PADDING || canvas.clientHeight <= 2 * FIT_PADDING)
      return;
    panRef.current = null;
    const scale = Math.max(
      MIN_ZOOM,
      Math.min(
        1,
        (canvas.clientWidth - 2 * FIT_PADDING) / bounds.width,
        (canvas.clientHeight - 2 * FIT_PADDING) / bounds.height,
      ),
    );
    setView({
      scale,
      x: (canvas.clientWidth - bounds.width * scale) / 2 - bounds.x * scale,
      y: (canvas.clientHeight - bounds.height * scale) / 2 - bounds.y * scale,
    });
  }
  function zoom(direction: 1 | -1) {
    panRef.current = null;
    setView((current) => {
      const scale = Math.max(
        MIN_ZOOM,
        Math.min(MAX_ZOOM, current.scale * ZOOM_FACTOR ** direction),
      );
      const center = anchor
        ? {
            x: current.x + anchor.x * current.scale,
            y: current.y + anchor.y * current.scale,
          }
        : { x: (ref.current?.clientWidth ?? 0) / 2, y: (ref.current?.clientHeight ?? 0) / 2 };
      const ratio = scale / current.scale;
      return {
        x: center.x - (center.x - current.x) * ratio,
        y: center.y - (center.y - current.y) * ratio,
        scale,
      };
    });
  }
  function onPointerDown(event: PointerEvent<HTMLDivElement>) {
    if (
      !event.isPrimary ||
      event.button !== 0 ||
      (event.target instanceof Element &&
        event.target.closest('[data-workflow-status], [role="button"]'))
    )
      return;
    event.currentTarget.focus({ preventScroll: true });
    event.currentTarget.setPointerCapture(event.pointerId);
    panRef.current = {
      pointerId: event.pointerId,
      clientX: event.clientX,
      clientY: event.clientY,
      origin: view,
    };
  }
  function onPointerMove(event: PointerEvent<HTMLDivElement>) {
    const pan = panRef.current;
    if (!pan || event.pointerId !== pan.pointerId) return;
    setView({
      ...pan.origin,
      x: pan.origin.x + event.clientX - pan.clientX,
      y: pan.origin.y + event.clientY - pan.clientY,
    });
  }
  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.target !== event.currentTarget || event.altKey || event.ctrlKey || event.metaKey)
      return;
    const steps: Record<string, WorkflowPoint> = {
      ArrowLeft: { x: PAN_STEP, y: 0 },
      ArrowRight: { x: -PAN_STEP, y: 0 },
      ArrowUp: { x: 0, y: PAN_STEP },
      ArrowDown: { x: 0, y: -PAN_STEP },
    };
    const step = steps[event.key];
    if (step) setView((current) => ({ ...current, x: current.x + step.x, y: current.y + step.y }));
    else if (event.key === 'Home') fit();
    else if (event.key === '+' || event.key === '=') zoom(1);
    else if (event.key === '-') zoom(-1);
    else return;
    event.preventDefault();
  }
  // Reveal an explicitly requested element, shrinking only when panning cannot fit it.
  function reveal(element: Element) {
    const canvas = ref.current;
    if (!canvas) return;
    const outer = canvas.getBoundingClientRect();
    const inner = element.getBoundingClientRect();
    if (
      canvas.clientWidth <= 2 * FIT_PADDING ||
      canvas.clientHeight <= 2 * FIT_PADDING ||
      !inner.width ||
      !inner.height
    )
      return;
    const left = outer.left + canvas.clientLeft + FIT_PADDING;
    const top = outer.top + canvas.clientTop + FIT_PADDING;
    const right = outer.left + canvas.clientLeft + canvas.clientWidth - FIT_PADDING;
    const bottom = outer.top + canvas.clientTop + canvas.clientHeight - FIT_PADDING;
    const factor = Math.min(1, (right - left) / inner.width, (bottom - top) / inner.height);
    if (factor < 1) {
      setView((current) => {
        const ratio = Math.max(MIN_ZOOM, current.scale * factor) / current.scale;
        return {
          scale: current.scale * ratio,
          x:
            canvas.clientWidth / 2 -
            (inner.left - outer.left - canvas.clientLeft + inner.width / 2 - current.x) * ratio,
          y:
            canvas.clientHeight / 2 -
            (inner.top - outer.top - canvas.clientTop + inner.height / 2 - current.y) * ratio,
        };
      });
      return;
    }
    const x = inner.left < left ? left - inner.left : inner.right > right ? right - inner.right : 0;
    const y = inner.top < top ? top - inner.top : inner.bottom > bottom ? bottom - inner.bottom : 0;
    if (x || y) setView((current) => ({ ...current, x: current.x + x, y: current.y + y }));
  }
  // Pointer focus must not change the camera after a gesture records its origin.
  function onFocusCapture(event: FocusEvent<HTMLDivElement>) {
    if (event.target !== event.currentTarget && event.target.matches(':focus-visible'))
      reveal(event.target);
  }
  const finishPan = (event: PointerEvent<HTMLDivElement>) => {
    if (panRef.current?.pointerId === event.pointerId) panRef.current = null;
  };
  return {
    props: {
      ref,
      tabIndex: 0,
      onPointerDown,
      onPointerMove,
      onPointerUp: finishPan,
      onPointerCancel: finishPan,
      onLostPointerCapture: finishPan,
      onKeyDown,
      onFocusCapture,
    },
    fit,
    zoom,
    reveal,
    scale: view.scale,
    canZoomIn: view.scale < MAX_ZOOM,
    canZoomOut: view.scale > MIN_ZOOM,
    style: { transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})` },
  };
}
