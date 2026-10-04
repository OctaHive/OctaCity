import { useCallback, useState } from 'react';

/** Owns explicit expansion choices while allowing a selected path to supply its default state. */
export function useExpansionOverrides() {
  const [overrides, setOverrides] = useState<ReadonlyMap<string, boolean>>(() => new Map());
  const isExpanded = useCallback(
    (id: string, defaultExpanded = false) => overrides.get(id) ?? defaultExpanded,
    [overrides],
  );
  const toggle = useCallback((id: string, currentlyExpanded: boolean) => {
    setOverrides((current) => {
      const next = new Map(current);
      next.set(id, !currentlyExpanded);
      return next;
    });
  }, []);

  return { isExpanded, toggle };
}
