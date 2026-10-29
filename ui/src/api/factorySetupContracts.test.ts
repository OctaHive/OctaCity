import { afterEach, expect, expectTypeOf, it, vi } from 'vitest';
import type { components } from '../../.generated/api/schema';
import { FACTORY_SETUP_SCHEMAS } from '../../.generated/api/factory-setup';
import { createManagementApiClient } from './client';

type Schemas = components['schemas'];
const project = '0199a6f4-d56c-7440-9aa2-6a320f862790';
const configuration = '0199a6f4-d56c-7440-9aa2-6a320f862791';
const reference = { identity: 'triage', version: 'v1', digest: 'ab'.repeat(32) };
afterEach(() => vi.unstubAllGlobals());

it('preserves typed preparation purposes and existing Source, Role and result meanings', () => {
  const preparation: Schemas['SourceAcceptanceMapping'] = {
    kind: 'process_completion',
    value: { kind: 'draft', project_id: project, preset: reference, content: 'Reviewed criteria' },
  };
  expect(preparation.kind).toBe('process_completion');
  expect(FACTORY_SETUP_SCHEMAS.FactoryPresetResolutionRequest.required).toEqual(['preset']);
  expect(
    FACTORY_SETUP_SCHEMAS.SourceAcceptanceMapping.oneOf.map(
      (variant) => variant.properties.kind.const,
    ),
  ).toEqual(['field', 'body', 'body_section', 'process_completion']);
  expectTypeOf<Schemas['SourceAcceptanceMapping']['kind']>().toEqualTypeOf<
    'field' | 'body' | 'body_section' | 'process_completion'
  >();
  expectTypeOf<Schemas['WorkSourcePredicate']['label_presence']>().toEqualTypeOf<
    'any' | 'none' | undefined
  >();
  expectTypeOf<Schemas['FactoryPresetResolution']['completion']>().toEqualTypeOf<
    Schemas['SourceCompletionDocument']
  >();
  expectTypeOf<Schemas['FactoryPresetProfileSelection']['profile']>().toEqualTypeOf<
    Schemas['FactoryRoleProfileReference']
  >();
  expectTypeOf<Schemas['FactoryReadinessReport']['configuration']>().toEqualTypeOf<
    Schemas['FactoryConfigurationReference']
  >();
  expectTypeOf<Schemas['FactoryNodeCompletionDiagnostic']['output_schema']>().toEqualTypeOf<
    Schemas['FactoryImmutableReferenceResource']
  >();
  expectTypeOf<Schemas['WorkSourceAdmissionDiagnostic']['issue']>().toEqualTypeOf<
    Schemas['WorkSourceIssueLink'] | undefined
  >();
  expectTypeOf<Schemas['FactoryReadinessSelection']>().not.toHaveProperty('credentials');
  expectTypeOf<Schemas['WorkSourceInputDraft']>().not.toHaveProperty('issue_id');
});

it('discovers, resolves and observes exact readiness through generated query contracts', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const repository = {
    project_id: project,
    id: '0199a6f4-d56c-7440-9aa2-6a320f862792',
    version: 3,
    definition_digest: 'cd'.repeat(32),
  };
  const page: Schemas['FactoryPresetPage'] = {
    total: 1,
    items: [{ reference, profile_aliases: [], parameters: [], required_node_kinds: [] }],
    next_cursor: null,
  };
  const resolution: Schemas['FactoryPresetResolution'] = {
    preset: reference,
    completion: {
      kind: 'draft',
      project_id: project,
      preset: reference,
      content: 'Reviewed criteria',
    },
    diagnostics: [{ path: '/profiles/eligibility', reason: 'required' }],
    flow: null,
    configuration: null,
    input: null,
    dependencies: [],
  };
  const readiness: Schemas['FactoryReadinessReport'] = {
    configuration: {
      id: configuration,
      project_id: project,
      version: 2,
      definition_digest: 'ef'.repeat(32),
    },
    repository,
    flow: { id: '0199a6f4-d56c-7440-9aa2-6a320f862793', version: 1, digest: '12'.repeat(32) },
    execution: 'unknown',
    native_write: 'unknown',
    observed_at: 1000,
    expires_at: 2000,
    checks: [],
  };
  const replies = [page, resolution, readiness];
  const requests: Request[] = [];
  const client = createManagementApiClient(async (request) => {
    requests.push(request);
    return Response.json(replies.shift(), { headers: { 'cache-control': 'no-store' } });
  });
  const discovered = await client.GET('/api/v1/projects/{project_id}/factory-presets', {
    params: { path: { project_id: project } },
  });
  expect(discovered.data).toEqual(page);
  const selection: Schemas['FactoryPresetResolutionRequest'] = {
    preset: reference,
    context: { eligibility: { project_id: project, documents: {} } },
  };
  const resolved = await client.POST('/api/v1/projects/{project_id}/factory-presets/resolve', {
    params: { path: { project_id: project } },
    body: selection,
  });
  expect(resolved.data).toEqual(resolution);
  const observed = await client.POST(
    '/api/v1/factory-configurations/{factory_configuration_id}/versions/{version}/readiness',
    {
      params: { path: { factory_configuration_id: configuration, version: 2 } },
      body: { repository },
    },
  );
  expect(observed.data).toEqual(readiness);
  expect(requests.map((request) => [request.method, new URL(request.url).pathname])).toEqual([
    ['GET', `/api/v1/projects/${project}/factory-presets`],
    ['POST', `/api/v1/projects/${project}/factory-presets/resolve`],
    ['POST', `/api/v1/factory-configurations/${configuration}/versions/2/readiness`],
  ]);
  expect(await requests[1]!.json()).toEqual(selection);
  expect(await requests[2]!.json()).toEqual({ repository });
  for (const request of requests) {
    expect(request.credentials).toBe('omit');
    expect(request.headers.has('Idempotency-Key')).toBe(false);
    expect(request.headers.has('If-Match')).toBe(false);
  }
});
