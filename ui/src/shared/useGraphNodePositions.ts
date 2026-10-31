import { useRef, useState, type PointerEvent } from 'react';
import type { GraphPoint } from './directedGraph';

/** Pointer placement belongs only to the view. No graph definition, policy or persistence is changed. */
export function useGraphNodePositions() {
  const [positions, setPositions] = useState(() => new Map<string, GraphPoint>());
  const movementRef = useRef<{
    id: string;
    pointerId: number;
    clientX: number;
    clientY: number;
    origin: GraphPoint;
    previous: GraphPoint | undefined;
    element: HTMLButtonElement;
  } | null>(null);
  function begin(event: PointerEvent<HTMLButtonElement>, id: string, origin: GraphPoint) {
    if (!event.isPrimary || event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    movementRef.current = {
      id,
      pointerId: event.pointerId,
      clientX: event.clientX,
      clientY: event.clientY,
      origin,
      previous: positions.get(id),
      element: event.currentTarget,
    };
  }
  function move(event: PointerEvent, scale: number, nodes: ReadonlyMap<string, GraphPoint>) {
    const movement = movementRef.current;
    if (!movement || movement.pointerId !== event.pointerId) return false;
    setPositions((current) =>
      new Map([...current].filter(([id]) => nodes.has(id))).set(movement.id, {
        x: movement.origin.x + (event.clientX - movement.clientX) / scale,
        y: movement.origin.y + (event.clientY - movement.clientY) / scale,
      }),
    );
    return true;
  }
  function cancel() {
    const movement = movementRef.current;
    movementRef.current = null;
    if (!movement) return;
    setPositions((current) => {
      const next = new Map(current);
      if (movement.previous && movement.element.isConnected)
        next.set(movement.id, movement.previous);
      else next.delete(movement.id);
      return next;
    });
    movement.element.focus();
  }
  function finish(event: PointerEvent) {
    if (movementRef.current?.pointerId === event.pointerId) movementRef.current = null;
  }
  function cancelPointer(event: PointerEvent) {
    if (movementRef.current?.pointerId === event.pointerId) cancel();
  }
  function remove(id: string) {
    setPositions((current) => {
      const next = new Map(current);
      next.delete(id);
      return next;
    });
  }
  function shift(id: string, offset: GraphPoint, nodes: ReadonlyMap<string, GraphPoint>) {
    const point = nodes.get(id);
    if (!point) return;
    setPositions((current) =>
      new Map([...current].filter(([key]) => nodes.has(key))).set(id, {
        x: point.x + offset.x,
        y: point.y + offset.y,
      }),
    );
  }
  return {
    positions,
    begin,
    move,
    finish,
    cancel,
    cancelPointer,
    remove,
    shift,
    isMoving: () => movementRef.current !== null,
  };
}
