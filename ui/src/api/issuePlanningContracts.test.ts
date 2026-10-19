import { afterEach, expect, expectTypeOf, it, vi } from 'vitest';
import type { components } from '../../.generated/api/schema';
import { createManagementApiClient, idempotencyHeaders, versionedMutationHeaders } from './client';
type Schemas = components['schemas'];
afterEach(() => vi.unstubAllGlobals());
it('generates closed planning contracts with reviewed publication and independent manual creation', async () => {
  expectTypeOf<Schemas['IssuePropertyValue']['type']>().toEqualTypeOf<
    'text' | 'choice' | 'boolean' | 'number' | 'date'
  >();
  expectTypeOf<Schemas['IssuePlanning']>().not.toHaveProperty('status_id');
  expectTypeOf<Schemas['IssuePlanning']>().not.toHaveProperty('execution_state');
  expectTypeOf<Schemas['IssueSummary']>().not.toHaveProperty('planning');
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const requests: Request[] = [];
  const client = createManagementApiClient(async (request) => {
    requests.push(request);
    return Response.json({});
  });
  const project_id = '0199a6f4-d56c-7440-9aa2-6a320f862795';
  const issue_id = '20000000-0000-4000-8000-000000000001';
  const plan: Schemas['IssuePlanning'] = {
    definition_version: 0,
    assignee: null,
    labels: [],
    due_date: null,
    parent_id: null,
    values: {},
  };
  await client.GET('/api/v1/projects/{project_id}/issue-property-choices', {
    params: { path: { project_id } },
  });
  await client.POST('/api/v1/projects/{project_id}/issue-property-definitions', {
    params: { path: { project_id }, header: idempotencyHeaders('catalog') },
    body: { expected_version: 0, definition: { labels: [], properties: [] } },
  });
  await client.GET('/api/v1/projects/{project_id}/issue-property-definitions/{property_version}', {
    params: { path: { project_id, property_version: 0 } },
  });
  await client.POST('/api/v1/projects/{project_id}/issues', {
    params: { path: { project_id }, header: idempotencyHeaders('manual') },
    body: { title: 'Manual task', description: '', planning: plan },
  });
  await client.PATCH('/api/v1/projects/{project_id}/issues/{issue_id}/planning', {
    params: { path: { project_id, issue_id }, header: versionedMutationHeaders('planning', 2) },
    body: plan,
  });
  await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/children', {
    params: { path: { project_id, issue_id }, query: { limit: 1, after: issue_id } },
  });
  expect(await requests[1]!.json()).toEqual({
    expected_version: 0,
    definition: { labels: [], properties: [] },
  });
  expect(requests[1]!.headers.get('If-Match')).toBeNull();
  expect(requests[4]!.headers.get('If-Match')).toBe('"2"');
  expect(await requests[4]!.json()).toEqual(plan);
  expect(requests[2]!.url).toBe(
    `https://console.example/api/v1/projects/${project_id}/issue-property-definitions/0`,
  );
  expect(requests[5]!.url).toBe(
    `https://console.example/api/v1/projects/${project_id}/issues/${issue_id}/children?limit=1&after=${issue_id}`,
  );
  expect(requests.every((request) => request.credentials === 'omit')).toBe(true);
});
