import { readFileSync } from 'node:fs';
import { afterEach, beforeEach, expect, expectTypeOf, it, vi } from 'vitest';
import type { components } from '../../.generated/api/schema';
import { createManagementApiClient, idempotencyHeaders, versionedMutationHeaders } from './client';

type Schemas = components['schemas'];
/** Canonical synthetic wire examples are schema-checked by the REST suite. */
function fixture<Name extends keyof Schemas>(name: Name, index = 0): Schemas[Name] {
  return JSON.parse(
    readFileSync(
      '../server/api/octacity-server-api-rest/fixtures/v1/integration-contracts.json',
      'utf8',
    ),
  )[name][index];
}
const connection = '0199a6f4-d56c-7440-9aa2-6a320f862795';
const project = '0199a6f4-d56c-7440-9aa2-6a320f862790';
const input = '0199a6f4-d56c-7440-9aa2-6a320f862791';
beforeEach(() => vi.stubGlobal('location', { origin: 'https://console.example' }));
afterEach(() => vi.unstubAllGlobals());

it('keeps generated ordinary contracts separate from write-only credential input and card commands', () => {
  expectTypeOf<Schemas['IntegrationConnection']>().not.toHaveProperty('value');
  expectTypeOf<Schemas['IntegrationAdministrationState']>().not.toHaveProperty('value');
  expectTypeOf<Schemas['IntegrationAdministrationReceipt']>().not.toHaveProperty('value');
  expectTypeOf<Schemas['IntegrationCredentialSubmission']>().toHaveProperty('value');
  expectTypeOf<Schemas['WorkSourceInputDraft']>().not.toHaveProperty('issue_id');
  expectTypeOf<Schemas['WorkSourceAdmissionDiagnostic']['issue']>().toEqualTypeOf<
    Schemas['WorkSourceIssueLink'] | undefined
  >();
  const action: Schemas['FactoryWorkStatusActionParameters'] = fixture(
    'FactoryWorkStatusActionParameters',
  );
  expect(action).toEqual({
    status: 'needs_context',
    outbox: { max_attempts: 3, retry_delay_millis: 250 },
  });
});

it('uses typed connection, access, check and credential contracts without provider-specific routes', async () => {
  const requests: Request[] = [];
  const replies = [
    fixture('IntegrationConnection'),
    { disposition: 'applied', access: fixture('IntegrationProjectAccess') },
    fixture('IntegrationConnectionCheckResult'),
    fixture('IntegrationAdministrationReceipt'),
  ];
  const client = createManagementApiClient(async (request) => {
    requests.push(request);
    return Response.json(replies.shift(), { headers: { 'cache-control': 'no-store' } });
  });
  const metadata = await client.GET('/api/v1/integrations/{integration_id}', {
    params: { path: { integration_id: connection } },
  });
  expect(metadata.data?.definition.provider).toBe('jira');
  const headers = versionedMutationHeaders('reviewed-intent', 1);
  await client.POST('/api/v1/integrations/{integration_id}/projects/{project_id}/access', {
    params: {
      path: { integration_id: connection, project_id: project },
      header: { ...idempotencyHeaders('create-access'), 'If-None-Match': '*' },
    },
    body: fixture('IntegrationProjectAccessPublication'),
  });
  await client.POST('/api/v1/integrations/{integration_id}/checks', {
    params: { path: { integration_id: connection } },
    body: fixture('IntegrationConnectionCheckRequest'),
  });
  const receipt = await client.POST('/api/v1/integrations/{integration_id}/credentials', {
    params: { path: { integration_id: connection }, header: headers },
    body: fixture('IntegrationCredentialSubmission', 2),
  });
  expect(receipt.data).toEqual(fixture('IntegrationAdministrationReceipt'));
  expect(JSON.stringify(receipt.data)).not.toContain('documentation-placeholder');
  expect(await requests[3]!.json()).toEqual(fixture('IntegrationCredentialSubmission', 2));
  expect(requests.every((request) => request.credentials === 'omit')).toBe(true);
  expect(requests[1]!.headers.get('If-None-Match')).toBe('*');
  expect(requests[3]!.headers.get('If-Match')).toBe('"1"');
  expect(requests[2]!.headers.has('Idempotency-Key')).toBe(false);
});

it('reads native Work/Run diagnostics with or without a card and previews without mutation headers', async () => {
  const requests: Request[] = [];
  const replies = [
    fixture('WorkSourcePreview'),
    fixture('WorkSourceDiagnostics'),
    fixture('WorkSourceDiagnostics', 1),
  ];
  const client = createManagementApiClient(async (request) => {
    requests.push(request);
    return Response.json(replies.shift());
  });
  const preview = await client.POST('/api/v1/projects/{project_id}/factory-sources/preview', {
    params: { path: { project_id: project } },
    body: fixture('WorkSourcePreviewSubmission'),
  });
  expect(preview.data?.candidates[0]?.disposition).toBe('selected');
  for (const linked of [false, true]) {
    const result = await client.GET(
      '/api/v1/projects/{project_id}/factory-source-inputs/{input_id}/versions/{version}/diagnostics',
      {
        params: { path: { project_id: project, input_id: input, version: 1 }, query: { limit: 2 } },
      },
    );
    const admission = result.data?.admissions[0];
    expect(admission?.association.native_correlation).toBe('22'.repeat(32));
    expect(admission?.association.run_id).toBe(connection);
    expect(admission?.reporting[0]?.state).toBe('pending');
    expect(admission?.issue?.issue_id).toBe(linked ? 'internal-task-123' : undefined);
  }
  expect(
    requests.every(
      (request) => !request.headers.has('Idempotency-Key') && !request.headers.has('If-Match'),
    ),
  ).toBe(true);
});
