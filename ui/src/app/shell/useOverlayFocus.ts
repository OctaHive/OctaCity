import { useEffect, type RefObject } from 'react';

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

interface OverlayFocusOptions {
  active?: boolean;
  containerRef: RefObject<HTMLElement | null>;
  initialFocusRef: RefObject<HTMLElement | null>;
  onDismiss: () => void;
  priority?: boolean;
  restoreFocus: () => void;
}

/** Contains keyboard focus in an open overlay and restores it when the overlay closes. */
export function useOverlayFocus({
  active = true,
  containerRef,
  initialFocusRef,
  onDismiss,
  priority = false,
  restoreFocus,
}: OverlayFocusOptions): void {
  useEffect(() => {
    if (!active) {
      return;
    }
    const container = containerRef.current;
    if (container === null) {
      return;
    }

    initialFocusRef.current?.focus();

    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (priority && (event.key === 'Escape' || event.key === 'Tab')) {
        event.stopPropagation();
      }
      if (event.key === 'Escape') {
        event.preventDefault();
        onDismiss();
        return;
      }
      if (event.key !== 'Tab') {
        return;
      }

      const focusable = [...container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)];
      if (focusable.length === 0) {
        event.preventDefault();
        container.focus();
        return;
      }

      const first = focusable[0];
      const last = focusable.at(-1);
      const active = document.activeElement;
      const activeIsFocusable = active instanceof HTMLElement && focusable.includes(active);
      if (event.shiftKey && (active === first || !activeIsFocusable)) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && (active === last || !activeIsFocusable)) {
        event.preventDefault();
        first?.focus();
      }
    };

    if (priority) {
      globalThis.addEventListener('keydown', handleKeyDown, true);
    } else {
      document.addEventListener('keydown', handleKeyDown, true);
    }
    return () => {
      if (priority) {
        globalThis.removeEventListener('keydown', handleKeyDown, true);
      } else {
        document.removeEventListener('keydown', handleKeyDown, true);
      }
      restoreFocus();
    };
  }, [active, containerRef, initialFocusRef, onDismiss, priority, restoreFocus]);
}
