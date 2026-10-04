import { ManagementApiError } from '../../api/client';
import type { JobEventPage, JobEventRequest, JobEventResource, JobResource } from './api';

/** Longest wait accepted by the server's Job-event endpoint. */
export const JOB_EVENT_WAIT_MILLISECONDS = 30_000;
/** Prevents an unexpectedly immediate empty response from creating a hot loop. */
export const JOB_EVENT_EMPTY_PAGE_DELAY_MILLISECONDS = 250;
/** Deterministic first retry delay for a transient transport or server failure. */
export const JOB_EVENT_RETRY_INITIAL_MILLISECONDS = 500;
const JOB_EVENT_RETRY_MAX_MILLISECONDS = 8_000;
/** Bounds recovery from a malformed page that skips the next durable sequence. */
export const JOB_EVENT_SEQUENCE_GAP_RETRY_LIMIT = 3;
/** Keeps long-running Jobs from growing browser memory and the event table without limit. */
export const JOB_EVENT_VISIBLE_LIMIT = 512;

export type JobEventTarget = Pick<JobResource, 'event_cursor' | 'id' | 'state' | 'terminal'>;

export interface JobEventSource {
  getJob(jobId: string, signal?: AbortSignal): Promise<JobEventTarget>;
  getJobEvents(jobId: string, request: JobEventRequest, signal: AbortSignal): Promise<JobEventPage>;
}

export interface JobEventFollowerSnapshot {
  cursor: number;
  discardedEventCount: number;
  error: Error | null;
  events: JobEventResource[];
  job: JobEventTarget;
  phase: 'following' | 'retrying' | 'complete' | 'failed';
  retryDelayMilliseconds: number | null;
}

interface FollowJobEventsOptions {
  initialJob: JobEventTarget;
  onSnapshot: (snapshot: JobEventFollowerSnapshot) => void;
  signal: AbortSignal;
  source: JobEventSource;
}

/**
 * Follows one Job sequentially until it is terminal and its durable event cursor
 * has been consumed. The caller owns cancellation, so a route or Job change can
 * stop the in-flight bounded wait without leaving another poller behind.
 */
export async function followJobEvents({
  initialJob,
  onSnapshot,
  signal,
  source,
}: FollowJobEventsOptions): Promise<void> {
  let job = initialJob;
  let snapshot: JobEventFollowerSnapshot = {
    cursor: 0,
    discardedEventCount: 0,
    error: null,
    events: [],
    job,
    phase: 'following',
    retryDelayMilliseconds: null,
  };
  let retryCount = 0;
  let sequenceGapCount = 0;
  publish();

  while (!signal.aborted) {
    if (isTerminal(job.state) && snapshot.cursor >= job.event_cursor) {
      snapshot = { ...snapshot, error: null, phase: 'complete', retryDelayMilliseconds: null };
      publish();
      return;
    }

    try {
      const page = await source.getJobEvents(
        job.id,
        {
          afterSequence: snapshot.cursor,
          waitMilliseconds: isTerminal(job.state) ? 0 : JOB_EVENT_WAIT_MILLISECONDS,
        },
        signal,
      );
      if (signal.aborted) return;

      const appended = contiguousEvents(page.items, snapshot.cursor);
      if (appended.length > 0) {
        const retained = retainVisibleEvents(snapshot.events, appended);
        snapshot = {
          cursor: appended.at(-1)?.sequence ?? snapshot.cursor,
          discardedEventCount: snapshot.discardedEventCount + retained.discarded,
          error: null,
          events: retained.events,
          job,
          phase: 'following',
          retryDelayMilliseconds: null,
        };
        sequenceGapCount = 0;
        publish();
      } else if (hasSequenceGap(page.items, snapshot.cursor)) {
        sequenceGapCount += 1;
        if (sequenceGapCount >= JOB_EVENT_SEQUENCE_GAP_RETRY_LIMIT) {
          snapshot = {
            ...snapshot,
            error: new Error('Job event sequence is not contiguous'),
            phase: 'failed',
            retryDelayMilliseconds: null,
          };
          publish();
          return;
        }
      } else {
        sequenceGapCount = 0;
      }

      job = await source.getJob(job.id, signal);
      if (signal.aborted) return;
      snapshot = { ...snapshot, job };
      publish();
      retryCount = 0;

      if (isTerminal(job.state) && snapshot.cursor >= job.event_cursor) {
        snapshot = { ...snapshot, error: null, phase: 'complete', retryDelayMilliseconds: null };
        publish();
        return;
      }
      if (appended.length === 0) {
        const elapsed = await abortableDelay(JOB_EVENT_EMPTY_PAGE_DELAY_MILLISECONDS, signal);
        if (!elapsed) return;
      }
    } catch (error) {
      if (signal.aborted || isAbortError(error)) return;
      const failure = asError(error);
      if (!isRetryable(failure)) {
        snapshot = {
          ...snapshot,
          error: failure,
          phase: 'failed',
          retryDelayMilliseconds: null,
        };
        publish();
        return;
      }

      const retryDelayMilliseconds = retryDelay(failure, retryCount);
      retryCount += 1;
      snapshot = {
        ...snapshot,
        error: failure,
        phase: 'retrying',
        retryDelayMilliseconds,
      };
      publish();
      const elapsed = await abortableDelay(retryDelayMilliseconds, signal);
      if (!elapsed) return;
      snapshot = {
        ...snapshot,
        error: null,
        phase: 'following',
        retryDelayMilliseconds: null,
      };
      publish();
    }
  }

  function publish() {
    if (!signal.aborted) onSnapshot(snapshot);
  }
}

