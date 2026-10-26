import { afterEach, expect, it, vi } from 'vitest';
import {
  createManagementApiClient,
  idempotencyHeaders,
  versionedMutationHeaders,
  ManagementApiError,
} from '../../api/client';
import { createIssueSourcesApi } from './sourceApi';
import { issueSourceChoice } from '../../../tests/fixtures/issueSources';
import { integrationFixture } from '../../../tests/fixtures/integrations';

afterEach(() => vi.unstubAllGlobals());
it('discovers, previews and publishes Project task import without an Issue or Factory target and retains exact lifecycle preconditions', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const calls: Request[] = [];
  const api = createIssueSourcesApi(
    createManagementApiClient(async (request) => {
      calls.push(request);
      return Response.json({});
    }),
  );
  const choice = integrationFixture('WorkSourceInputChoices');
  const read = choice.items.find((item) => item.purpose === 'task_read')!;
  const intent = {
    id: '0199a6f4-d56c-7440-9aa2-6a320f862795',
    project_id: 'project',
    expected_current_version: null,
    definition: {
      enabled: true,
      source: read.binding,
      connection: {
        id: 'connection',
        version: 2,
        access_version: choice.access_version,
        source_configuration: read.connection_configuration,
        reporter_configuration: null,
      },
    },
  };
  await api.connections(null);
  await api.imports('project', 'visible-cursor');
  await api.importChoices('project', 'connection', 2);
  const preview = {
    connection_id: 'connection',
    connection_version: 2,
    access_version: choice.access_version,
    source: read.binding,
    limit: 10,
  };
  await api.previewImport('project', preview);
  await api.publishImport(intent, idempotencyHeaders('enable-import'));
  await api.publishImport(
    {
      ...intent,
      expected_current_version: 3,
      definition: { ...intent.definition, enabled: false },
    },
    idempotencyHeaders('disable-import'),
  );
  expect(calls.map((request) => [request.method, new URL(request.url).pathname])).toEqual([
    ['GET', '/api/v1/integrations'],
    ['GET', '/api/v1/projects/project/issue-source-imports'],
    ['GET', '/api/v1/projects/project/issue-source-choices'],
    ['POST', '/api/v1/projects/project/issue-sources/preview'],
    ['POST', `/api/v1/projects/project/issue-source-imports/${intent.id}/versions`],
    ['POST', `/api/v1/projects/project/issue-source-imports/${intent.id}/versions`],
  ]);
  expect(new URL(calls[1]!.url).searchParams.get('after')).toBe('visible-cursor');
  expect(new URL(calls[1]!.url).searchParams.get('limit')).toBe('25');
  expect(new URL(calls[2]!.url).searchParams.get('connection_version')).toBe('2');
  expect(calls[3]!.headers.has('Idempotency-Key')).toBe(false);
  expect(await calls[3]!.json()).toEqual(preview);
  expect(await calls[4]!.json()).toEqual({ intent });
  expect(calls[4]!.headers.get('If-None-Match')).toBe('*');
  expect(calls[5]!.headers.get('If-Match')).toBe('"3"');
  expect(calls.every((request) => request.credentials === 'omit')).toBe(true);
});

