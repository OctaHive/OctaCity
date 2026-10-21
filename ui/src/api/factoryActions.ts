import type { components } from '../../.generated/api/schema';
import { ISSUE_CONSTRAINTS } from '../../.generated/api/constraints';
import { managementApi, requireManagementResponse } from './client';
import { createFactoryNodeControls } from './factoryNodeControls';
export type FactoryActionItem = components['schemas']['FactoryActionQueueItem'];
export type FactoryActionsPage = components['schemas']['FactoryActionsPage'];
export type FactoryActionObservation = components['schemas']['FactoryNodeActionObservation'];
export type FactoryActionScope = Readonly<{
  projectId?: string | undefined;
  issueId?: string | undefined;
}>;
/** Generated read contracts plus the same existing guarded control implementation. */
export function createFactoryActionsApi(client = managementApi) {
  return {
    ...createFactoryNodeControls(client),
    async list(
      scope: FactoryActionScope,
      after: string | null,
      signal?: AbortSignal,
    ): Promise<FactoryActionsPage> {
      const query = { limit: ISSUE_CONSTRAINTS.defaultPageSize, ...(after ? { after } : {}) };
      if (scope.issueId) {
        if (!scope.projectId) throw new Error('Invalid action selection');
        return requireManagementResponse(
          (
            await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/actions', {
              params: { path: { project_id: scope.projectId, issue_id: scope.issueId }, query },
              cache: 'no-store',
              signal: signal ?? null,
            })
          ).data,
        );
      }
      if (scope.projectId)
        return requireManagementResponse(
          (
            await client.GET('/api/v1/projects/{project_id}/factory-actions', {
              params: { path: { project_id: scope.projectId }, query },
              cache: 'no-store',
              signal: signal ?? null,
            })
          ).data,
        );
      return requireManagementResponse(
        (
          await client.GET('/api/v1/factory-actions', {
            params: { query },
            cache: 'no-store',
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async observe(
      run: string,
      attempt: string,
      signal?: AbortSignal,
      scope: FactoryActionScope = {},
    ): Promise<FactoryActionObservation> {
      if (scope.issueId && !scope.projectId) throw new Error('Invalid action selection');
      return requireManagementResponse(
        (
          await client.GET('/api/v1/factory-runs/{factory_run_id}/nodes/{node_attempt_id}/action', {
            params: {
              path: { factory_run_id: run, node_attempt_id: attempt },
              query: {
                ...(scope.projectId ? { project_id: scope.projectId } : {}),
                ...(scope.issueId ? { issue_id: scope.issueId } : {}),
              },
            },
            cache: 'no-store',
            signal: signal ?? null,
          })
        ).data,
      );
    },
  };
}
export type FactoryActionsApi = ReturnType<typeof createFactoryActionsApi>;
