import type { components } from '../../.generated/api/schema';
import type { VersionedMutationHeaders } from '../shared/confirmedIntent';
import { managementApi, requireManagementResponse } from './client';
/** Shared existing guarded response/operation/retry contracts; task versions grant no authority. */
export interface FactoryNodeControlApi {
  retryNode(
    runId: string,
    nodeId: string,
    headers: VersionedMutationHeaders,
  ): Promise<components['schemas']['FactoryRunControlResponse']>;
  respondNode(
    runId: string,
    nodeId: string,
    request: components['schemas']['FactoryNodeResponseRequest'],
    headers: VersionedMutationHeaders,
  ): Promise<components['schemas']['FactoryRunControlResponse']>;
  requestNodeOperation(
    runId: string,
    nodeId: string,
    request: components['schemas']['FactoryNodeOperationRequest'],
    headers: VersionedMutationHeaders,
  ): Promise<components['schemas']['FactoryRunControlResponse']>;
}
/** One transport implementation used by advanced Run controls and the shared action views. */
export function createFactoryNodeControls(client = managementApi): FactoryNodeControlApi {
  return {
    async retryNode(runId, nodeId, headers) {
      const { data } = await client.POST(
        '/api/v1/factory-runs/{factory_run_id}/nodes/{node_attempt_id}/retries',
        {
          body: {},
          params: { path: { factory_run_id: runId, node_attempt_id: nodeId }, header: headers },
        },
      );
      return requireManagementResponse(data);
    },
    async respondNode(runId, nodeId, body, headers) {
      const { data } = await client.POST(
        '/api/v1/factory-runs/{factory_run_id}/nodes/{node_attempt_id}/responses',
        {
          body,
          params: { path: { factory_run_id: runId, node_attempt_id: nodeId }, header: headers },
        },
      );
      return requireManagementResponse(data);
    },
    async requestNodeOperation(runId, nodeId, body, headers) {
      const { data } = await client.POST(
        '/api/v1/factory-runs/{factory_run_id}/nodes/{node_attempt_id}/requests',
        {
          body,
          params: { path: { factory_run_id: runId, node_attempt_id: nodeId }, header: headers },
        },
      );
      return requireManagementResponse(data);
    },
  };
}
