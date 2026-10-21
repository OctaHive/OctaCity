// @vitest-environment jsdom
import { expect, it } from 'vitest';
import { createManagementApiClient } from '../../api/client';
import { createIntegrationsApi } from './api';
import { integrationFixture } from '../../../tests/fixtures/integrations';

it('submits credentials only to the protected operation and discards echoed error details', async () => {
  const requests: Request[] = [];
  const secret = 'transient-test-credential';
  const client = createManagementApiClient(async (request) => {
    requests.push(request);
    return Response.json(
      { code: 'forbidden', message: secret, request_id: secret },
      { status: 403 },
    );
  });
  const api = createIntegrationsApi(client);
  const intent = {
    id: '0199a6f4-d56c-7440-9aa2-6a320f862795',
    expected_metadata_version: 1,
    expected_revision: 0,
    action: {
      operation: 'credential' as const,
      kind: 'provision' as const,
      reference: { provider: 'installed', reference: 'reader' },
      activate_connection: false,
    },
  };
  const failure = await api
    .credential(intent, secret, { 'Idempotency-Key': 'one-operation' })
    .catch((error: unknown) => error);
  expect(JSON.stringify(failure)).not.toContain(secret);
  expect(failure).toHaveProperty('code', 'forbidden');
  expect(failure).toHaveProperty('requestId', null);
  expect((failure as Error).message).not.toContain(secret);
  const request = requests[0]!;
  expect(new URL(request.url).pathname).toBe(`/api/v1/integrations/${intent.id}/credentials`);
  expect(new URL(request.url).search).toBe('');
  expect(request.credentials).toBe('omit');
  expect(request.headers.get('If-None-Match')).toBe('*');
  expect(await request.json()).toEqual({ intent, value: secret });
});

it('uses bounded provider-neutral discovery and exact preconditioned metadata and lifecycle operations', async () => {
  const requests: Request[] = [];
  const api = createIntegrationsApi(
    createManagementApiClient(async (request) => {
      requests.push(request);
      return Response.json({});
    }),
  );
  const intent = integrationFixture('IntegrationConnectionPublication').intent;
  await api.listConnections('position');
  await api.listInstallations(null);
  await api.getConnection(intent.id);
  await api.getState(intent.id);
  await api.validate(intent);
  await api.publish(intent, { 'Idempotency-Key': 'publish' });
  await api.setState(
    {
      id: intent.id,
      expected_revision: 1,
      expected_metadata_version: 1,
      action: { operation: 'disable' },
    },
    { 'Idempotency-Key': 'disable' },
  );
  await api.check(intent.id, integrationFixture('IntegrationConnectionCheckRequest'));
  expect(requests.map((request) => [request.method, new URL(request.url).pathname])).toEqual([
    ['GET', '/api/v1/integrations'],
    ['GET', '/api/v1/integration-installations'],
    ['GET', `/api/v1/integrations/${intent.id}`],
    ['GET', `/api/v1/integrations/${intent.id}/state`],
    ['POST', `/api/v1/integrations/${intent.id}/validate`],
    ['POST', `/api/v1/integrations/${intent.id}/versions`],
    ['POST', `/api/v1/integrations/${intent.id}/state`],
    ['POST', `/api/v1/integrations/${intent.id}/checks`],
  ]);
  expect(new URL(requests[0]!.url).searchParams.get('after')).toBe('position');
  expect(new URL(requests[0]!.url).searchParams.get('limit')).toBe('25');
  expect(requests[5]!.headers.get('If-None-Match')).toBe('*');
  expect(requests[6]!.headers.get('If-Match')).toBe('"1"');
  for (const index of [0, 1, 2, 3, 4, 7])
    expect(requests[index]!.headers.has('Idempotency-Key')).toBe(false);
});

it('keeps Project access publication preconditions independent from connection state and uses scoped dependency reads', async () => {
  const requests: Request[] = [];
  const api = createIntegrationsApi(
    createManagementApiClient(async (request) => {
      requests.push(request);
      return Response.json({});
    }),
  );
  const intent = integrationFixture('IntegrationProjectAccessPublication').intent;
  await api.getProjectAccess(intent.connection_id, intent.project_id);
  await api.listProjectResources(intent.connection_id, intent.project_id);
  await api.listDependencies(intent.connection_id, intent.project_id, null);
  await api.publishProjectAccess(intent, { 'Idempotency-Key': 'grant' });
  await api.publishProjectAccess(
    { ...intent, expected_current_version: 7, resources: [] },
    { 'Idempotency-Key': 'revoke' },
  );
  expect(requests.map((r) => [r.method, new URL(r.url).pathname])).toEqual([
    ['GET', `/api/v1/integrations/${intent.connection_id}/projects/${intent.project_id}/access`],
    ['GET', `/api/v1/integrations/${intent.connection_id}/projects/${intent.project_id}/resources`],
    [
      'GET',
      `/api/v1/integrations/${intent.connection_id}/projects/${intent.project_id}/dependencies`,
    ],
    ['POST', `/api/v1/integrations/${intent.connection_id}/projects/${intent.project_id}/access`],
    ['POST', `/api/v1/integrations/${intent.connection_id}/projects/${intent.project_id}/access`],
  ]);
  expect(requests[3]!.headers.get('If-None-Match')).toBe('*');
  expect(requests[4]!.headers.get('If-Match')).toBe('"7"');
  expect(await requests[4]!.json()).toEqual({
    intent: { ...intent, expected_current_version: 7, resources: [] },
  });
  expect(requests.slice(0, 3).every((r) => !r.headers.has('Idempotency-Key'))).toBe(true);
});

it('reads the exact retained connection version through the existing generated contract', async () => {
  const requests: Request[] = [];
  const original = integrationFixture('IntegrationConnection');
  const api = createIntegrationsApi(
    createManagementApiClient(async (request) => {
      requests.push(request);
      return Response.json(original);
    }),
  );
  expect(await api.getConnectionVersion(original.id, 1)).toEqual(original);
  expect(new URL(requests[0]!.url).pathname).toBe(`/api/v1/integrations/${original.id}/versions/1`);
  expect(requests[0]!.credentials).toBe('omit');
});
