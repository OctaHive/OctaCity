// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

it('sends bounded Factory filters and exclusive continuation through the typed REST client', async () => {
  const requests: Request[] = [];
  const fetch = vi.fn(async (request: Request) => {
    requests.push(request);
    return Response.json({ items: [], next_cursor: null, total: 0 });
  });
  vi.stubGlobal('fetch', fetch);
  const { factoryApi } = await import('./api');
  await factoryApi.listRuns(
    'project-a',
    { state: 'waiting', decision: 'rework', delivery_state: 'unknown', admitted_from_unix_ms: 10 },
    'next-page',
  );
  const request = requests[0];
  expect(request).toBeInstanceOf(Request);
  if (request === undefined) throw new Error('Factory request was not sent');
  const url = new URL(request.url);
  expect(url.pathname).toBe('/api/v1/projects/project-a/factory-runs');
  expect([...url.searchParams.entries()]).toEqual([
    ['state', 'waiting'],
    ['decision', 'rework'],
    ['delivery_state', 'unknown'],
    ['admitted_from_unix_ms', '10'],
    ['after', 'next-page'],
    ['limit', '25'],
  ]);
  expect(request.credentials).toBe('omit');
});

it('normalizes a guided schema on the owning Project and pages only visible resource choices', async () => {
  const requests: Request[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (request: Request) => {
      requests.push(request);
      return Response.json({ items: [], next_cursor: null, total: 0 });
    }),
  );
  const { factoryApi } = await import('./api');
  await factoryApi.listChoices('visible-project', 'next');
  await factoryApi.normalizeSchema({
    project_id: 'visible-project',
    identity: 'operator.boolean',
    version: 'v1',
    shape: { type: 'boolean' },
  });
  expect(new URL(requests[0]!.url).pathname).toBe(
    '/api/v1/projects/visible-project/factory-configuration-choices',
  );
  expect(new URL(requests[0]!.url).searchParams.get('limit')).toBe('25');
  expect(new URL(requests[1]!.url).pathname).toBe('/api/v1/flow-data-schemas/validate');
  expect(requests[1]!.method).toBe('POST');
  expect(await requests[1]!.json()).toEqual({
    project_id: 'visible-project',
    identity: 'operator.boolean',
    version: 'v1',
    shape: { type: 'boolean' },
  });
});

it('validates without mutation headers and publishes immutable configuration and Flow versions with explicit intent headers', async () => {
  const requests: Request[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (request: Request) => {
      requests.push(request);
      return Response.json({ disposition: 'created', resource: {} });
    }),
  );
  const { factoryApi } = await import('./api');
  const { configurationFixture } = await import('../../../tests/fixtures/factory');
  const configuration = configurationFixture().configuration;
  const { reference, ...root } = configuration.flow.closure.definitions[0]!;
  void reference;
  const definition = {
    root,
    data_schemas: configuration.flow.data_schemas,
    limits: configuration.flow.limits,
  };
  const config = {
    enabled: configuration.enabled,
    admission_policy: 'visible-policy',
    permission_ceiling: 'visible-ceiling',
    flow: configuration.flow,
    hard_budget: configuration.hard_budget,
    wip_limits: configuration.wip_limits,
  };
  const create = {
    id: configuration.reference.id,
    project_id: configuration.reference.project_id,
    definition: config,
  };
  const flow = {
    id: configuration.flow.closure.root.id,
    project_id: create.project_id,
    definition,
  };
  const headers = { 'Idempotency-Key': '44444444-4444-4444-8444-444444444444', 'If-Match': '"7"' };
  await factoryApi.validateConfiguration(create);
  await factoryApi.validateFlow(flow);
  await factoryApi.createConfiguration(create, headers);
  await factoryApi.replaceConfiguration(create.id, { definition: config }, headers);
  await factoryApi.createFlow(flow, headers);
  await factoryApi.replaceFlow(flow.id, { definition }, headers);
  expect(
    requests
      .slice(0, 2)
      .every(
        (request) => !request.headers.has('Idempotency-Key') && !request.headers.has('If-Match'),
      ),
  ).toBe(true);
  expect(
    requests
      .slice(2)
      .map((request) => [
        request.method,
        new URL(request.url).pathname,
        request.headers.get('Idempotency-Key'),
        request.headers.get('If-Match'),
      ]),
  ).toEqual([
    ['POST', '/api/v1/factory-configurations', headers['Idempotency-Key'], null],
    [
      'POST',
      `/api/v1/factory-configurations/${create.id}/versions`,
      headers['Idempotency-Key'],
      '"7"',
    ],
    ['POST', '/api/v1/flow-definitions', headers['Idempotency-Key'], null],
    ['POST', `/api/v1/flow-definitions/${flow.id}/versions`, headers['Idempotency-Key'], '"7"'],
  ]);
});
