import {
  createFactoryNodeControls,
  type FactoryNodeControlApi,
} from '../../api/factoryNodeControls';
import type { components, operations } from '../../../.generated/api/schema';
import type { IdempotencyRequestHeaders } from '../../api/client';
import type { VersionedMutationHeaders } from '../../shared/confirmedIntent';
import { managementApi, requireManagementResponse } from '../../api/client';

export type FactoryRun = components['schemas']['FactoryRunSummaryResource'];
export type FactoryDiagnosticCollection =
  operations['listFactoryDiagnostics']['parameters']['path']['collection'];
export type FactoryDiagnostic = components['schemas']['FactoryDiagnosticPage']['items'][number];
export type FactoryConfiguration = components['schemas']['FactoryConfigurationSummaryResource'];
export type FactoryConfigurationDetails = components['schemas']['FactoryConfigurationDetails'];
export type FactoryFlowDetails = components['schemas']['FactoryFlowDefinitionDetails'];
export type FactoryFilters = Omit<
  NonNullable<operations['listFactoryRuns']['parameters']['query']>,
  'after' | 'limit'
>;
export interface FactoryApi extends FactoryNodeControlApi {
  readiness(
    id: string,
    version: number,
    selection: components['schemas']['FactoryReadinessSelection'],
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryReadinessReport']>;
  listPresets(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryPresetPage']>;
  resolvePreset(
    projectId: string,
    request: components['schemas']['FactoryPresetResolutionRequest'],
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryPresetResolution']>;
  selectProfile(
    runId: string,
    inputDigest: string,
    body: components['schemas']['SelectFactoryProfileRequest'],
    headers: VersionedMutationHeaders,
  ): Promise<components['schemas']['FactoryRunControlResponse']>;
  cancelRun(
    runId: string,
    headers: VersionedMutationHeaders,
  ): Promise<components['schemas']['FactoryRunControlResponse']>;
  listDiagnostics(
    runId: string,
    collection: FactoryDiagnosticCollection,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryDiagnosticPage']>;
  admitWork(
    projectId: string,
    request: components['schemas']['AdmitManualFactoryWorkRequest'],
    headers: IdempotencyRequestHeaders,
  ): Promise<components['schemas']['FactoryAdmissionResponse']>;
  validateConfiguration(
    request: components['schemas']['CreateFactoryConfigurationRequest'],
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryConfigurationValidation']>;
  validateFlow(
    request: components['schemas']['CreateFactoryFlowDefinitionRequest'],
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryFlowDefinitionValidation']>;
  createConfiguration(
    request: components['schemas']['CreateFactoryConfigurationRequest'],
    headers: IdempotencyRequestHeaders,
  ): Promise<components['schemas']['FactoryConfigurationMutationResponse']>;
  replaceConfiguration(
    id: string,
    request: components['schemas']['ReplaceFactoryConfigurationRequest'],
    headers: VersionedMutationHeaders,
  ): Promise<components['schemas']['FactoryConfigurationMutationResponse']>;
  createFlow(
    request: components['schemas']['CreateFactoryFlowDefinitionRequest'],
    headers: IdempotencyRequestHeaders,
  ): Promise<components['schemas']['FactoryFlowDefinitionMutationResponse']>;
  replaceFlow(
    id: string,
    request: components['schemas']['ReplaceFactoryFlowDefinitionRequest'],
    headers: VersionedMutationHeaders,
  ): Promise<components['schemas']['FactoryFlowDefinitionMutationResponse']>;
  currentConfiguration(id: string, signal?: AbortSignal): Promise<FactoryConfigurationDetails>;
  currentFlow(id: string, signal?: AbortSignal): Promise<FactoryFlowDetails>;
  listChoices(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryConfigurationChoicePage']>;
  normalizeSchema(
    request: components['schemas']['ValidateFactoryFlowDataSchemaRequest'],
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryFlowDataSchema']>;
  listRuns(
    projectId: string | null,
    filters: FactoryFilters,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryRunSummaryPage']>;
  listConfigurations(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryConfigurationSummaryPage']>;
  getRun(id: string, signal?: AbortSignal): Promise<FactoryRun>;
  getConfiguration(
    id: string,
    version: number,
    signal?: AbortSignal,
  ): Promise<FactoryConfigurationDetails>;
  listPhaseReady(
    projectId: string,
    digest: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryPhaseReadyPage']>;
  listFlows(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryFlowDefinitionPage']>;
  getFlow(id: string, version: number, signal?: AbortSignal): Promise<FactoryFlowDetails>;
}

/** Authorization-filtered, bounded same-origin Factory reads. */
export const factoryApi: FactoryApi = {
  async readiness(id, version, body, signal) {
    const { data } = await managementApi.POST(
      '/api/v1/factory-configurations/{factory_configuration_id}/versions/{version}/readiness',
      {
        params: { path: { factory_configuration_id: id, version } },
        body,
        signal: signal ?? null,
      },
    );
    return requireManagementResponse(data);
  },
  async listPresets(projectId, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/projects/{project_id}/factory-presets', {
      params: {
        path: { project_id: projectId },
        query: { limit: 25, ...(cursor === null ? {} : { after: cursor }) },
      },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async resolvePreset(projectId, body, signal) {
    const { data } = await managementApi.POST(
      '/api/v1/projects/{project_id}/factory-presets/resolve',
      {
        params: { path: { project_id: projectId } },
        body,
        signal: signal ?? null,
      },
    );
    return requireManagementResponse(data);
  },
  async selectProfile(runId, inputDigest, body, headers) {
    const { data } = await managementApi.POST(
      '/api/v1/factory-runs/{factory_run_id}/profile-selections/{input_digest}',
      {
        body,
        params: { path: { factory_run_id: runId, input_digest: inputDigest }, header: headers },
      },
    );
    return requireManagementResponse(data);
  },
  async cancelRun(runId, headers) {
    const { data } = await managementApi.POST(
      '/api/v1/factory-runs/{factory_run_id}/cancellations',
      { body: {}, params: { path: { factory_run_id: runId }, header: headers } },
    );
    return requireManagementResponse(data);
  },
  ...createFactoryNodeControls(),
  async listDiagnostics(runId, collection, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/factory-runs/{factory_run_id}/{collection}', {
      params: {
        path: { factory_run_id: runId, collection },
        query: { limit: 25, ...(cursor === null ? {} : { after: cursor }) },
      },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async admitWork(projectId, body, headers) {
    const { data } = await managementApi.POST('/api/v1/projects/{project_id}/factory-work', {
      body,
      params: { path: { project_id: projectId }, header: headers },
    });
    return requireManagementResponse(data);
  },
  async validateConfiguration(body, signal) {
    const { data } = await managementApi.POST('/api/v1/factory-configurations/validate', {
      body,
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async validateFlow(body, signal) {
    const { data } = await managementApi.POST('/api/v1/flow-definitions/validate', {
      body,
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async createConfiguration(body, headers) {
    const { data } = await managementApi.POST('/api/v1/factory-configurations', {
      body,
      params: { header: { 'Idempotency-Key': headers['Idempotency-Key'] } },
    });
    return requireManagementResponse(data);
  },
  async replaceConfiguration(id, body, headers) {
    const { data } = await managementApi.POST(
      '/api/v1/factory-configurations/{factory_configuration_id}/versions',
      { body, params: { path: { factory_configuration_id: id }, header: headers } },
    );
    return requireManagementResponse(data);
  },
  async createFlow(body, headers) {
    const { data } = await managementApi.POST('/api/v1/flow-definitions', {
      body,
      params: { header: { 'Idempotency-Key': headers['Idempotency-Key'] } },
    });
    return requireManagementResponse(data);
  },
  async replaceFlow(id, body, headers) {
    const { data } = await managementApi.POST(
      '/api/v1/flow-definitions/{flow_definition_id}/versions',
      { body, params: { path: { flow_definition_id: id }, header: headers } },
    );
    return requireManagementResponse(data);
  },
  async currentConfiguration(id, signal) {
    const { data } = await managementApi.GET(
      '/api/v1/factory-configurations/{factory_configuration_id}',
      { params: { path: { factory_configuration_id: id } }, signal: signal ?? null },
    );
    return requireManagementResponse(data);
  },
  async currentFlow(id, signal) {
    const { data } = await managementApi.GET('/api/v1/flow-definitions/{flow_definition_id}', {
      params: { path: { flow_definition_id: id } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async listChoices(projectId, cursor, signal) {
    const { data } = await managementApi.GET(
      '/api/v1/projects/{project_id}/factory-configuration-choices',
      {
        params: {
          path: { project_id: projectId },
          query: { limit: 25, ...(cursor === null ? {} : { after: cursor }) },
        },
        signal: signal ?? null,
      },
    );
    return requireManagementResponse(data);
  },
  async normalizeSchema(body, signal) {
    const { data } = await managementApi.POST('/api/v1/flow-data-schemas/validate', {
      body,
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async listFlows(projectId, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/projects/{project_id}/flow-definitions', {
      params: {
        path: { project_id: projectId },
        query: { limit: 25, ...(cursor === null ? {} : { after: cursor }) },
      },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async getFlow(id, version, signal) {
    const { data } = await managementApi.GET(
      '/api/v1/flow-definitions/{flow_definition_id}/versions/{version}',
      { params: { path: { flow_definition_id: id, version } }, signal: signal ?? null },
    );
    return requireManagementResponse(data);
  },
  async listPhaseReady(projectId, digest, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/projects/{project_id}/factory-phase-ready', {
      params: {
        path: { project_id: projectId },
        query: { policy_digest: digest, limit: 25, ...(cursor === null ? {} : { after: cursor }) },
      },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async getRun(id, signal) {
    const { data } = await managementApi.GET('/api/v1/factory-runs/{factory_run_id}', {
      params: { path: { factory_run_id: id } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async getConfiguration(id, version, signal) {
    const { data } = await managementApi.GET(
      '/api/v1/factory-configurations/{factory_configuration_id}/versions/{version}',
      { params: { path: { factory_configuration_id: id, version } }, signal: signal ?? null },
    );
    return requireManagementResponse(data);
  },
  async listConfigurations(projectId, cursor, signal) {
    const { data } = await managementApi.GET(
      '/api/v1/projects/{project_id}/factory-configurations',
      {
        params: {
          path: { project_id: projectId },
          query: { limit: 25, ...(cursor === null ? {} : { after: cursor }) },
        },
        signal: signal ?? null,
      },
    );
    return requireManagementResponse(data);
  },
  async listRuns(projectId, filters, cursor, signal) {
    const query = { ...filters, ...(cursor === null ? {} : { after: cursor }), limit: 25 };
    const result =
      projectId === null
        ? await managementApi.GET('/api/v1/factory-runs', {
            params: { query },
            signal: signal ?? null,
          })
        : await managementApi.GET('/api/v1/projects/{project_id}/factory-runs', {
            params: { path: { project_id: projectId }, query },
            signal: signal ?? null,
          });
    return requireManagementResponse(result.data);
  },
};
