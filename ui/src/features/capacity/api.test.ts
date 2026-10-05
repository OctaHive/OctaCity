// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CapacityApi } from './api';

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

describe('capacity command API', () => {
  it.each([
    [
      'drain',
      '/api/v1/agents/agent-a/drain',
      { mode: 'forced' },
      (api: CapacityApi, headers: VersionedHeaders) =>
        api.drainAgent('agent-a', { mode: 'forced' }, headers),
    ],
    [
      'reassignment',
      '/api/v1/agents/agent-a/pool',
      { pool_id: 'pool-b' },
      (api: CapacityApi, headers: VersionedHeaders) =>
        api.reassignAgentPool('agent-a', { pool_id: 'pool-b' }, headers),
    ],
  ] as const)(
    'sends a typed versioned %s command and returns request correlation',
    async (_kind, path, body, invoke) => {
      let observed: Request | null = null;
      vi.stubGlobal(
        'fetch',
        vi.fn(async (request: Request) => {
          observed = request.clone();
          return new Response(JSON.stringify({ disposition: 'applied', resource: {} }), {
            headers: {
              'content-type': 'application/json',
              'x-request-id': 'request-agent-command',
            },
            status: 200,
          });
        }),
      );
      const { capacityApi } = await import('./api');
      const headers = {
        'Idempotency-Key': '11111111-1111-4111-8111-111111111111',
        'If-Match': '"3"',
      } as const;

      const result = await invoke(capacityApi, headers);

      expect(observed).not.toBeNull();
      const request = observed as unknown as Request;
      expect(new URL(request.url).pathname).toBe(path);
      expect(request.method).toBe('POST');
      expect(request.credentials).toBe('omit');
      expect(request.headers.get('idempotency-key')).toBe(headers['Idempotency-Key']);
      expect(request.headers.get('if-match')).toBe(headers['If-Match']);
      expect(await request.json()).toEqual(body);
      expect(result.requestId).toBe('request-agent-command');
      expect(result.response.disposition).toBe('applied');
    },
  );
});

type VersionedHeaders = Readonly<{
  'Idempotency-Key': string;
  'If-Match': string;
}>;
