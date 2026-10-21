import type { components } from '../../../.generated/api/schema';
import {
  managementApi,
  ManagementApiError,
  redactManagementError,
  requireManagementResponse,
  versionedMutationHeaders,
  type IdempotencyRequestHeaders,
} from '../../api/client';

export type CredentialIntent = components['schemas']['IntegrationCredentialSubmission']['intent'];
export type IntegrationReceipt = components['schemas']['IntegrationAdministrationReceipt'];
export type Connection = components['schemas']['IntegrationConnection'];
export type Installation = components['schemas']['IntegrationInstallationPage']['items'][number];
export type ConnectionIntent = components['schemas']['IntegrationConnectionPublication']['intent'];
export type ProjectAccessIntent =
  components['schemas']['IntegrationProjectAccessPublication']['intent'];
export type ProjectAccess = components['schemas']['IntegrationProjectAccess'];
export type SourceRecord = components['schemas']['WorkSourceConfigurationRecord'];
export type Administration = components['schemas']['IntegrationAdministrationState'];
export type StateIntent = components['schemas']['IntegrationStateChange']['intent'];
export type CheckRequest = components['schemas']['IntegrationConnectionCheckRequest'];
export type CheckResult = components['schemas']['IntegrationConnectionCheckResult'];

/** Only closed error metadata crosses integration views; remote text can echo submitted values. */
export function safeIntegrationError(
  error: unknown,
  message = 'Integration operation failed.',
): ManagementApiError {
  return redactManagementError(error, message);
}

/** Version zero is an absent-resource precondition, not an If-Match version. */
function precondition(headers: IdempotencyRequestHeaders, version: number) {
  return version === 0
    ? { ...headers, 'If-None-Match': '*' as const }
    : versionedMutationHeaders(headers['Idempotency-Key'], version);
}
async function safeResponse<T>(request: Promise<{ data?: T }>): Promise<T> {
  try {
    return requireManagementResponse((await request).data);
  } catch (error) {
    throw safeIntegrationError(error);
  }
}

