import { factoryApi } from './api';
import type { components } from '../../../.generated/api/schema';
import {
  managementApi,
  requireManagementResponse,
  type IdempotencyRequestHeaders,
} from '../../api/client';

type S = components['schemas'];
export type PolicyRecord = S['FactoryProjectPolicyRecord'];
export type PolicyRef = S['FactoryProjectPolicyRef'];
export type PolicyKind = PolicyRef['kind'];

/** Exact Project-owned publications; reading never provisions defaults or grants execution. */
export function createFactoryPolicyApi(client = managementApi) {
  return {
    choices: factoryApi.listChoices,
    async list(project_id: string, after: PolicyRef | null, signal?: AbortSignal) {
      const { data } = await client.GET('/api/v1/projects/{project_id}/factory-policies', {
        params: { path: { project_id }, query: after ? { after: JSON.stringify(after) } : {} },
        signal: signal ?? null,
      });
      return requireManagementResponse(data);
    },
    async get(project_id: string, selected: PolicyRef, signal?: AbortSignal) {
      const { data } = await client.GET(
        '/api/v1/projects/{project_id}/factory-policies/{kind}/{policy_id}/versions/{version}',
        {
          params: {
            path: {
              project_id,
              kind: selected.kind,
              policy_id: selected.reference.id,
              version: selected.reference.version,
            },
            query: { digest: selected.reference.digest },
          },
          signal: signal ?? null,
        },
      );
      return requireManagementResponse(data);
    },
    async resolveLegacy(
      project_id: string,
      body: S['FactoryLegacyPolicyReference'],
      signal?: AbortSignal,
    ) {
      const { data } = await client.POST(
        '/api/v1/projects/{project_id}/factory-policies/legacy/resolve',
        {
          params: { path: { project_id } },
          body,
          signal: signal ?? null,
        },
      );
      return requireManagementResponse(data);
    },
    async defaults(project_id: string, signal?: AbortSignal) {
      const { data } = await client.GET('/api/v1/projects/{project_id}/factory-policies/defaults', {
        params: { path: { project_id } },
        signal: signal ?? null,
      });
      return requireManagementResponse(data);
    },
    async validate(project_id: string, body: S['FactoryProjectPolicyDraft']) {
      const { data } = await client.POST(
        '/api/v1/projects/{project_id}/factory-policies/validate',
        { params: { path: { project_id } }, body },
      );
      return requireManagementResponse(data);
    },
    async publish(
      project_id: string,
      body: S['FactoryProjectPolicyDraft'],
      headers: IdempotencyRequestHeaders,
    ) {
      const { data } = await client.POST('/api/v1/projects/{project_id}/factory-policies', {
        params: { path: { project_id }, header: headers },
        body,
      });
      return requireManagementResponse(data);
    },
    async selectDefaults(
      project_id: string,
      body: S['FactoryProjectPolicyDefaultsDraft'],
      headers: IdempotencyRequestHeaders,
    ) {
      const { data } = await client.POST(
        '/api/v1/projects/{project_id}/factory-policies/defaults',
        { params: { path: { project_id }, header: headers }, body },
      );
      return requireManagementResponse(data);
    },
  };
}
export const factoryPolicyApi = createFactoryPolicyApi();
export type FactoryPolicyApi = typeof factoryPolicyApi;
