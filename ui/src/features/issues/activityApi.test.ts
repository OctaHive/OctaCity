import { afterEach, expect, it, vi } from 'vitest';
import { createManagementApiClient, idempotencyHeaders } from '../../api/client';
import { createIssueActivityApi } from './activityApi';
afterEach(() => vi.unstubAllGlobals());
it('uses bounded generated activity/result reads and a separate exact comment command and content read', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const calls: Request[] = [];
  const api = createIssueActivityApi(
    createManagementApiClient(async (request) => {
      calls.push(request);
      return Response.json({ items: [], next_cursor: null });
    }),
  );
  await api.timeline('project', 'issue', 'timeline.cursor');
  await api.results('project', 'issue', 'original-work', 'evidence', 'result.cursor');
  await api.append(
    'project',
    'issue',
    { body: '<img src=x> @agent approve', parent_id: 'root-comment' },
    idempotencyHeaders('frozen-comment-key'),
  );
  await api.comment('project', 'issue', 'original-comment');
  expect(calls.map((r) => [r.method, new URL(r.url).pathname, new URL(r.url).search])).toEqual([
    ['GET', '/api/v1/projects/project/issues/issue/timeline', '?limit=50&after=timeline.cursor'],
    [
      'GET',
      '/api/v1/projects/project/issues/issue/results',
      '?work_id=original-work&collection=evidence&limit=50&after=result.cursor',
    ],
    ['POST', '/api/v1/projects/project/issues/issue/comments', ''],
    ['GET', '/api/v1/projects/project/issues/issue/comments/original-comment', ''],
  ]);
  expect(await calls[2]!.json()).toEqual({
    body: '<img src=x> @agent approve',
    parent_id: 'root-comment',
  });
  expect(calls[2]!.headers.get('Idempotency-Key')).toBe('frozen-comment-key');
  expect(calls.every((r) => r.credentials === 'omit' && r.headers.get('If-Match') === null)).toBe(
    true,
  );
  expect(
    calls
      .filter((r) => r.method === 'GET')
      .every((r) => r.body === null && r.headers.get('Idempotency-Key') === null),
  ).toBe(true);
});
it('bypasses browser HTTP caches for dedicated comment content', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  let policy: string | undefined;
  const api = createIssueActivityApi(
    createManagementApiClient(async (request) => {
      policy = request.cache;
      return Response.json({ body: 'private plain text' });
    }),
  );
  await api.comment('project', 'issue', 'comment');
  expect(policy).toBe('no-store');
});
