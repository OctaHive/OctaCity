import type { ConnectionIntent } from './api';

/** Immutable references compare their contract fields, independent of JSON key order. */
export function sameReference(
  left: ConnectionIntent['draft']['installation'],
  right: ConnectionIntent['draft']['installation'] | undefined,
): boolean {
  return (
    right !== undefined &&
    left.identity === right.identity &&
    left.version === right.version &&
    left.digest === right.digest
  );
}
