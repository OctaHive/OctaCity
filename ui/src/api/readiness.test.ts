import { afterEach, describe, expect, it, vi } from 'vitest';

import { fetchReadiness } from './readiness';

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('readiness boundary', () => {
  it.each([
    [200, 'ready', 'ready'],
    [503, 'unavailable', 'unavailable'],
    [200, 'unexpected', 'unavailable'],
  ] as const)('maps HTTP %s with %s to %s', async (status, responseStatus, expected) => {
    const fetch = vi.fn(
      async () =>
        new Response(JSON.stringify({ status: responseStatus }), {
          status,
          headers: { 'content-type': 'application/json' },
        }),
    );
    vi.stubGlobal('fetch', fetch);

    await expect(fetchReadiness()).resolves.toBe(expected);
    expect(fetch).toHaveBeenCalledWith('/health/ready', {
      cache: 'no-store',
      credentials: 'omit',
      headers: { accept: 'application/json' },
      signal: expect.any(AbortSignal),
    });
  });

  it('reports a reachable malformed response as unavailable', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('<html>proxy error</html>')),
    );

    await expect(fetchReadiness()).resolves.toBe('unavailable');
  });

  it('reports an unreachable server without exposing transport details', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => Promise.reject(new Error('private proxy detail'))),
    );

    await expect(fetchReadiness()).resolves.toBe('unreachable');
  });

  it('passes caller cancellation to the active request', async () => {
    const parent = new AbortController();
    vi.stubGlobal(
      'fetch',
      vi.fn(
        (_path: RequestInfo | URL, init?: RequestInit) =>
          new Promise<Response>((_resolve, reject) => {
            init?.signal?.addEventListener(
              'abort',
              () => reject(new DOMException('cancelled', 'AbortError')),
              { once: true },
            );
          }),
      ),
    );

    const readiness = fetchReadiness(parent.signal);
    parent.abort();

    await expect(readiness).resolves.toBe('unreachable');
  });

  it('bounds a stalled readiness request', async () => {
    vi.useFakeTimers();
    vi.stubGlobal(
      'fetch',
      vi.fn(
        (_path: RequestInfo | URL, init?: RequestInit) =>
          new Promise<Response>((_resolve, reject) => {
            init?.signal?.addEventListener(
              'abort',
              () => reject(new DOMException('timed out', 'AbortError')),
              { once: true },
            );
          }),
      ),
    );

    const readiness = fetchReadiness();
    await vi.runAllTimersAsync();

    await expect(readiness).resolves.toBe('unreachable');
  });
});
