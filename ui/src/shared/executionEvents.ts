/** Shared memory bound for execution event views. Older rows remain independently readable. */
/** Shared bounded backoff for ordinary and Issue-scoped event readers. */
export const EXECUTION_EVENT_RETRY_INITIAL_MILLISECONDS = 500;
export const EXECUTION_EVENT_RETRY_MAX_MILLISECONDS = 8_000;
/** Counter saturation follows the existing delay ceiling rather than a separate retry policy. */
export const EXECUTION_EVENT_BACKOFF_STEPS = Math.ceil(
  Math.log2(EXECUTION_EVENT_RETRY_MAX_MILLISECONDS / EXECUTION_EVENT_RETRY_INITIAL_MILLISECONDS),
);
export const EXECUTION_EVENT_VISIBLE_LIMIT = 512;
export function retainVisibleEvents<T>(
  current: T[],
  appended: T[],
): { discarded: number; events: T[] } {
  const combined = [...current, ...appended];
  const discarded = Math.max(0, combined.length - EXECUTION_EVENT_VISIBLE_LIMIT);
  return { discarded, events: discarded === 0 ? combined : combined.slice(discarded) };
}
