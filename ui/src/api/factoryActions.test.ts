import { afterEach, expect, it, vi } from 'vitest';
import { createManagementApiClient, versionedMutationHeaders } from './client';
import { createFactoryActionsApi } from './factoryActions';
afterEach(() => vi.unstubAllGlobals());
it('uses bounded generated global Project Issue and exact-target reads plus the existing guarded command', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const requests: Request[] = [];
  const api = createFactoryActionsApi(
    createManagementApiClient(async (request) => {
      requests.push(request);
      return Response.json({});
    }),
  );
  await api.list({}, null);
  await api.list({ projectId: 'project' }, 'next');
  await api.list({ projectId: 'project', issueId: 'issue' }, null);
  await api.observe('original-run', 'original-gate');
  await api.respondNode(
    'original-run',
    'original-gate',
    { outcome: 'approved', value: true },
    versionedMutationHeaders('frozen-response', 9),
  );
  expect(requests.map((r) => [r.method, new URL(r.url).pathname])).toEqual([
    ['GET', '/api/v1/factory-actions'],
    ['GET', '/api/v1/projects/project/factory-actions'],
    ['GET', '/api/v1/projects/project/issues/issue/actions'],
    ['GET', '/api/v1/factory-runs/original-run/nodes/original-gate/action'],
    ['POST', '/api/v1/factory-runs/original-run/nodes/original-gate/responses'],
  ]);
  expect(new URL(requests[1]!.url).search).toBe('?limit=50&after=next');
  expect(requests.slice(0, 4).every((r) => r.cache === 'no-store')).toBe(true);
  expect(requests[4]!.headers.get('If-Match')).toBe('"9"');
  expect(requests[4]!.headers.get('Idempotency-Key')).toBe('frozen-response');
  expect(await requests[4]!.json()).toEqual({ outcome: 'approved', value: true });
});
it('binds an exact selected action to the original card scope without an Issue-version control precondition', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const calls: Request[] = [];
  const api = createFactoryActionsApi(
    createManagementApiClient(async (request) => {
      calls.push(request);
      return Response.json({});
    }),
  );
  await api.observe('run', 'gate', undefined, { projectId: 'project', issueId: 'issue' });
  expect(new URL(calls[0]!.url).search).toBe('?project_id=project&issue_id=issue');
  expect(calls[0]!.headers.has('If-Match')).toBe(false);
});
