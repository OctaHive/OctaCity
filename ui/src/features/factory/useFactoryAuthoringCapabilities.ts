import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import type { FlowAuthoringServices } from './FlowPrimitives';

export function useFactoryAuthoringCapabilities(
  services: FlowAuthoringServices | undefined,
  enabled = true,
  staleTime?: number,
) {
  return useQuery({
    queryKey: queryKeys.factoryAuthoringCapabilities(services?.projectId ?? ''),
    queryFn: ({ signal }) => services!.api.getAuthoringCapabilities(services!.projectId, signal),
    enabled: !!services && enabled,
    ...(staleTime === undefined ? {} : { staleTime }),
  });
}
