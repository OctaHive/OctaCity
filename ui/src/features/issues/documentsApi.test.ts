import { afterEach, expect, it, vi } from 'vitest';
import {
  createManagementApiClient,
  idempotencyHeaders,
  versionedMutationHeaders,
} from '../../api/client';
import { createIssueDocumentsApi } from './documentsApi';
afterEach(() => vi.unstubAllGlobals());

it('keeps Issue metadata/body reads no-store and publication/material commands on separate scoped routes', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const calls: Request[] = [];
  const api = createIssueDocumentsApi(
    createManagementApiClient(async (request) => {
      calls.push(request.clone());
      return Response.json({ project_id: 'project', issue_id: 'issue' });
    }),
  );
  const signal = new AbortController().signal;
  await api.list('project', 'issue', null, signal);
  await api.list('project', 'issue', 'retained.cursor', signal);
  await api.read('project', 'issue', 'exact-publication');
  await api.publish(
    'project',
    'issue',
    { name: 'Task notes', content: 'Private text' },
    idempotencyHeaders('publication'),
  );
  await api.editMaterials(
    'project',
    'issue',
    { references: [] },
    versionedMutationHeaders('selection', 7),
  );
  const base = '/api/v1/projects/project/issues/issue';
  expect(calls.map((r) => [r.method, new URL(r.url).pathname, new URL(r.url).search])).toEqual([
    ['GET', `${base}/input-documents`, ''],
    ['GET', `${base}/input-documents`, '?after=retained.cursor'],
    ['GET', `${base}/input-documents/exact-publication`, ''],
    ['POST', `${base}/input-documents`, ''],
    ['PATCH', `${base}/materials`, ''],
  ]);
  expect(calls.slice(0, 3).every((r) => r.cache === 'no-store' && r.body === null)).toBe(true);
  expect(await calls[3]!.json()).toEqual({ name: 'Task notes', content: 'Private text' });
  expect(calls[3]!.headers.get('Idempotency-Key')).toBe('publication');
  expect(calls[3]!.headers.has('If-Match')).toBe(false);
  expect(await calls[4]!.json()).toEqual({ references: [] });
  expect(calls[4]!.headers.get('Idempotency-Key')).toBe('selection');
  expect(calls[4]!.headers.get('If-Match')).toBe('"7"');
});
