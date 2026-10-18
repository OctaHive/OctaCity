import type { components } from '../../../.generated/api/schema';
import {
  managementApi,
  redactManagementError,
  requireManagementResponse,
  versionedMutationHeaders,
  type IdempotencyRequestHeaders,
} from '../../api/client';

export type SourceIntent = components['schemas']['WorkSourceConfigurationIntent'];
export type SourceRecord = components['schemas']['WorkSourceConfigurationRecord'];
export type InputChoice = components['schemas']['WorkSourceInputChoices']['items'][number];
export type SourceSnapshot = { records: SourceRecord[]; connections: SourceConnection[] };
export type SourceConnection = components['schemas']['IntegrationConnection'];

/** Guided input transport carries public references only; credentials stay in Integrations. */
export function createFactorySourcesApi(client = managementApi) {
  return {
    async diagnostics(
      projectId: string,
      target: { kind: 'input' | 'group'; id: string; version: number },
      after: string | null,
      signal?: AbortSignal,
    ) {
      try {
        const query = { limit: 10, ...(after === null ? {} : { after }) };
        return requireManagementResponse(
          (target.kind === 'input'
            ? await client.GET(
                '/api/v1/projects/{project_id}/factory-source-inputs/{input_id}/versions/{version}/diagnostics',
                {
                  params: {
                    path: { project_id: projectId, input_id: target.id, version: target.version },
                    query,
                  },
                  signal: signal ?? null,
                },
              )
            : await client.GET(
                '/api/v1/projects/{project_id}/factory-source-groups/{group_id}/versions/{version}/diagnostics',
                {
                  params: {
                    path: { project_id: projectId, group_id: target.id, version: target.version },
                    query,
                  },
                  signal: signal ?? null,
                },
              )
          ).data,
        );
      } catch (error) {
        throw redactManagementError(error, 'The source operation could not be completed.');
      }
    },
    async connections(after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/integrations', {
            params: { query: { limit: 25, ...(after === null ? {} : { after }) } },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async list(projectId: string, after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/factory-sources', {
            params: {
              path: { project_id: projectId },
              query: { limit: 25, ...(after === null ? {} : { after }) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async choices(projectId: string, connectionId: string, version: number, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/factory-source-choices', {
            params: {
              path: { project_id: projectId },
              query: { connection_id: connectionId, connection_version: version },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async validate(intent: SourceIntent, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/factory-sources/validate', {
            params: { path: { project_id: intent.project_id } },
            body: { intent },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async preview(intent: SourceIntent, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/factory-sources/preview', {
            params: { path: { project_id: intent.project_id } },
            body: { intent, limit: 10 },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async publish(intent: SourceIntent, headers: IdempotencyRequestHeaders) {
      const version =
        intent.group?.expected_version ??
        (intent.group ? null : intent.inputs[0]?.expected_version);
      const precondition =
        version == null
          ? { ...headers, 'If-None-Match': '*' as const }
          : versionedMutationHeaders(headers['Idempotency-Key'], version);
      if (intent.group)
        return requireManagementResponse(
          (
            await client.POST(
              '/api/v1/projects/{project_id}/factory-source-groups/{group_id}/versions',
              {
                params: {
                  path: { project_id: intent.project_id, group_id: intent.group.id },
                  header: precondition,
                },
                body: { intent },
              },
            )
          ).data,
        );
      const input = intent.inputs[0];
      if (!input || intent.inputs.length !== 1)
        throw new Error('An input publication requires exactly one input.');
      return requireManagementResponse(
        (
          await client.POST(
            '/api/v1/projects/{project_id}/factory-source-inputs/{input_id}/versions',
            {
              params: {
                path: { project_id: intent.project_id, input_id: input.id },
                header: precondition,
              },
              body: { intent },
            },
          )
        ).data,
      );
    },
  };
}
export const factorySourcesApi = createFactorySourcesApi();
export type FactorySourcesApi = ReturnType<typeof createFactorySourcesApi>;
