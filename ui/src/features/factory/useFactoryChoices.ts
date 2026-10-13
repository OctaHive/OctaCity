import { queryKeys } from '../../app/query';
import { useCursorPage } from '../../shared/PagedExplorerBranch';
import type { FactoryApi } from './api';

/** Shares the Project-scoped availability and bounded choices with draft editors. */
export function useFactoryChoices(api: FactoryApi, projectId: string) {
  return useCursorPage(queryKeys.factoryChoices(projectId), async (cursor, signal) => {
    const page = await api.listChoices(projectId, cursor, signal);
    return {
      ...page,
      items: page.items.map((choice) => ({ ...choice, id: `${choice.kind}:${choice.alias}` })),
    };
  });
}
