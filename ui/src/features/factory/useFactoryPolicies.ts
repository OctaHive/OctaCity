import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import { factoryPolicyApi, type FactoryPolicyApi, type PolicyRef } from './policyApi';

export function useFactoryPolicyCatalog(project: string, api: FactoryPolicyApi = factoryPolicyApi) {
  return useInfiniteQuery({
    queryKey: queryKeys.factoryPolicies(project),
    initialPageParam: null as PolicyRef | null,
    queryFn: ({ pageParam, signal }) => api.list(project, pageParam, signal),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}

export function useFactoryPolicy(
  project: string,
  selected: PolicyRef | null,
  api: FactoryPolicyApi = factoryPolicyApi,
) {
  return useQuery({
    queryKey: queryKeys.factoryPolicy(project, selected ?? {}),
    queryFn: ({ signal }) =>
      selected
        ? api.get(project, selected, signal)
        : Promise.reject(new Error('Policy selection is missing')),
    enabled: selected !== null,
  });
}

export function policyVersionKey({ reference }: PolicyRef) {
  return `${reference.id}:${reference.version}:${reference.digest}`;
}
