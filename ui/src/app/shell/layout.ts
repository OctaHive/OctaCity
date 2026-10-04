import { useSyncExternalStore } from 'react';

export const NARROW_WORKBENCH_MAX_WIDTH_REM = 56;
export const NARROW_WORKBENCH_MEDIA_QUERY = `(max-width: ${NARROW_WORKBENCH_MAX_WIDTH_REM}rem)`;

/** Reports whether the contextual explorer must use its narrow overlay layout. */
export function useNarrowWorkbench(): boolean {
  return useSyncExternalStore(subscribeToNarrowWorkbench, isNarrowWorkbench, () => false);
}

export function isNarrowWorkbench(): boolean {
  return globalThis.matchMedia?.(NARROW_WORKBENCH_MEDIA_QUERY).matches === true;
}

/** Observes layout-boundary transitions through the shell's single media-query contract. */
export function observeNarrowWorkbench(listener: (narrow: boolean) => void): () => void {
  const mediaQuery = globalThis.matchMedia?.(NARROW_WORKBENCH_MEDIA_QUERY);
  if (mediaQuery === undefined) {
    return () => undefined;
  }

  const update = (event: MediaQueryListEvent) => listener(event.matches);
  mediaQuery.addEventListener('change', update);
  return () => mediaQuery.removeEventListener('change', update);
}

function subscribeToNarrowWorkbench(onStoreChange: () => void): () => void {
  return observeNarrowWorkbench(onStoreChange);
}
