import { useId, useState } from 'react';

/** Presentation-only row identities survive field edits and removal of earlier rows. */
export function useDraftRowKeys<T>(items: readonly T[]): readonly string[] {
  const rowId = useId();
  const [previous, setPrevious] = useState(() => ({
    items,
    keys: items.map((_, index) => `${rowId}:${index}`),
    next: items.length,
  }));
  if (
    previous.items.length === items.length &&
    items.every((item, index) => item === previous.items[index])
  )
    return previous.keys;
  // Scalar edits have no object identity: retain their row positions at equal length.
  if (
    previous.items.length === items.length &&
    items.every((item) => item === null || typeof item !== 'object')
  ) {
    setPrevious({ ...previous, items });
    return previous.keys;
  }
  const claimed = new Set<number>();
  const matches = items.map((item) => {
    const index = previous.items.findIndex(
      (candidate, index) => candidate === item && !claimed.has(index),
    );
    if (index >= 0) claimed.add(index);
    return index;
  });
  let next = previous.next;
  const keys = matches.map((match, index) => {
    if (match >= 0) return previous.keys[match]!;
    if (index < previous.keys.length && !claimed.has(index)) {
      claimed.add(index);
      return previous.keys[index]!;
    }
    return `${rowId}:${next++}`;
  });
  setPrevious({ items, keys, next });
  return keys;
}
