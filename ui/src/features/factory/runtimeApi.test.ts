// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { createManagementApiClient } from '../../api/client';
import { factoryRoleFixture } from '../../../tests/fixtures/factoryRoles';

afterEach(() => vi.unstubAllGlobals());

it('reads, reviews and publishes Runtime settings through generated management contracts', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const { profile } = factoryRoleFixture();
  const definition = {
    toolchain: profile.definition.toolchain,
    mcp_servers: {},
    integrations: {},
    permissions: profile.definition.permissions,
  };
  const body = { expected_version: 7, definition };
  const settings = {
    project_id: profile.role.project_id,
    runtime: 'runtime',
    version: 7,
    definition,
    digest: 'a'.repeat(64),
  };
  const reviewed = { ...settings, version: 8 };
  const responses = [
    { settings: null },
    { settings },
    { total: 0, items: [], next_cursor: null },
    { settings: reviewed },
    { disposition: 'applied', settings: reviewed, published_at: 100 },
  ];
  const requests: Request[] = [];
  const fetch = vi.fn(async (request: Request) => {
    requests.push(request);
    return Response.json(responses[requests.length - 1]);
  });
  const { createFactoryRuntimeApi } = await import('./runtimeApi');
  const api = createFactoryRuntimeApi(createManagementApiClient(fetch));
  const project = profile.role.project_id;
  expect(await api.current(project, 'runtime')).toEqual({ settings: null });
  expect(await api.version(project, 'runtime', 7)).toEqual({ settings });
  await api.choices(project, { limit: 1, after: 'previous-runtime' });
  expect(await api.validate(project, 'runtime', body)).toEqual({ settings: reviewed });
  expect(
    (await api.publish(project, 'runtime', body, { 'Idempotency-Key': 'reviewed-runtime' }))
      .settings,
  ).toEqual(reviewed);
  expect(requests.map((request) => [request.method, new URL(request.url).pathname])).toEqual([
    ['GET', `/api/v1/projects/${project}/factory-runtimes/runtime`],
    ['GET', `/api/v1/projects/${project}/factory-runtimes/runtime/versions/7`],
    ['GET', `/api/v1/projects/${project}/factory-runtime-choices`],
    ['POST', `/api/v1/projects/${project}/factory-runtimes/runtime/validate`],
    ['POST', `/api/v1/projects/${project}/factory-runtimes/runtime`],
  ]);
  expect(new URL(requests[2]!.url).searchParams.get('after')).toBe('previous-runtime');
  expect(new URL(requests[2]!.url).searchParams.get('limit')).toBe('1');
  expect(await requests[3]!.json()).toEqual(body);
  expect(await requests[4]!.json()).toEqual(body);
  expect(requests[3]!.headers.get('Idempotency-Key')).toBeNull();
  expect(requests[4]!.headers.get('Idempotency-Key')).toBe('reviewed-runtime');
  expect(requests[4]!.headers.get('If-Match')).toBeNull();
});