it('reads bounded Reporter receipts and submits a reviewed resolution through generated routes', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const calls: Request[] = [];
  const api = createIssueSourcesApi(
    createManagementApiClient(async (request) => {
      calls.push(request);
      return Response.json({});
    }),
  );
  await api.projections('project', 'issue', 'scoped-cursor');
  const body = {
    review: { operation_id: '11'.repeat(32), expected_record: '22'.repeat(32) },
    mapping: {
      expected_mapping: null,
      definition: {
        source: issueSourceChoice().source,
        reporter: null,
        push_enabled: false,
        fields: {},
        status: null,
      },
    },
  };
  await api.resolve('project', 'issue', body, versionedMutationHeaders('repair', 8));
  expect(new URL(calls[0]!.url).pathname).toBe(
    '/api/v1/projects/project/issues/issue/synchronization/projections',
  );
  expect(new URL(calls[0]!.url).searchParams.get('after')).toBe('scoped-cursor');
  expect(new URL(calls[0]!.url).searchParams.get('limit')).toBe('20');
  expect(calls[1]!.method).toBe('POST');
  expect(new URL(calls[1]!.url).pathname).toBe(
    '/api/v1/projects/project/issues/issue/synchronization/resolve',
  );
  expect(calls[1]!.headers.get('If-Match')).toBe('"8"');
  expect(calls[1]!.headers.get('Idempotency-Key')).toBe('repair');
  expect(await calls[1]!.json()).toEqual(body);
});
it('uses generated Source routes with independent Issue/mapping/pull preconditions and no provider transport', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const calls: Request[] = [];
  const api = createIssueSourcesApi(
    createManagementApiClient(async (request) => {
      calls.push(request);
      return Response.json({});
    }),
  );
  const pin = (identity: string) => ({ identity, version: 'v1', digest: '01'.repeat(32) });
  const source = {
    installation: pin('installation'),
    capability: pin('capability'),
    configuration: pin('configuration'),
    resource: pin('resource'),
  };
  await api.review('project', 'issue');
  await api.choices('project', 'issue');
  await api.candidates('project', 'issue', { source, continuation: null, scan_id: null });
  await api.attach(
    'project',
    'issue',
    {
      source,
      item: { resource: 'board', item: '7' },
      expected_field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
      continuation: null,
    },
    versionedMutationHeaders('attach', 8),
  );
  await api.publishMapping(
    'project',
    'issue',
    {
      expected_mapping: null,
      definition: {
        source,
        reporter: null,
        push_enabled: false,
        fields: { description: 'source_owned_pull' },
        status: null,
      },
    },
    versionedMutationHeaders('mapping', 8),
  );
  await api.mappingVersion('project', 'issue', 2);
  await api.refresh(
    'project',
    'issue',
    { source, continuation: null },
    idempotencyHeaders('observe'),
  );
  await api.setPull(
    'project',
    'issue',
    { expected_generation: 3, enabled: false },
    idempotencyHeaders('pull'),
  );
  expect(calls.map((r) => [r.method, new URL(r.url).pathname, r.headers.get('If-Match')])).toEqual([
    ['GET', '/api/v1/projects/project/issues/issue/synchronization', null],
    ['GET', '/api/v1/projects/project/issues/issue/source-choices', null],
    ['POST', '/api/v1/projects/project/issues/issue/source-link/candidates', null],
    ['POST', '/api/v1/projects/project/issues/issue/source-link', '"8"'],
    ['POST', '/api/v1/projects/project/issues/issue/source-link/mappings', '"8"'],
    ['GET', '/api/v1/projects/project/issues/issue/source-link/mappings/2', null],
    ['POST', '/api/v1/projects/project/issues/issue/synchronization/refresh', null],
    ['POST', '/api/v1/projects/project/issues/issue/synchronization/pull', null],
  ]);
  expect(calls[2]!.headers.get('Idempotency-Key')).toBeNull();
  expect(calls[6]!.headers.get('Idempotency-Key')).toBe('observe');
  expect(await calls[7]!.json()).toEqual({ expected_generation: 3, enabled: false });
  expect(calls.every((r) => r.credentials === 'omit')).toBe(true);
});

it('treats an undisclosed canonical association as a definitive conflict without inventing a link', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const api = createIssueSourcesApi(
    createManagementApiClient(async () => Response.json({ kind: 'conflict', canonical: null })),
  );
  const request = api.attach(
    'project',
    'issue',
    {
      source: issueSourceChoice().source,
      item: { resource: 'board', item: '7' },
      expected_field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
      continuation: null,
    },
    versionedMutationHeaders('review', 1),
  );
  await expect(request).rejects.toBeInstanceOf(ManagementApiError);
  await expect(request).rejects.toMatchObject({ code: 'conflict', canonical: null });
});
