import { useEffect, useRef, type RefObject } from 'react';

interface DismissibleLayerOptions {
  active: boolean;
  layerRef: RefObject<HTMLElement | null>;
  onDismiss: () => void;
  triggerRef?: RefObject<HTMLElement | null>;
}

/** Dismisses a non-modal layer when focus or pointer interaction moves outside it. */
export function useDismissibleLayer({
  active,
  layerRef,
  onDismiss,
  triggerRef,
}: DismissibleLayerOptions): void {
  const onDismissRef = useRef(onDismiss);
  onDismissRef.current = onDismiss;

  useEffect(() => {
    if (!active) return;

    const isInside = (target: EventTarget | null) => {
      if (!(target instanceof Node)) return false;
      return (
        layerRef.current?.contains(target) === true ||
        triggerRef?.current?.contains(target) === true
      );
    };
    const dismissOutside = (event: PointerEvent | FocusEvent) => {
      if (!isInside(event.target)) onDismissRef.current();
    };

    document.addEventListener('pointerdown', dismissOutside, true);
    document.addEventListener('focusin', dismissOutside, true);
    return () => {
      document.removeEventListener('pointerdown', dismissOutside, true);
      document.removeEventListener('focusin', dismissOutside, true);
    };
  }, [active, layerRef, triggerRef]);
}
