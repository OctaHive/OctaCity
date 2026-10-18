import { afterEach, expect, it, vi } from 'vitest';
import { createManagementApiClient } from '../../api/client';
import { createIssueWorkflowsApi } from './api';
import { workflowFixture } from '../../../tests/fixtures/issueWorkflows';

afterEach(() => vi.unstubAllGlobals());
it('uses the Project workflow HTTP contract for exact reads and independently preconditioned default selection', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const workflow = { ...workflowFixture('delivery'), version: 2 };
  const requests: Request[] = [];
  const api = createIssueWorkflowsApi(
    createManagementApiClient(async (request) => {
      requests.push(request);
      if (request.url.includes('issue-workflow-default'))
        return Response.json({ disposition: 'applied', resource: { version: 8, workflow } });
      if (request.url.includes('?')) return Response.json({ items: [], next_cursor: null });
      return Response.json(workflow);
    }),
  );
  await api.list('delivery', 'after-workflow');
  expect(new URL(requests[0]!.url).searchParams.get('after')).toBe('after-workflow');
  expect(Number(new URL(requests[0]!.url).searchParams.get('limit'))).toBeLessThanOrEqual(200);
  const version = await api.get('delivery', workflow.id, 2);
  expect(version).toEqual(workflow);
  expect(new URL(requests[1]!.url).pathname).toBe(
    `/api/v1/projects/delivery/issue-workflows/${workflow.id}/versions/2`,
  );
  await api.selectDefault(
    'delivery',
    { workflow_id: workflow.id, workflow_version: 2 },
    { 'Idempotency-Key': 'select-reviewed-policy', 'If-Match': '"7"' },
  );
  expect(await requests[2]!.json()).toEqual({ workflow_id: workflow.id, workflow_version: 2 });
  expect(requests[2]!.headers.get('If-Match')).toBe('"7"');
  expect(requests[2]!.headers.get('Idempotency-Key')).toBe('select-reviewed-policy');
  expect(requests.every((request) => request.credentials === 'omit')).toBe(true);
});
