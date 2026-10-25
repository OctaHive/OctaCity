import {
  managementApi,
  requireManagementResponse,
  type IdempotencyRequestHeaders,
} from '../../api/client';
import type { components, paths } from '../../../.generated/api/schema';

type SettingsRequest = components['schemas']['FactoryRuntimeSettingsRequest'];
type ChoiceQuery =
  paths['/api/v1/projects/{project_id}/factory-runtime-choices']['get']['parameters']['query'];

/** Reviewed Project configuration uses generated contracts and the ordinary management client. */
export function createFactoryRuntimeApi(client = managementApi) {
  return {
    async choices(project: string, query: ChoiceQuery = {}, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/factory-runtime-choices', {
            params: { path: { project_id: project }, query },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async current(project: string, alias: string, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/factory-runtimes/{alias}', {
            params: { path: { project_id: project, alias } },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async version(project: string, alias: string, version: number, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/factory-runtimes/{alias}/versions/{version}',
            {
              params: { path: { project_id: project, alias, version } },
              signal: signal ?? null,
            },
          )
        ).data,
      );
    },
    async validate(project: string, alias: string, body: SettingsRequest) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/factory-runtimes/{alias}/validate', {
            params: { path: { project_id: project, alias } },
            body,
          })
        ).data,
      );
    },
    async publish(
      project: string,
      alias: string,
      body: SettingsRequest,
      headers: IdempotencyRequestHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/factory-runtimes/{alias}', {
            params: { path: { project_id: project, alias }, header: headers },
            body,
          })
        ).data,
      );
    },
  };
}
export const factoryRuntimeApi = createFactoryRuntimeApi();
