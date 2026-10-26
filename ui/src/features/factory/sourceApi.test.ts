// @vitest-environment jsdom
import { expect, it } from 'vitest';
import { createManagementApiClient } from '../../api/client';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { createFactorySourcesApi } from './sourceApi';

it.each([
  'connections',
  'list',
  'choices',
  'validate',
  'preview',
  'publish',
  'diagnostics',
] as const)(
  'keeps classified %s failures and server correlation without retaining private response material',
  async (operation) => {
    const requestId = '0199a6f4-d56c-7440-9aa2-6a320f862795';
    const api = createFactorySourcesApi(
      createManagementApiClient(async () =>
        Response.json(
          {
            code: 'capability_unavailable',
            message: 'private-selector-token /private/runtime.json',
            request_id: requestId,
            diagnostics: {
              errors: [{ field: '/private/selector', code: 'unavailable_reference' }],
            },
          },
          { status: 422, headers: { 'X-Request-Id': requestId } },
        ),
      ),
    );
    const intent = integrationFixture('WorkSourceConfigurationSubmission').intent;
    const operations = {
      connections: () => api.connections(null),
      list: () => api.list(intent.project_id, null),
      choices: () => api.choices(intent.project_id, 'connection', 1),
      validate: () => api.validate(intent),
      preview: () => api.preview(intent),
      publish: () => api.publish(intent, { 'Idempotency-Key': 'reviewed-input' }),
      diagnostics: () =>
        api.diagnostics(intent.project_id, { kind: 'input', id: 'input', version: 1 }, null),
    };
    const error: unknown = await operations[operation]().catch((error: unknown) => error);
    expect(error).toMatchObject({ code: 'capability_unavailable', status: 422, requestId });
    expect(String(error)).not.toContain('private-');
    expect(JSON.stringify(error)).not.toContain('private');
  },
);

it('drops remote error text before diagnostics can enter a query cache while retaining safe retry metadata', async () => {
  const api = createFactorySourcesApi(
    createManagementApiClient(async () =>
      Response.json(
        {
          code: 'rate_limited',
          message: 'private-provider-content',
          request_id: 'private-provider-content',
        },
        { status: 429, headers: { 'Retry-After': '2' } },
      ),
    ),
  );
  const error = await api
    .diagnostics('project', { kind: 'input', id: 'input', version: 1 }, null)
    .catch((error: unknown) => error);
  expect(error).toHaveProperty('code', 'rate_limited');
  expect(error).toHaveProperty('retryAfterMilliseconds', 2000);
  expect(error).toHaveProperty('requestId', null);
  expect(String(error)).not.toContain('private-provider-content');
  expect(JSON.stringify(error)).not.toContain('private-provider-content');
});

it('reads exact input and group diagnostics with bounded visible-history cursors and cancellation', async () => {
  const requests: Request[] = [];
  const api = createFactorySourcesApi(
    createManagementApiClient(async (request) => {
      requests.push(request);
      return Response.json({ admissions: [], next_cursor: null });
    }),
  );
  const controller = new AbortController();
  await api.diagnostics(
    'project',
    { kind: 'input', id: 'input', version: 3 },
    'visible-cursor',
    controller.signal,
  );
  await api.diagnostics('project', { kind: 'group', id: 'group', version: 2 }, null);
  expect(requests.map((request) => new URL(request.url).pathname)).toEqual([
    '/api/v1/projects/project/factory-source-inputs/input/versions/3/diagnostics',
    '/api/v1/projects/project/factory-source-groups/group/versions/2/diagnostics',
  ]);
  expect(new URL(requests[0]!.url).searchParams.get('after')).toBe('visible-cursor');
  expect(new URL(requests[0]!.url).searchParams.get('limit')).toBe('10');
  expect(new URL(requests[1]!.url).searchParams.has('after')).toBe(false);
  expect(
    requests.every(
      (request) => request.method === 'GET' && !request.headers.has('Idempotency-Key'),
    ),
  ).toBe(true);
  controller.abort();
  expect(requests[0]!.signal.aborted).toBe(true);
});

it('uses Project-scoped source choices, validation, read-only preview and exact preconditions without credentials or task creation', async () => {
  const requests: Request[] = [];
  const api = createFactorySourcesApi(
    createManagementApiClient(async (request) => {
      requests.push(request);
      return Response.json({});
    }),
  );
  const intent = integrationFixture('WorkSourceConfigurationSubmission').intent;
  const input = intent.inputs[0]!;
  const connection = input.definition.connection!;
  await api.list(intent.project_id, null);
  await api.choices(intent.project_id, connection.id, connection.version);
  await api.validate(intent);
  await api.preview(intent);
  await api.publish(intent, { 'Idempotency-Key': 'source' });
  await api.publish(
    { ...intent, inputs: [{ ...input, expected_version: 3 }] },
    { 'Idempotency-Key': 'replace-source' },
  );
  const group = {
    ...intent,
    inputs: [],
    group: { id: input.id, expected_version: 2, members: [{ id: input.id, version: 3 }] },
  };
  await api.publish(group, { 'Idempotency-Key': 'group' });
  expect(requests.map((r) => [r.method, new URL(r.url).pathname])).toEqual([
    ['GET', `/api/v1/projects/${intent.project_id}/factory-sources`],
    ['GET', `/api/v1/projects/${intent.project_id}/factory-source-choices`],
    ['POST', `/api/v1/projects/${intent.project_id}/factory-sources/validate`],
    ['POST', `/api/v1/projects/${intent.project_id}/factory-sources/preview`],
    ['POST', `/api/v1/projects/${intent.project_id}/factory-source-inputs/${input.id}/versions`],
    ['POST', `/api/v1/projects/${intent.project_id}/factory-source-inputs/${input.id}/versions`],
    ['POST', `/api/v1/projects/${intent.project_id}/factory-source-groups/${input.id}/versions`],
  ]);
  expect(requests[4]!.headers.get('If-None-Match')).toBe('*');
  expect(requests[5]!.headers.get('If-Match')).toBe('"3"');
  expect(requests[6]!.headers.get('If-Match')).toBe('"2"');
  for (const request of requests.slice(0, 4))
    expect(request.headers.has('Idempotency-Key')).toBe(false);
  expect(await requests[3]!.json()).toEqual({ intent, limit: 10 });
  expect(new URL(requests[1]!.url).searchParams.get('connection_id')).toBe(connection.id);
  expect(new URL(requests[1]!.url).searchParams.get('connection_version')).toBe('1');
});