/** Credential submission is imperative: callers must never put its value in query/mutation state. */
export function createIntegrationsApi(client = managementApi) {
  return {
    listConnections(after: string | null, signal?: AbortSignal) {
      return safeResponse(
        client.GET('/api/v1/integrations', {
          params: { query: { limit: 25, ...(after === null ? {} : { after }) } },
          signal: signal ?? null,
        }),
      );
    },
    listInstallations(after: string | null, signal?: AbortSignal) {
      return safeResponse(
        client.GET('/api/v1/integration-installations', {
          params: { query: { limit: 25, ...(after === null ? {} : { after }) } },
          signal: signal ?? null,
        }),
      );
    },
    getInputDiagnostics(
      projectId: string,
      inputId: string,
      version: number,
      after: string | null,
      signal?: AbortSignal,
    ) {
      return safeResponse(
        client.GET(
          '/api/v1/projects/{project_id}/factory-source-inputs/{input_id}/versions/{version}/diagnostics',
          {
            params: {
              path: { project_id: projectId, input_id: inputId, version },
              query: { limit: 10, ...(after === null ? {} : { after }) },
            },
            signal: signal ?? null,
          },
        ),
      );
    },
    getProjectAccess(connectionId: string, projectId: string, signal?: AbortSignal) {
      return safeResponse(
        client.GET('/api/v1/integrations/{integration_id}/projects/{project_id}/access', {
          params: { path: { integration_id: connectionId, project_id: projectId } },
          signal: signal ?? null,
        }),
      );
    },
    listProjectResources(connectionId: string, projectId: string, signal?: AbortSignal) {
      return safeResponse(
        client.GET('/api/v1/integrations/{integration_id}/projects/{project_id}/resources', {
          params: { path: { integration_id: connectionId, project_id: projectId } },
          signal: signal ?? null,
        }),
      );
    },
    listDependencies(
      connectionId: string,
      projectId: string,
      after: string | null,
      signal?: AbortSignal,
    ) {
      return safeResponse(
        client.GET('/api/v1/integrations/{integration_id}/projects/{project_id}/dependencies', {
          params: {
            path: { integration_id: connectionId, project_id: projectId },
            query: { limit: 10, ...(after === null ? {} : { after }) },
          },
          signal: signal ?? null,
        }),
      );
    },
    publishProjectAccess(intent: ProjectAccessIntent, headers: IdempotencyRequestHeaders) {
      return safeResponse(
        client.POST('/api/v1/integrations/{integration_id}/projects/{project_id}/access', {
          params: {
            path: { integration_id: intent.connection_id, project_id: intent.project_id },
            header: precondition(headers, intent.expected_current_version ?? 0),
          },
          body: { intent },
        }),
      );
    },
    getConnectionVersion(id: string, version: number, signal?: AbortSignal) {
      return safeResponse(
        client.GET('/api/v1/integrations/{integration_id}/versions/{version}', {
          params: { path: { integration_id: id, version } },
          signal: signal ?? null,
        }),
      );
    },
    getConnection(id: string, signal?: AbortSignal) {
      return safeResponse(
        client.GET('/api/v1/integrations/{integration_id}', {
          params: { path: { integration_id: id } },
          signal: signal ?? null,
        }),
      );
    },
    getState(id: string, signal?: AbortSignal) {
      return safeResponse(
        client.GET('/api/v1/integrations/{integration_id}/state', {
          params: { path: { integration_id: id } },
          signal: signal ?? null,
        }),
      );
    },
    validate(intent: ConnectionIntent, signal?: AbortSignal) {
      return safeResponse(
        client.POST('/api/v1/integrations/{integration_id}/validate', {
          params: { path: { integration_id: intent.id } },
          body: { intent },
          signal: signal ?? null,
        }),
      );
    },
    publish(intent: ConnectionIntent, headers: IdempotencyRequestHeaders) {
      return safeResponse(
        client.POST('/api/v1/integrations/{integration_id}/versions', {
          params: {
            path: { integration_id: intent.id },
            header: precondition(headers, intent.expected_current_version ?? 0),
          },
          body: { intent },
        }),
      );
    },
    setState(intent: StateIntent, headers: IdempotencyRequestHeaders) {
      return safeResponse(
        client.POST('/api/v1/integrations/{integration_id}/state', {
          params: {
            path: { integration_id: intent.id },
            header: precondition(headers, intent.expected_revision),
          },
          body: { intent },
        }),
      );
    },
    check(id: string, body: CheckRequest, signal?: AbortSignal) {
      return safeResponse(
        client.POST('/api/v1/integrations/{integration_id}/checks', {
          params: { path: { integration_id: id } },
          body,
          signal: signal ?? null,
        }),
      );
    },
    async credential(
      intent: CredentialIntent,
      value: string | undefined,
      headers: IdempotencyRequestHeaders,
    ): Promise<IntegrationReceipt> {
      try {
        const { data } = await client.POST('/api/v1/integrations/{integration_id}/credentials', {
          params: {
            path: { integration_id: intent.id },
            header: precondition(headers, intent.expected_revision),
          },
          body: { intent, ...(value === undefined ? {} : { value }) },
        });
        const receipt = requireManagementResponse(data);
        // Keep only the safe receipt, including when a peer supplies undocumented properties.
        return {
          operation_id: receipt.operation_id,
          integration_id: receipt.integration_id,
          metadata_version: receipt.metadata_version,
          revision: receipt.revision,
          status: receipt.status,
          requested_at_unix_ms: receipt.requested_at_unix_ms,
        };
      } catch (error) {
        throw safeIntegrationError(error);
      }
    },
  };
}
export const integrationsApi = createIntegrationsApi();
export type IntegrationsApi = ReturnType<typeof createIntegrationsApi>;
