import { afterEach, describe, expect, it, vi } from 'vitest';

import type { JobEventPage, JobEventResource } from './api';
import {
  followJobEvents,
  JOB_EVENT_EMPTY_PAGE_DELAY_MILLISECONDS,
  JOB_EVENT_RETRY_INITIAL_MILLISECONDS,
  JOB_EVENT_SEQUENCE_GAP_RETRY_LIMIT,
  JOB_EVENT_VISIBLE_LIMIT,
  JOB_EVENT_WAIT_MILLISECONDS,
  type JobEventFollowerSnapshot,
  type JobEventSource,
  type JobEventTarget,
} from './jobEventFollower';

afterEach(() => {
  vi.useRealTimers();
});

describe('Job event follower', () => {
  it('keeps the last contiguous sequence across empty, appended, duplicate, and gapped pages', async () => {
    vi.useFakeTimers();
    const controller = new AbortController();
    const requests: number[] = [];
    const snapshots: JobEventFollowerSnapshot[] = [];
    const pages: JobEventPage[] = [
      page([], 0),
      page([event(1), event(2)], 2),
      page([event(2), event(4)], 4),
    ];
    const source: JobEventSource = {
      getJob: vi.fn().mockResolvedValue(target('running', 4)),
      getJobEvents: vi.fn(async (_jobId, request, signal) => {
        requests.push(request.afterSequence);
        return pages.shift() ?? pendingUntilAbort(signal);
      }),
    };

    const following = followJobEvents({
      initialJob: target('running', 4),
      onSnapshot: (snapshot) => snapshots.push(snapshot),
      signal: controller.signal,
      source,
    });

    await settleMicrotasks();
    expect(requests).toEqual([0]);
    expect(source.getJobEvents).toHaveBeenLastCalledWith(
      'job-1',
      { afterSequence: 0, waitMilliseconds: JOB_EVENT_WAIT_MILLISECONDS },
      controller.signal,
    );

    await vi.advanceTimersByTimeAsync(JOB_EVENT_EMPTY_PAGE_DELAY_MILLISECONDS);
    await settleMicrotasks();
    expect(requests).toEqual([0, 0, 2]);
    expect(snapshots.at(-1)?.events.map(({ sequence }) => sequence)).toEqual([1, 2]);

    await vi.advanceTimersByTimeAsync(JOB_EVENT_EMPTY_PAGE_DELAY_MILLISECONDS);
    await settleMicrotasks();
    expect(requests).toEqual([0, 0, 2, 2]);
    expect(snapshots.at(-1)?.cursor).toBe(2);
    expect(snapshots.at(-1)?.events.map(({ sequence }) => sequence)).toEqual([1, 2]);

    controller.abort();
    await following;
  });

  it('backs off after retryable failure and stops after catching up to terminal state', async () => {
    vi.useFakeTimers();
    const snapshots: JobEventFollowerSnapshot[] = [];
    const source: JobEventSource = {
      getJob: vi.fn().mockResolvedValue(target('succeeded', 1)),
      getJobEvents: vi
        .fn()
        .mockRejectedValueOnce(new Error('temporary outage'))
        .mockResolvedValueOnce(page([event(1)], 1)),
    };

    const following = followJobEvents({
      initialJob: target('running', 0),
      onSnapshot: (snapshot) => snapshots.push(snapshot),
      signal: new AbortController().signal,
      source,
    });

    await settleMicrotasks();
    expect(source.getJobEvents).toHaveBeenCalledTimes(1);
    expect(snapshots.at(-1)?.phase).toBe('retrying');

    await vi.advanceTimersByTimeAsync(JOB_EVENT_RETRY_INITIAL_MILLISECONDS - 1);
    expect(source.getJobEvents).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    await following;

    expect(source.getJobEvents).toHaveBeenCalledTimes(2);
    expect(snapshots.at(-1)).toMatchObject({ cursor: 1, phase: 'complete' });
  });

  it('stops after bounded recovery from a permanent terminal sequence gap', async () => {
    vi.useFakeTimers();
    const snapshots: JobEventFollowerSnapshot[] = [];
    const source: JobEventSource = {
      getJob: vi.fn().mockResolvedValue(target('failed', 2)),
      getJobEvents: vi.fn().mockResolvedValue(page([event(2)], 2)),
    };

    const following = followJobEvents({
      initialJob: target('failed', 2),
      onSnapshot: (snapshot) => snapshots.push(snapshot),
      signal: new AbortController().signal,
      source,
    });

    for (let attempt = 1; attempt < JOB_EVENT_SEQUENCE_GAP_RETRY_LIMIT; attempt += 1) {
      await settleMicrotasks();
      await vi.advanceTimersByTimeAsync(JOB_EVENT_EMPTY_PAGE_DELAY_MILLISECONDS);
    }
    await following;

    expect(source.getJobEvents).toHaveBeenCalledTimes(JOB_EVENT_SEQUENCE_GAP_RETRY_LIMIT);
    expect(snapshots.at(-1)).toMatchObject({ cursor: 0, phase: 'failed' });
  });

  it('retains only a bounded live event window while advancing the durable cursor', async () => {
    const events = Array.from({ length: JOB_EVENT_VISIBLE_LIMIT + 1 }, (_, index) =>
      event(index + 1),
    );
    const snapshots: JobEventFollowerSnapshot[] = [];
    const source: JobEventSource = {
      getJob: vi.fn().mockResolvedValue(target('succeeded', events.length)),
      getJobEvents: vi.fn().mockResolvedValue(page(events, events.length)),
    };

    await followJobEvents({
      initialJob: target('succeeded', events.length),
      onSnapshot: (snapshot) => snapshots.push(snapshot),
      signal: new AbortController().signal,
      source,
    });

    expect(snapshots.at(-1)).toMatchObject({
      cursor: events.length,
      discardedEventCount: 1,
      phase: 'complete',
    });
    expect(snapshots.at(-1)?.events).toHaveLength(JOB_EVENT_VISIBLE_LIMIT);
    expect(snapshots.at(-1)?.events[0]?.sequence).toBe(2);
  });

  it('aborts the sole in-flight request without starting a parallel poller', async () => {
    vi.useFakeTimers();
    const controller = new AbortController();
    let activeRequests = 0;
    let maximumActiveRequests = 0;
    let requestSignal: AbortSignal | undefined;
    const source: JobEventSource = {
      getJob: vi.fn(),
      getJobEvents: vi.fn(async (_jobId, _request, signal) => {
        requestSignal = signal;
        activeRequests += 1;
        maximumActiveRequests = Math.max(maximumActiveRequests, activeRequests);
        try {
          return await pendingUntilAbort(signal);
        } finally {
          activeRequests -= 1;
        }
      }),
    };

    const following = followJobEvents({
      initialJob: target('running', 0),
      onSnapshot: vi.fn(),
      signal: controller.signal,
      source,
    });
    await settleMicrotasks();

    await vi.advanceTimersByTimeAsync(JOB_EVENT_WAIT_MILLISECONDS * 2);
    expect(source.getJobEvents).toHaveBeenCalledTimes(1);
    expect(maximumActiveRequests).toBe(1);

    controller.abort();
    await following;
    expect(requestSignal?.aborted).toBe(true);
    expect(activeRequests).toBe(0);
  });
});

function target(state: JobEventTarget['state'], eventCursor: number): JobEventTarget {
  return {
    event_cursor: eventCursor,
    id: 'job-1',
    state,
    terminal:
      state === 'failed' || state === 'succeeded' || state === 'cancelled' || state === 'skipped'
        ? {
            completed_at_unix_ms: 1_700_000_001_000,
            failure_classification: state === 'failed' ? 'execution' : null,
            state,
          }
        : null,
  };
}

function event(sequence: number): JobEventResource {
  return {
    kind: 'progress',
    occurred_at_unix_ms: 1_700_000_000_000 + sequence,
    payload: { step: sequence },
    sequence,
  };
}

function page(items: JobEventResource[], cursor: number): JobEventPage {
  return { cursor, items };
}

function pendingUntilAbort(signal: AbortSignal): Promise<JobEventPage> {
  return new Promise((_resolve, reject) => {
    const rejectAbort = () => reject(new DOMException('Aborted', 'AbortError'));
    if (signal.aborted) rejectAbort();
    else signal.addEventListener('abort', rejectAbort, { once: true });
  });
}

async function settleMicrotasks() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}
