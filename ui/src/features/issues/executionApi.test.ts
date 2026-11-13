import { afterEach, expect, it, vi } from 'vitest';
import { createManagementApiClient, versionedMutationHeaders } from '../../api/client';
import { createIssueExecutionsApi, type IssueExecutionRequest } from './executionApi';

afterEach(() => vi.unstubAllGlobals());
it('uses generated reviewed execution HTTP contracts with frozen headers and bounded read-only history', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const calls: Request[] = [];
  const client = createManagementApiClient(async (request) => {
    calls.push(request);
    return Response.json({ state: 'pending' }, { status: request.method === 'POST' ? 202 : 200 });
  });
  const api = createIssueExecutionsApi(client);
  const request: IssueExecutionRequest = {
    target: {
      configuration_id: 'target',
      configuration_version: 3,
      repository_id: 'repository',
      repository_version: 2,
      source: { kind: 'default_reference' },
      mapping: 'title_description_acceptance_v1',
    },
    classification: { priority: 20, risk: 'medium', metadata: {} },
  };
  await api.start('project', 'issue', request, versionedMutationHeaders('original-review', 7));
  await api.get('project', 'issue', 'request');
  await api.list('project', 'issue', 'opaque.cursor');
  expect(calls.map((r) => [r.method, new URL(r.url).pathname, r.headers.get('If-Match')])).toEqual([
    ['POST', '/api/v1/projects/project/issues/issue/executions', '"7"'],
    ['GET', '/api/v1/projects/project/issues/issue/executions/request', null],
    ['GET', '/api/v1/projects/project/issues/issue/executions', null],
  ]);
  expect(calls[0]!.headers.get('Idempotency-Key')).toBe('original-review');
  expect(await calls[0]!.json()).toEqual(request);
  expect(new URL(calls[2]!.url).search).toBe('?limit=50&after=opaque.cursor');
  expect(calls.every((r) => r.credentials === 'omit')).toBe(true);
  expect(
    calls.slice(1).every((r) => r.headers.get('Idempotency-Key') === null && r.body === null),
  ).toBe(true);
});

it('reads original legacy Work through generated no-store contracts without manufacturing a request', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const calls: Request[] = [];
  const api = createIssueExecutionsApi(
    createManagementApiClient(async (request) => {
      calls.push(request);
      return Response.json({ items: [], next_cursor: null, observation: 'a'.repeat(64) });
    }),
  );
  await api.listLegacy('project', 'issue', 'opaque.cursor');
  await api.getWork('project', 'issue', 'original-work');
  expect(calls.map((r) => [r.method, new URL(r.url).pathname, r.cache])).toEqual([
    ['GET', '/api/v1/projects/project/issues/issue/legacy-work', 'no-store'],
    ['GET', '/api/v1/projects/project/issues/issue/work/original-work', 'no-store'],
  ]);
  expect(new URL(calls[0]!.url).search).toBe('?limit=50&after=opaque.cursor');
  expect(calls.every((r) => r.body === null && r.headers.get('Idempotency-Key') === null)).toBe(
    true,
  );
});

it('retains bounded snapshot-specific review diagnostics from the HTTP boundary', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const api = createIssueExecutionsApi(
    createManagementApiClient(async () =>
      Response.json(
        {
          code: 'invalid_request',
          message: 'Snapshot prerequisites failed',
          request_id: 'review-diagnostic',
          diagnostics: {
            errors: [
              {
                field: '/issue_snapshot/materials/context',
                code: 'issue_snapshot_material_unavailable',
              },
            ],
          },
        },
        { status: 400 },
      ),
    ),
  );
  await expect(
    api.review(
      'project',
      'issue',
      {
        target: {
          configuration_id: 'config',
          configuration_version: 1,
          repository_id: 'repo',
          repository_version: 1,
          source: { kind: 'default_reference' },
          mapping: 'title_description_acceptance_v1',
        },
      },
      1,
    ),
  ).rejects.toMatchObject({
    validationIssues: [
      { field: '/issue_snapshot/materials/context', code: 'issue_snapshot_material_unavailable' },
    ],
  });
});

it('reviews exact inputs without a mutation key and reads only the selected retained snapshot with no-store', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const calls: Request[] = [];
  const api = createIssueExecutionsApi(
    createManagementApiClient(async (request) => {
      calls.push(request);
      return Response.json({});
    }),
  );
  const request: IssueExecutionRequest = {
    target: {
      configuration_id: 'config',
      configuration_version: 2,
      repository_id: 'repo',
      repository_version: 1,
      mapping: 'title_description_acceptance_v1',
      source: { kind: 'reference', value: 'main' },
    },
  };
  await api.review('project', 'issue', request, 7);
  await api.snapshot('project', 'issue', 'exact-request');
  expect(
    calls.map((r) => [
      r.method,
      new URL(r.url).pathname,
      r.cache,
      r.headers.get('If-Match'),
      r.headers.get('Idempotency-Key'),
    ]),
  ).toEqual([
    ['POST', '/api/v1/projects/project/issues/issue/executions/review', 'no-store', '"7"', null],
    [
      'GET',
      '/api/v1/projects/project/issues/issue/executions/exact-request/snapshot',
      'no-store',
      null,
      null,
    ],
  ]);
  expect(await calls[0]!.json()).toEqual(request);
  expect(calls[1]!.body).toBeNull();
});
