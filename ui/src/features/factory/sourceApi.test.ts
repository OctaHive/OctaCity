// @vitest-environment jsdom
import { expect, it } from 'vitest';
import { createManagementApiClient } from '../../api/client';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { createFactorySourcesApi } from './sourceApi';

it('uses Project-scoped source choices, validation, read-only preview and exact preconditions without credentials or task creation', async () => {
  const requests: Request[] = [];
  const api = createFactorySourcesApi(
    createManagementApiClient(async (request) => {
      requests.push(request);
      return Response.json({});
    }),
  );
  const intent = integrationFixture('WorkSourceConfigurationSubmission').intent;
  const input = intent.inputs[0]!;
  const connection = input.definition.connection!;
  await api.list(intent.project_id, null);
  await api.choices(intent.project_id, connection.id, connection.version);
  await api.validate(intent);
  await api.preview(intent);
  await api.publish(intent, { 'Idempotency-Key': 'source' });
  await api.publish(
    { ...intent, inputs: [{ ...input, expected_version: 3 }] },
    { 'Idempotency-Key': 'replace-source' },
  );
  const group = {
    ...intent,
    inputs: [],
    group: { id: input.id, expected_version: 2, members: [{ id: input.id, version: 3 }] },
  };
  await api.publish(group, { 'Idempotency-Key': 'group' });
  expect(requests.map((r) => [r.method, new URL(r.url).pathname])).toEqual([
    ['GET', `/api/v1/projects/${intent.project_id}/factory-sources`],
    ['GET', `/api/v1/projects/${intent.project_id}/factory-source-choices`],
    ['POST', `/api/v1/projects/${intent.project_id}/factory-sources/validate`],
    ['POST', `/api/v1/projects/${intent.project_id}/factory-sources/preview`],
    ['POST', `/api/v1/projects/${intent.project_id}/factory-source-inputs/${input.id}/versions`],
    ['POST', `/api/v1/projects/${intent.project_id}/factory-source-inputs/${input.id}/versions`],
    ['POST', `/api/v1/projects/${intent.project_id}/factory-source-groups/${input.id}/versions`],
  ]);
  expect(requests[4]!.headers.get('If-None-Match')).toBe('*');
  expect(requests[5]!.headers.get('If-Match')).toBe('"3"');
  expect(requests[6]!.headers.get('If-Match')).toBe('"2"');
  for (const request of requests.slice(0, 4))
    expect(request.headers.has('Idempotency-Key')).toBe(false);
  expect(await requests[3]!.json()).toEqual({ intent, limit: 10 });
  expect(new URL(requests[1]!.url).searchParams.get('connection_id')).toBe(connection.id);
  expect(new URL(requests[1]!.url).searchParams.get('connection_version')).toBe('1');
});
