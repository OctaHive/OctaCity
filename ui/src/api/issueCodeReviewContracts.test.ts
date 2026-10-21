import { afterEach, expect, expectTypeOf, it, vi } from 'vitest';
import type { components } from '../../.generated/api/schema';
import { createManagementApiClient, idempotencyHeaders, versionedMutationHeaders } from './client';

type Schemas = components['schemas'];
afterEach(() => vi.unstubAllGlobals());
it('generates independent review attachment and retained reads without lifecycle or credential setters', async () => {
  expectTypeOf<Schemas['AttachIssueCodeReviewRequest']>().not.toHaveProperty('credentials');
  expectTypeOf<Schemas['AttachIssueCodeReviewRequest']>().not.toHaveProperty('url');
  expectTypeOf<Schemas['AttachIssueCodeReviewRequest']>().not.toHaveProperty('provenance');
  expectTypeOf<Schemas['AttachIssueCodeReviewRequest']>().not.toHaveProperty('status_id');
  expectTypeOf<Schemas['IssueCodeReviewAvailability']>().toEqualTypeOf<'available' | 'blocked'>();
  expectTypeOf<Schemas['IssueCodeReviewObservation']['changed_files']>().toEqualTypeOf<
    number | null
  >();
  expectTypeOf<Schemas['IssueCodeReviewObservation']['checks']>().toEqualTypeOf<string | null>();
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const requests: Request[] = [];
  const client = createManagementApiClient(async (request) => {
    requests.push(request);
    return Response.json({});
  });
  const project_id = '0199a6f4-d56c-7440-9aa2-6a320f862795';
  const issue_id = '20000000-0000-4000-8000-000000000001';
  const review_id = 'ab'.repeat(32);
  const pin = { identity: 'installed-code-reader', version: 'v1', digest: 'cd'.repeat(32) };
  const body: Schemas['AttachIssueCodeReviewRequest'] = {
    selection: {
      source: { installation: pin, capability: pin, configuration: pin, resource: pin },
      item: { resource: 'native-code-repository', item: 'native-merge-request' },
    },
  };
  await client.POST('/api/v1/projects/{project_id}/issues/{issue_id}/code-reviews', {
    params: { path: { project_id, issue_id }, header: versionedMutationHeaders('attach', 3) },
    body,
  });
  await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/code-reviews', {
    params: { path: { project_id, issue_id }, query: { limit: 1, after: 'original-membership' } },
  });
  await client.POST(
    '/api/v1/projects/{project_id}/issues/{issue_id}/code-reviews/{review_id}/observations',
    {
      params: { path: { project_id, issue_id, review_id }, header: idempotencyHeaders('refresh') },
      body: {},
    },
  );
  await client.GET(
    '/api/v1/projects/{project_id}/issues/{issue_id}/code-reviews/{review_id}/observations',
    {
      params: {
        path: { project_id, issue_id, review_id },
        query: { limit: 1, after: 'original-head-history' },
      },
    },
  );
  expect(requests.map((request) => request.method)).toEqual(['POST', 'GET', 'POST', 'GET']);
  expect(requests.map((request) => request.headers.get('If-Match'))).toEqual([
    '"3"',
    null,
    null,
    null,
  ]);
  expect(requests.map((request) => request.headers.get('Idempotency-Key'))).toEqual([
    'attach',
    null,
    'refresh',
    null,
  ]);
  expect(await requests[0]!.json()).toEqual(body);
  expect(await requests[2]!.json()).toEqual({});
  expect(requests[3]!.url).toBe(
    `https://console.example/api/v1/projects/${project_id}/issues/${issue_id}/code-reviews/${review_id}/observations?limit=1&after=original-head-history`,
  );
  expect(requests.every((request) => request.credentials === 'omit')).toBe(true);
});
