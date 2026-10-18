import { readFileSync } from 'node:fs';
import { afterEach, expect, expectTypeOf, it, vi } from 'vitest';
import type { components } from '../../.generated/api/schema';
import { createManagementApiClient, idempotencyHeaders, versionedMutationHeaders } from './client';

type Schemas = components['schemas'];
afterEach(() => vi.unstubAllGlobals());

it('exposes typed workflow settings and separate publication/default requests without a provider', async () => {
  expectTypeOf<Schemas['IssueWorkflowStatus']>().toEqualTypeOf<{ id: string; name: string }>();
  expectTypeOf<Schemas['IssueWorkflowDefinition']>().toHaveProperty('initial_status_id');
  expectTypeOf<Schemas['IssueWorkflowDefinition']>().toHaveProperty('terminal_status_ids');
  expectTypeOf<Schemas['IssueWorkflowDefinition']>().not.toHaveProperty('run_state');
  expectTypeOf<Schemas['IssueWorkflowTransition']['evidence'][number]>().toEqualTypeOf<
    'accepted_execution' | 'passed_required_checks'
  >();
  const definition: Schemas['IssueWorkflowDefinition'] = JSON.parse(
    readFileSync(
      '../server/api/octacity-server-api-rest/fixtures/v1/issue-workflow-definition.json',
      'utf8',
    ),
  );
  const project_id = '0199a6f4-d56c-7440-9aa2-6a320f862795';
  const workflow_id = '20000000-0000-4000-8000-000000000001';
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const requests: Request[] = [];
  const client = createManagementApiClient(async (request) => {
    requests.push(request);
    return Response.json({});
  });
  await client.POST('/api/v1/projects/{project_id}/issue-workflows/validate', {
    params: { path: { project_id } },
    body: { definition },
  });
  await client.POST('/api/v1/projects/{project_id}/issue-workflows', {
    params: { path: { project_id }, header: idempotencyHeaders('new-workflow') },
    body: { id: workflow_id, definition },
  });
  await client.POST('/api/v1/projects/{project_id}/issue-workflows/{workflow_id}/versions', {
    params: {
      path: { project_id, workflow_id },
      header: versionedMutationHeaders('replace-workflow', 2),
    },
    body: { definition },
  });
  await client.POST('/api/v1/projects/{project_id}/issue-workflow-default', {
    params: { path: { project_id }, header: versionedMutationHeaders('select-workflow', 7) },
    body: { workflow_id, workflow_version: 3 },
  });
  expect(requests.map((request) => request.headers.get('If-Match'))).toEqual([
    null,
    null,
    '"2"',
    '"7"',
  ]);
  expect(requests[0]!.headers.has('Idempotency-Key')).toBe(false);
  expect(await requests[1]!.json()).toEqual({ id: workflow_id, definition });
  expect(await requests[3]!.json()).toEqual({ workflow_id, workflow_version: 3 });
  expect(requests.every((request) => request.credentials === 'omit')).toBe(true);
});
