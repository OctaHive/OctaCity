import type { components } from '../../../.generated/api/schema';
import { AUDIT_QUERY_CONSTRAINTS } from '../../../.generated/api/constraints';
import { managementApi, requireManagementResponse } from '../../api/client';

export type AuditActorKind = components['schemas']['AuditActorKind'];
export type AuditFactPage = components['schemas']['AuditFactPage'];
export type AuditFactResource = components['schemas']['AuditFactResource'];

export interface AuditFilters {
  actorIdentity: string | null;
  actorKind: AuditActorKind | null;
  occurredFromUnixMs: number | null;
  occurredThroughUnixMs: number | null;
  operation: string | null;
  requestIdentity: string | null;
  targetIdentity: string | null;
  targetKind: string | null;
}

export interface AuditApi {
  listAuditFacts(
    filters: Readonly<AuditFilters>,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<AuditFactPage>;
}

/** Typed same-origin REST boundary for immutable audit evidence. */
export const auditApi: AuditApi = {
  async listAuditFacts(filters, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/audit-facts', {
      params: {
        query: {
          ...(filters.actorKind === null ? {} : { actor_kind: filters.actorKind }),
          ...(filters.actorIdentity === null ? {} : { actor_identity: filters.actorIdentity }),
          ...(filters.operation === null ? {} : { operation: filters.operation }),
          ...(filters.targetKind === null ? {} : { target_kind: filters.targetKind }),
          ...(filters.targetIdentity === null ? {} : { target_identity: filters.targetIdentity }),
          ...(filters.requestIdentity === null
            ? {}
            : { request_identity: filters.requestIdentity }),
          ...(filters.occurredFromUnixMs === null
            ? {}
            : { occurred_from_unix_ms: filters.occurredFromUnixMs }),
          ...(filters.occurredThroughUnixMs === null
            ? {}
            : { occurred_through_unix_ms: filters.occurredThroughUnixMs }),
          ...(cursor === null ? {} : { after: cursor }),
          limit: AUDIT_QUERY_CONSTRAINTS.defaultPageSize,
        },
      },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
};
