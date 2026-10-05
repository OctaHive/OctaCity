import type { components } from '../../../.generated/api/schema';
import {
  managementApi,
  requestIdFromResponse,
  requireManagementResponse,
  type IdempotencyRequestHeaders,
} from '../../api/client';

export type AgentPage = components['schemas']['AgentPage'];
export type AgentPoolPage = components['schemas']['AgentPoolPage'];
export type AgentPoolResource = components['schemas']['AgentPoolResource'];
export type AgentResource = components['schemas']['AgentResource'];
export type AgentMutationResponse = components['schemas']['AgentMutationResponse'];
export type DrainAgentRequest = components['schemas']['DrainAgentRequest'];
export type ReassignAgentPoolRequest = components['schemas']['ReassignAgentPoolRequest'];

type VersionedMutationHeaders = Readonly<IdempotencyRequestHeaders & { 'If-Match': string }>;

/** Agent command result paired with its safe server request identity for audit correlation. */
export interface AgentCommandResult {
  requestId: string | null;
  response: AgentMutationResponse;
}

const CAPACITY_PAGE_SIZE = 25;

/** Bounded capacity reads shared by the explorer and deep-linked details. */
export interface CapacityApi {
  drainAgent(
    agentId: string,
    request: Readonly<DrainAgentRequest>,
    headers: VersionedMutationHeaders,
  ): Promise<AgentCommandResult>;
  getAgent(agentId: string, signal?: AbortSignal): Promise<AgentResource>;
  getAgentPool(poolId: string, signal?: AbortSignal): Promise<AgentPoolResource>;
  listAgentPools(cursor: string | null, signal?: AbortSignal): Promise<AgentPoolPage>;
  listAgents(poolId: string, cursor: string | null, signal?: AbortSignal): Promise<AgentPage>;
  reassignAgentPool(
    agentId: string,
    request: Readonly<ReassignAgentPoolRequest>,
    headers: VersionedMutationHeaders,
  ): Promise<AgentCommandResult>;
}

/** Typed same-origin REST boundary for Agent capacity discovery. */
export const capacityApi: CapacityApi = {
  async drainAgent(agentId, request, headers) {
    const { data, response } = await managementApi.POST('/api/v1/agents/{agent_id}/drain', {
      body: request,
      params: { header: headers, path: { agent_id: agentId } },
    });
    return {
      requestId: requestIdFromResponse(response),
      response: requireManagementResponse(data),
    };
  },
  async getAgent(agentId, signal) {
    const { data } = await managementApi.GET('/api/v1/agents/{agent_id}', {
      params: { path: { agent_id: agentId } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async getAgentPool(poolId, signal) {
    const { data } = await managementApi.GET('/api/v1/agent-pools/{pool_id}', {
      params: { path: { pool_id: poolId } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async listAgentPools(cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/agent-pools', {
      params: { query: pageQuery(cursor) },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async listAgents(poolId, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/agents', {
      params: { query: { ...pageQuery(cursor), pool_id: poolId } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async reassignAgentPool(agentId, request, headers) {
    const { data, response } = await managementApi.POST('/api/v1/agents/{agent_id}/pool', {
      body: request,
      params: { header: headers, path: { agent_id: agentId } },
    });
    return {
      requestId: requestIdFromResponse(response),
      response: requireManagementResponse(data),
    };
  },
};

function pageQuery(cursor: string | null) {
  return {
    ...(cursor === null ? {} : { after: cursor }),
    limit: CAPACITY_PAGE_SIZE,
  };
}
