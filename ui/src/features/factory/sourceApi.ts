import type { components } from '../../../.generated/api/schema';
import {
  managementApi,
  requireManagementResponse,
  versionedMutationHeaders,
  type IdempotencyRequestHeaders,
} from '../../api/client';

import { sourceRequest } from '../../shared/sourceErrors';

async function sourceResponse<T>(request: Promise<{ data?: T }>): Promise<T> {
  return sourceRequest(request.then((response) => requireManagementResponse(response.data)));
}

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
      const query = { limit: 10, ...(after === null ? {} : { after }) };
      return sourceResponse(
        target.kind === 'input'
          ? client.GET(
              '/api/v1/projects/{project_id}/factory-source-inputs/{input_id}/versions/{version}/diagnostics',
              {
                params: {
                  path: { project_id: projectId, input_id: target.id, version: target.version },
                  query,
                },
                signal: signal ?? null,
              },
            )
          : client.GET(
              '/api/v1/projects/{project_id}/factory-source-groups/{group_id}/versions/{version}/diagnostics',
              {
                params: {
                  path: { project_id: projectId, group_id: target.id, version: target.version },
                  query,
                },
                signal: signal ?? null,
              },
            ),
      );
    },
    async connections(after: string | null, signal?: AbortSignal) {
      return sourceResponse(
        client.GET('/api/v1/integrations', {
          params: { query: { limit: 25, ...(after === null ? {} : { after }) } },
          signal: signal ?? null,
        }),
      );
    },
    async list(projectId: string, after: string | null, signal?: AbortSignal) {
      return sourceResponse(
        client.GET('/api/v1/projects/{project_id}/factory-sources', {
          params: {
            path: { project_id: projectId },
            query: { limit: 25, ...(after === null ? {} : { after }) },
          },
          signal: signal ?? null,
        }),
      );
    },
    async choices(projectId: string, connectionId: string, version: number, signal?: AbortSignal) {
      return sourceResponse(
        client.GET('/api/v1/projects/{project_id}/factory-source-choices', {
          params: {
            path: { project_id: projectId },
            query: { connection_id: connectionId, connection_version: version },
          },
          signal: signal ?? null,
        }),
      );
    },
    async validate(intent: SourceIntent, signal?: AbortSignal) {
      return sourceResponse(
        client.POST('/api/v1/projects/{project_id}/factory-sources/validate', {
          params: { path: { project_id: intent.project_id } },
          body: { intent },
          signal: signal ?? null,
        }),
      );
    },
    async preview(intent: SourceIntent, signal?: AbortSignal) {
      return sourceResponse(
        client.POST('/api/v1/projects/{project_id}/factory-sources/preview', {
          params: { path: { project_id: intent.project_id } },
          body: { intent, limit: 10 },
          signal: signal ?? null,
        }),
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
        return sourceResponse(
          client.POST('/api/v1/projects/{project_id}/factory-source-groups/{group_id}/versions', {
            params: {
              path: { project_id: intent.project_id, group_id: intent.group.id },
              header: precondition,
            },
            body: { intent },
          }),
        );
      const input = intent.inputs[0];
      if (!input || intent.inputs.length !== 1)
        throw new Error('An input publication requires exactly one input.');
      return sourceResponse(
        client.POST('/api/v1/projects/{project_id}/factory-source-inputs/{input_id}/versions', {
          params: {
            path: { project_id: intent.project_id, input_id: input.id },
            header: precondition,
          },
          body: { intent },
        }),
      );
    },
  };
}
export const factorySourcesApi = createFactorySourcesApi();
export type FactorySourcesApi = ReturnType<typeof createFactorySourcesApi>;
