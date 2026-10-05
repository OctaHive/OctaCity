import { useCallback, useState } from 'react';

import { usePresentation } from '../app/presentation/PresentationProvider';
import type { PreferenceResourceKind } from '../app/presentation/preferences';

/** Owns explicit expansion choices while allowing a selected path to supply its default state. */
export function useExpansionOverrides(kind: PreferenceResourceKind) {
  const { preferences, setExpanded } = usePresentation();
  const [overrides, setOverrides] = useState<ReadonlyMap<string, boolean>>(() => new Map());
  const isExpanded = useCallback(
    (id: string, defaultExpanded = false) => {
      const persisted = preferences.expanded.some(
        (identity) => identity.kind === kind && identity.id === id,
      );
      return overrides.get(id) ?? (persisted || defaultExpanded);
    },
    [kind, overrides, preferences.expanded],
  );
  const toggle = useCallback(
    (id: string, currentlyExpanded: boolean) => {
      const expanded = !currentlyExpanded;
      setOverrides((current) => {
        const next = new Map(current);
        next.set(id, expanded);
        return next;
      });
      setExpanded({ id, kind }, expanded);
    },
    [kind, setExpanded],
  );

  return { isExpanded, toggle };
}
