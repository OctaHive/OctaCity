import { afterEach, expect, it, vi } from 'vitest';
import { createManagementApiClient } from '../../api/client';
import { createFactoryPolicyApi } from './policyApi';

afterEach(() => vi.unstubAllGlobals());
it('validates and publishes the exact policy precondition through ordinary management transport', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const requests: Request[] = [];
  const fetch = vi.fn(async (request: Request) => {
    requests.push(request);
    return Response.json({ policy: {} });
  });
  const api = createFactoryPolicyApi(createManagementApiClient(fetch));
  const body = {
    kind: 'launch' as const,
    id: 'policy',
    expected_current: { project_id: 'project', id: 'policy', version: 3, digest: 'a'.repeat(64) },
    definition: { name: 'Manual only', allow_manual: true, allow_automatic_source: false },
  };
  await api.validate('project', body);
  expect(requests).toHaveLength(1);
  expect(new URL(requests[0]!.url).pathname).toBe(
    '/api/v1/projects/project/factory-policies/validate',
  );
  await api.publish('project', body, { 'Idempotency-Key': 'confirmed-policy' });
  expect(new URL(requests[1]!.url).pathname).toBe('/api/v1/projects/project/factory-policies');
  expect(requests[1]!.method).toBe('POST');
  expect(requests[1]!.headers.get('Idempotency-Key')).toBe('confirmed-policy');
  expect(await requests[1]!.json()).toEqual(body);
  expect(requests[1]!.credentials).toBe('omit');
});

it('reads an exact policy version with its digest instead of resolving a current head', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const fetch = vi.fn<(request: Request) => Promise<Response>>(async () =>
    Response.json({ policy: {} }),
  );
  const api = createFactoryPolicyApi(createManagementApiClient(fetch));
  await api.get('project', {
    kind: 'access',
    reference: { project_id: 'project', id: 'exact', version: 2, digest: 'b'.repeat(64) },
  });
  const url = new URL(fetch.mock.calls[0]![0].url);
  expect(url.pathname).toBe('/api/v1/projects/project/factory-policies/access/exact/versions/2');
  expect(url.searchParams.get('digest')).toBe('b'.repeat(64));
});

it('changes defaults with the exact version precondition and no policy publication', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const fetch = vi.fn<(request: Request) => Promise<Response>>(async () =>
    Response.json({ defaults: null }),
  );
  const api = createFactoryPolicyApi(createManagementApiClient(fetch));
  const body = { expected_version: 7, launch_policy: null, access_profile: null };
  await api.selectDefaults('project', body, { 'Idempotency-Key': 'default-review' });
  expect(fetch).toHaveBeenCalledTimes(1);
  const request = fetch.mock.calls[0]![0];
  expect(new URL(request.url).pathname).toBe('/api/v1/projects/project/factory-policies/defaults');
  expect(request.headers.get('Idempotency-Key')).toBe('default-review');
  expect(await request.json()).toEqual(body);
});

it('looks up only the exact retained legacy binding through the read-only resolver', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const fetch = vi.fn<(request: Request) => Promise<Response>>(async () =>
    Response.json({ mapping: null }),
  );
  const api = createFactoryPolicyApi(createManagementApiClient(fetch));
  const legacy = {
    kind: 'launch' as const,
    reference: { identity: 'installed', version: 'v1', digest: 'e'.repeat(64) },
  };
  expect(await api.resolveLegacy('project', legacy)).toEqual({ mapping: null });
  const request = fetch.mock.calls[0]![0];
  expect(new URL(request.url).pathname).toBe(
    '/api/v1/projects/project/factory-policies/legacy/resolve',
  );
  expect(await request.json()).toEqual(legacy);
  expect(request.headers.has('Idempotency-Key')).toBe(false);
});
