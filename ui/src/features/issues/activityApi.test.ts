import { afterEach, expect, it, vi } from 'vitest';
import { createManagementApiClient, idempotencyHeaders } from '../../api/client';
import { createIssueActivityApi } from './activityApi';
afterEach(() => vi.unstubAllGlobals());
it('reads requestless Work events without inventing an execution request', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const calls: Request[] = [];
  const api = createIssueActivityApi(
    createManagementApiClient(async (request) => {
      calls.push(request);
      return Response.json({});
    }),
  );
  const target = { build_execution: 'a'.repeat(64), job_id: 'original-job' };
  await api.journalEvents('project', 'issue', { work_id: 'original-work' }, target, 'retained');
  await api.journalEvent('project', 'issue', { work_id: 'original-work' }, target, 7);
  expect(calls.map((r) => new URL(r.url).pathname)).toEqual([
    '/api/v1/projects/project/issues/issue/work/original-work/journal/events',
    '/api/v1/projects/project/issues/issue/work/original-work/journal/events/7',
  ]);
  expect(new URL(calls[0]!.url).searchParams.get('after')).toBe('retained');
  expect(calls.every((r) => r.cache === 'no-store' && r.body === null)).toBe(true);
});
it('reads only the selected journal, Job stream and expanded event through bounded no-store contracts', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const calls: Request[] = [];
  const api = createIssueActivityApi(
    createManagementApiClient(async (request) => {
      calls.push(request);
      return Response.json({});
    }),
  );
  const signal = new AbortController().signal;
  const target = { build_execution: 'a'.repeat(64), job_id: 'original-job' };
  await api.journal(
    'project',
    'issue',
    'historical-request',
    'node-attempts',
    'hierarchy.cursor',
    signal,
  );
  await api.journalEvents(
    'project',
    'issue',
    'historical-request',
    target,
    'stream.cursor',
    signal,
  );
  await api.journalEvent('project', 'issue', 'historical-request', target, 7, signal);
  expect(
    calls.map((request) => [
      request.method,
      new URL(request.url).pathname,
      [...new URL(request.url).searchParams],
    ]),
  ).toEqual([
    [
      'GET',
      '/api/v1/projects/project/issues/issue/executions/historical-request/journal',
      [
        ['collection', 'node-attempts'],
        ['limit', '50'],
        ['after', 'hierarchy.cursor'],
      ],
    ],
    [
      'GET',
      '/api/v1/projects/project/issues/issue/executions/historical-request/journal/events',
      [
        ['build_execution', target.build_execution],
        ['job_id', 'original-job'],
        ['limit', '50'],
        ['after', 'stream.cursor'],
      ],
    ],
    [
      'GET',
      '/api/v1/projects/project/issues/issue/executions/historical-request/journal/events/7',
      [
        ['build_execution', target.build_execution],
        ['job_id', 'original-job'],
      ],
    ],
  ]);
  expect(
    calls.every(
      (request) =>
        request.cache === 'no-store' &&
        request.body === null &&
        !request.headers.has('Idempotency-Key'),
    ),
  ).toBe(true);
});
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
