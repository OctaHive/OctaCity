// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

it('resolves the first executor through the installed binding owner without a Flow or mutation identity', async () => {
  const requests: Request[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (request: Request) => {
      requests.push(request);
      return Response.json({ build: null, incompatibility: 'build_unavailable' });
    }),
  );
  const { factoryRolesApi } = await import('./roleApi');
  const request = {
    profile: { project_id: 'delivery', id: 'profile', version: 2, digest: 'a'.repeat(64) },
    capability: 'installed-runtime',
    result_outcome: 'completed',
  };
  expect(await factoryRolesApi.resolveExecutionBinding('delivery', request)).toEqual({
    build: null,
    incompatibility: 'build_unavailable',
  });
  expect(requests).toHaveLength(1);
  expect(new URL(requests[0]!.url).pathname).toBe(
    '/api/v1/projects/delivery/factory-execution-bindings/resolve',
  );
  expect(await requests[0]!.json()).toEqual(request);
  expect(requests[0]!.headers.has('Idempotency-Key')).toBe(false);
});
