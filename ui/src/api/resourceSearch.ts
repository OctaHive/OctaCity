import { RESOURCE_SEARCH_CONSTRAINTS } from '../../.generated/api/constraints';
import type { components } from '../../.generated/api/schema';
import { managementApi, requireManagementResponse } from './client';

export type ResourceSearchKind = components['schemas']['ResourceSearchKind'];
export type ResourceSearchPage = components['schemas']['ResourceSearchPage'];
export type ResourceSearchResult = components['schemas']['ResourceSearchResult'];

/** Bounded typed search shared by the global and context-scoped command-center entry points. */
export interface ResourceSearchApi {
  searchResources(
    query: string,
    kinds: readonly ResourceSearchKind[],
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<ResourceSearchPage>;
}

export const resourceSearchApi: ResourceSearchApi = {
  async searchResources(query, kinds, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/search', {
      params: {
        query: {
          ...(cursor === null ? {} : { after: cursor }),
          ...(kinds.length === 0 ? {} : { kinds: [...kinds] }),
          limit: RESOURCE_SEARCH_CONSTRAINTS.defaultPageSize,
          query,
        },
      },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
};

/** Measures the UTF-8 representation used by the server's declared search bound. */
export function resourceSearchQueryBytes(query: string): number {
  return new TextEncoder().encode(query).length;
}