function retainVisibleEvents(
  current: JobEventResource[],
  appended: JobEventResource[],
): { discarded: number; events: JobEventResource[] } {
  const combined = [...current, ...appended];
  const discarded = Math.max(0, combined.length - JOB_EVENT_VISIBLE_LIMIT);
  return { discarded, events: discarded === 0 ? combined : combined.slice(discarded) };
}

function hasSequenceGap(items: JobEventResource[], cursor: number): boolean {
  let expected = cursor + 1;
  for (const event of items) {
    if (event.sequence < expected) continue;
    if (event.sequence > expected) return true;
    expected += 1;
  }
  return false;
}

function contiguousEvents(items: JobEventResource[], cursor: number): JobEventResource[] {
  const accepted: JobEventResource[] = [];
  let expected = cursor + 1;
  for (const event of items) {
    if (event.sequence < expected) continue;
    if (event.sequence !== expected) break;
    accepted.push(event);
    expected += 1;
  }
  return accepted;
}

function isTerminal(state: JobResource['state']): boolean {
  return (
    state === 'succeeded' || state === 'failed' || state === 'cancelled' || state === 'skipped'
  );
}

function retryDelay(error: Error, retryCount: number): number {
  if (error instanceof ManagementApiError && error.retryAfterMilliseconds !== null) {
    return error.retryAfterMilliseconds;
  }
  return Math.min(
    JOB_EVENT_RETRY_INITIAL_MILLISECONDS * 2 ** retryCount,
    JOB_EVENT_RETRY_MAX_MILLISECONDS,
  );
}

function isRetryable(error: Error): boolean {
  if (!(error instanceof ManagementApiError)) return true;
  return (
    error.code === 'transport_failure' ||
    error.code === 'unavailable' ||
    error.code === 'internal' ||
    error.code === 'rate_limited'
  );
}

function asError(error: unknown): Error {
  return error instanceof Error ? error : new Error('Job event follow failed');
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError';
}

function abortableDelay(milliseconds: number, signal: AbortSignal): Promise<boolean> {
  if (signal.aborted) return Promise.resolve(false);
  return new Promise((resolve) => {
    const timeout = globalThis.setTimeout(() => finish(true), milliseconds);
    const abort = () => finish(false);
    signal.addEventListener('abort', abort, { once: true });

    function finish(elapsed: boolean) {
      globalThis.clearTimeout(timeout);
      signal.removeEventListener('abort', abort);
      resolve(elapsed);
    }
  });
}
