import { OPERATOR_ATTENTION_CONSTRAINTS } from '../../.generated/api/constraints';
import type { components } from '../../.generated/api/schema';
import { managementApi, requireManagementResponse } from './client';

export type OperatorAttentionItem = components['schemas']['OperatorAttentionItem'];
export type OperatorAttentionPage = components['schemas']['OperatorAttentionPage'];
export type OperatorAttentionTargetKind = components['schemas']['OperatorAttentionTargetKind'];

export interface OperatorAttentionScope {
  readonly agentIds: readonly string[];
  readonly buildIds: readonly string[];
  readonly includeCriticalConditions: boolean;
  readonly poolIds: readonly string[];
}

export interface OperatorAttentionApi {
  listOperatorAttention(
    scope: OperatorAttentionScope,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<OperatorAttentionPage>;
}

/** Typed bounded read for favorite-resource transitions and server-classified critical conditions. */
export const operatorAttentionApi: OperatorAttentionApi = {
  async listOperatorAttention(scope, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/operator-attention', {
      params: {
        query: {
          ...(scope.agentIds.length === 0 ? {} : { agent_ids: [...scope.agentIds] }),
          ...(scope.buildIds.length === 0 ? {} : { build_ids: [...scope.buildIds] }),
          ...(scope.poolIds.length === 0 ? {} : { pool_ids: [...scope.poolIds] }),
          ...(cursor === null ? {} : { after: cursor }),
          include_critical_conditions: scope.includeCriticalConditions,
          limit: OPERATOR_ATTENTION_CONSTRAINTS.defaultPageSize,
        },
      },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
};
