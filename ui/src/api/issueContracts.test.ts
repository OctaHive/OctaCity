import { afterEach, expect, expectTypeOf, it, vi } from 'vitest';
import type { components } from '../../.generated/api/schema';
import { createManagementApiClient, idempotencyHeaders, versionedMutationHeaders } from './client';
type Schemas = components['schemas'];
afterEach(() => vi.unstubAllGlobals());
it('generates provider-free task requests with separate edit, move and adoption contracts', async () => {
  expectTypeOf<Schemas['IssueContent']>().not.toHaveProperty('status_id');
  expectTypeOf<Schemas['IssueContent']>().not.toHaveProperty('workflow_id');
  expectTypeOf<Schemas['IssueSummary']>().not.toHaveProperty('content');
  expectTypeOf<Schemas['IssueRevisionHeader']>().not.toHaveProperty('description');
  expectTypeOf<Schemas['IssueMoveBlockReason']>().toEqualTypeOf<
    'permission_required' | 'source_owned' | 'origin_not_allowed' | 'evidence_required'
  >();
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const requests: Request[] = [];
  const client = createManagementApiClient(async (request) => {
    requests.push(request);
    return Response.json({});
  });
  const project_id = '0199a6f4-d56c-7440-9aa2-6a320f862795';
  const issue_id = '20000000-0000-4000-8000-000000000001';
  const status_id = '10000000-0000-4000-8000-000000000001';
  const content: Schemas['IssueContent'] = { title: 'Manual draft', description: '' };
  await client.POST('/api/v1/projects/{project_id}/issues', {
    params: { path: { project_id }, header: idempotencyHeaders('create') },
    body: content,
  });
  await client.GET('/api/v1/projects/{project_id}/issues', {
    params: { path: { project_id }, query: { limit: 1, after: issue_id } },
  });
  await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}', {
    params: { path: { project_id, issue_id } },
  });
  await client.PATCH('/api/v1/projects/{project_id}/issues/{issue_id}', {
    params: { path: { project_id, issue_id }, header: versionedMutationHeaders('edit', 2) },
    body: content,
  });
  await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/revisions', {
    params: { path: { project_id, issue_id }, query: { limit: 1, after: 1 } },
  });
  await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/revisions/{version}', {
    params: { path: { project_id, issue_id, version: 1 } },
  });
  await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/transitions', {
    params: { path: { project_id, issue_id } },
  });
  const transition: Schemas['TransitionIssueRequest'] = {
    workflow_id: issue_id,
    workflow_version: 1,
    expected_status_id: status_id,
    transition_id: '30000000-0000-4000-8000-000000000001',
    reason: null,
  };
  await client.POST('/api/v1/projects/{project_id}/issues/{issue_id}/transitions', {
    params: { path: { project_id, issue_id }, header: versionedMutationHeaders('move', 3) },
    body: transition,
  });
  await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/status-history', {
    params: { path: { project_id, issue_id }, query: { limit: 1, after: 1 } },
  });
  await client.POST('/api/v1/projects/{project_id}/issues/{issue_id}/workflow-adoption', {
    params: { path: { project_id, issue_id }, header: versionedMutationHeaders('adopt', 4) },
    body: {
      expected_workflow_id: issue_id,
      expected_workflow_version: 1,
      expected_status_id: status_id,
      target_workflow_id: issue_id,
      target_workflow_version: 2,
      status_correspondence: { [status_id]: status_id },
      expected_field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
    },
  });
  expect(requests.map((request) => request.headers.get('If-Match'))).toEqual([
    null,
    null,
    null,
    '"2"',
    null,
    null,
    null,
    '"3"',
    null,
    '"4"',
  ]);
  expect(requests[1]!.url).toBe(
    `https://console.example/api/v1/projects/${project_id}/issues?limit=1&after=${issue_id}`,
  );
  expect(await requests[7]!.json()).toEqual(transition);
  expect(requests.every((request) => request.credentials === 'omit')).toBe(true);
});
