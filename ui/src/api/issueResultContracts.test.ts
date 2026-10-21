import { afterEach, expect, expectTypeOf, it, vi } from 'vitest';
import type { components } from '../../.generated/api/schema';
import { createManagementApiClient } from './client';

type Schemas = components['schemas'];
afterEach(() => vi.unstubAllGlobals());
it('reads exact retained result subjects and explicit content without provider or acceptance setters', async () => {
  expectTypeOf<Schemas['IssueCheckState']>().toEqualTypeOf<
    | 'missing'
    | 'pending'
    | 'running'
    | 'pass'
    | 'fail'
    | 'cancelled'
    | 'skipped'
    | 'indeterminate'
    | 'unavailable'
  >();
  expectTypeOf<Schemas['IssueChecksPage']>().not.toHaveProperty('accepted');
  expectTypeOf<Schemas['IssueArtifactProjection']>().not.toHaveProperty('get_url');
  expectTypeOf<Schemas['IssueExternalChecksPage']['source']>().toEqualTypeOf<'external_ci'>();
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const requests: Request[] = [];
  const client = createManagementApiClient(async (request) => {
    requests.push(request);
    return request.url.includes('/content?')
      ? new Response('retained text', { headers: { 'content-type': 'text/plain' } })
      : Response.json({ items: [], coverage: 'partial' });
  });
  const path = {
    project_id: '0199a6f4-d56c-7440-9aa2-6a320f862795',
    issue_id: '20000000-0000-4000-8000-000000000001',
  };
  const work_id = '20000000-0000-4000-8000-000000000002';
  const record = 'ab'.repeat(32);
  await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/results/checks', {
    params: { path, query: { work_id, record, limit: 1, after: 'exact-original-page' } },
  });
  await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/results/artifacts', {
    params: { path, query: { work_id, limit: 1 } },
  });
  await client.GET(
    '/api/v1/projects/{project_id}/issues/{issue_id}/code-reviews/{review_id}/checks',
    {
      params: { path: { ...path, review_id: 'cd'.repeat(32) }, query: { after: 'original-head' } },
    },
  );
  const content = await client.GET(
    '/api/v1/projects/{project_id}/issues/{issue_id}/results/artifacts/{artifact_id}/content',
    {
      params: {
        path: { ...path, artifact_id: '20000000-0000-4000-8000-000000000003' },
        query: { work_id, mode: 'preview' },
      },
      parseAs: 'blob',
    },
  );
  expect(await content.data?.text()).toBe('retained text');
  expect(requests.map((request) => request.method)).toEqual(['GET', 'GET', 'GET', 'GET']);
  expect(new URL(requests[0]!.url).searchParams.get('record')).toBe(record);
  expect(new URL(requests[0]!.url).searchParams.get('after')).toBe('exact-original-page');
  expect(
    requests.every((request) => request.url.startsWith('https://console.example/api/v1/')),
  ).toBe(true);
  expect(requests.every((request) => request.headers.get('Idempotency-Key') === null)).toBe(true);
});
